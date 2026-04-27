// shared_mem.cpp â€” Kernel-mode reader for the PhoneMike shared ring buffer.
//
// The PC client creates a file-backed mapping at C:\ProgramData\PhoneMike\ring.dat.
// This module maps that file into kernel space and reads PCM data from the ring.
//
// Layout (28-byte header, pack 1):
//   [0..4]   Magic "PHMC"  (0x434D4850 LE)
//   [4..8]   SampleRate u32
//   [8..10]  Channels u16
//   [10..12] Bits u16
//   [12..16] RingCapacity u32
//   [16..20] WriteIndex i32 (monotonic, atomically updated by pc-client)
//   [20..24] ReadIndex i32 (monotonic, atomically updated by driver)
//   [24..28] Running i32 (1 = active)
//   [28..]   Ring data

#include "driver.h"
#include "shared_mem.h"

#define SHM_FILE_PATH  L"\\??\\C:\\ProgramData\\PhoneMike\\ring.dat"
#define SHM_TOTAL_SIZE 65536
#define SHM_HEADER     28
#define SHM_RING_CAP   (SHM_TOTAL_SIZE - SHM_HEADER)
#define SHM_MAGIC      0x434D4850  // 'PHMC' LE

// Offsets
#define OFF_MAGIC       0
#define OFF_WRITE_INDEX 16
#define OFF_READ_INDEX  20
#define OFF_RUNNING     24
#define OFF_RING_DATA   28

// MmMapViewInSystemSpace / MmUnmapViewInSystemSpace â€” maps into kernel VA
// accessible from any process context (unlike ZwMapViewOfSection which maps
// into current process user-mode VA).
extern "C" {
    NTSTATUS MmMapViewInSystemSpace(PVOID SectionObject, PVOID* MappedBase, PSIZE_T ViewSize);
    NTSTATUS MmUnmapViewInSystemSpace(PVOID MappedBase);
}

static HANDLE  g_ShmFileHandle    = nullptr;
static HANDLE  g_ShmSectionHandle = nullptr;
static PVOID   g_ShmSectionObject = nullptr;
static PVOID   g_ShmBaseAddress   = nullptr;
static PBYTE   g_ShmView          = nullptr;

BOOLEAN OpenSharedMemory()
{
    if (g_ShmView) return TRUE;  // already open

    UNICODE_STRING filePath;
    RtlInitUnicodeString(&filePath, SHM_FILE_PATH);

    OBJECT_ATTRIBUTES oa;
    InitializeObjectAttributes(&oa, &filePath, OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE, nullptr, nullptr);

    IO_STATUS_BLOCK iosb;
    NTSTATUS status = ZwOpenFile(
        &g_ShmFileHandle,
        GENERIC_READ | GENERIC_WRITE,
        &oa,
        &iosb,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        FILE_SYNCHRONOUS_IO_NONALERT);

    if (!NT_SUCCESS(status)) {
        DbgPrint("PhoneMike: ZwOpenFile failed: 0x%08X\n", status);
        return FALSE;
    }

    // Create section (file mapping)
    LARGE_INTEGER maxSize;
    maxSize.QuadPart = SHM_TOTAL_SIZE;

    status = ZwCreateSection(
        &g_ShmSectionHandle,
        SECTION_MAP_READ | SECTION_MAP_WRITE,
        nullptr,
        &maxSize,
        PAGE_READWRITE,
        SEC_COMMIT,
        g_ShmFileHandle);

    if (!NT_SUCCESS(status)) {
        DbgPrint("PhoneMike: ZwCreateSection failed: 0x%08X\n", status);
        ZwClose(g_ShmFileHandle);
        g_ShmFileHandle = nullptr;
        return FALSE;
    }

    // Get section object pointer for kernel-space mapping
    PVOID sectionObject = nullptr;
    status = ObReferenceObjectByHandle(
        g_ShmSectionHandle,
        SECTION_MAP_READ | SECTION_MAP_WRITE,
        nullptr,
        KernelMode,
        &sectionObject,
        nullptr);

    if (!NT_SUCCESS(status)) {
        DbgPrint("PhoneMike: ObReferenceObjectByHandle failed: 0x%08X\n", status);
        ZwClose(g_ShmSectionHandle);
        ZwClose(g_ShmFileHandle);
        g_ShmSectionHandle = nullptr;
        g_ShmFileHandle = nullptr;
        return FALSE;
    }

    // Map into KERNEL VA (system space) â€” accessible from any process context.
    // Unlike ZwMapViewOfSection(ZwCurrentProcess()) which maps into user-mode VA
    // of the calling process and BSODs when accessed from a different process.
    SIZE_T viewSize = SHM_TOTAL_SIZE;
    g_ShmBaseAddress = nullptr;

    status = MmMapViewInSystemSpace(sectionObject, &g_ShmBaseAddress, &viewSize);

    if (!NT_SUCCESS(status)) {
        DbgPrint("PhoneMike: MmMapViewInSystemSpace failed: 0x%08X\n", status);
        ObDereferenceObject(sectionObject);
        ZwClose(g_ShmSectionHandle);
        ZwClose(g_ShmFileHandle);
        g_ShmSectionHandle = nullptr;
        g_ShmFileHandle = nullptr;
        return FALSE;
    }

    g_ShmSectionObject = sectionObject;  // keep referenced while mapped
    g_ShmView = (PBYTE)g_ShmBaseAddress;

    // Verify magic
    ULONG magic = *(volatile ULONG*)(g_ShmView + OFF_MAGIC);
    if (magic != SHM_MAGIC) {
        DbgPrint("PhoneMike: Bad shared mem magic: 0x%08X (expected 0x%08X)\n", magic, SHM_MAGIC);
        CloseSharedMemory();
        return FALSE;
    }

    DbgPrint("PhoneMike: Shared memory opened OK, ring capacity = %u\n", SHM_RING_CAP);
    return TRUE;
}

void CloseSharedMemory()
{
    if (g_ShmBaseAddress) {
        MmUnmapViewInSystemSpace(g_ShmBaseAddress);
        g_ShmBaseAddress = nullptr;
        g_ShmView = nullptr;
    }
    if (g_ShmSectionObject) {
        ObDereferenceObject(g_ShmSectionObject);
        g_ShmSectionObject = nullptr;
    }
    if (g_ShmSectionHandle) {
        ZwClose(g_ShmSectionHandle);
        g_ShmSectionHandle = nullptr;
    }
    if (g_ShmFileHandle) {
        ZwClose(g_ShmFileHandle);
        g_ShmFileHandle = nullptr;
    }
}

ULONG ReadSharedMemBytes(PBYTE dest, ULONG count)
{
    if (!g_ShmView || !dest || count == 0) {
        if (dest && count) RtlZeroMemory(dest, count);
        return 0;
    }

    // Atomically read write index (set by pc-client)
    volatile LONG* pWriteIdx = (volatile LONG*)(g_ShmView + OFF_WRITE_INDEX);
    volatile LONG* pReadIdx  = (volatile LONG*)(g_ShmView + OFF_READ_INDEX);

    LONG wi = InterlockedCompareExchange(pWriteIdx, 0, 0);
    LONG ri = InterlockedCompareExchange(pReadIdx, 0, 0);
    LONG available = wi - ri;

    if (available <= 0) {
        RtlZeroMemory(dest, count);
        return 0;
    }

    ULONG toRead = min(count, (ULONG)available);
    PBYTE ringData = g_ShmView + OFF_RING_DATA;
    ULONG remaining = toRead;
    ULONG offset = 0;

    while (remaining > 0) {
        ULONG pos = ((ULONG)ri + offset) % SHM_RING_CAP;
        ULONG toEnd = SHM_RING_CAP - pos;
        ULONG chunk = min(remaining, toEnd);
        RtlCopyMemory(dest + offset, ringData + pos, chunk);
        offset += chunk;
        remaining -= chunk;
    }

    // Silence-fill remainder
    if (toRead < count) {
        RtlZeroMemory(dest + toRead, count - toRead);
    }

    // Advance read index
    InterlockedExchangeAdd(pReadIdx, (LONG)toRead);
    return toRead;
}

ULONG SharedMemAvailable()
{
    if (!g_ShmView) return 0;
    volatile LONG* pWriteIdx = (volatile LONG*)(g_ShmView + OFF_WRITE_INDEX);
    volatile LONG* pReadIdx  = (volatile LONG*)(g_ShmView + OFF_READ_INDEX);
    LONG wi = InterlockedCompareExchange(pWriteIdx, 0, 0);
    LONG ri = InterlockedCompareExchange(pReadIdx, 0, 0);
    LONG avail = wi - ri;
    return (avail > 0) ? (ULONG)avail : 0;
}

void SkipSharedMemBytes(ULONG count)
{
    if (!g_ShmView || count == 0) return;
    volatile LONG* pReadIdx = (volatile LONG*)(g_ShmView + OFF_READ_INDEX);
    InterlockedExchangeAdd(pReadIdx, (LONG)count);
}

BOOLEAN IsSharedMemOpen()
{
    return g_ShmView != nullptr;
}