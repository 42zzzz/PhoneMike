#include "driver.h"
#include "shared_mem.h"

extern void WriteDriverLog(const char* msg);

//
// CMiniportStream â€” PortCls stream for the PhoneMike capture pin.
//
// Reads audio from the shared memory ring buffer (written by pc-client)
// and copies it into the DMA cyclic buffer for PortCls.
//
class CMiniportStream : public IMiniportWaveCyclicStream, public CUnknown
{
public:
    DECLARE_STD_UNKNOWN()
    DEFINE_STD_CONSTRUCTOR(CMiniportStream)
    ~CMiniportStream();

    // IMiniportWaveCyclicStream
    STDMETHODIMP_(NTSTATUS) SetFormat(PKSDATAFORMAT DataFormat) override;
    STDMETHODIMP_(ULONG)    SetNotificationFreq(ULONG Interval, PULONG FrameSize) override;
    STDMETHODIMP_(NTSTATUS) SetState(KSSTATE State) override;
    STDMETHODIMP_(NTSTATUS) GetPosition(PULONG Position) override;
    STDMETHODIMP_(NTSTATUS) NormalizePhysicalPosition(PLONGLONG PhysicalPosition) override;
    STDMETHODIMP_(void)     Silence(PVOID Buffer, ULONG ByteCount) override;

    // Init â€” called by factory
    NTSTATUS Init(PPORTWAVECYCLIC Port, PSERVICEGROUP ServiceGroup,
                  PKSDATAFORMAT DataFormat, PWAVEFORMATEX WaveFormat);

    // Exposed to factory for DmaChannel hand-off to PortCls
    PDMACHANNEL m_DmaChannel = nullptr;

private:
    static void NTAPI WorkerThread(PVOID Context);
    void ServiceBuffer(ULONG bytesToRead);

    PPORTWAVECYCLIC m_Port          = nullptr;
    PSERVICEGROUP   m_ServiceGroup  = nullptr;
    PBYTE           m_DmaBuffer     = nullptr;
    ULONG           m_DmaBufferSize = 0;
    ULONG           m_WritePos      = 0;
    ULONG           m_NotifyInterval= 0;
    KSSTATE         m_State         = KSSTATE_STOP;

    // Worker thread
    HANDLE          m_ThreadHandle  = nullptr;
    PVOID           m_ThreadObject  = nullptr;
    KEVENT          m_StopEvent     = {};
    volatile LONG   m_Running       = 0;
};

STDMETHODIMP_(NTSTATUS) CMiniportStream::NonDelegatingQueryInterface(REFIID riid, PVOID* ppVoid)
{
    if (IsEqualGUIDAligned(riid, IID_IUnknown)) {
        *ppVoid = static_cast<PUNKNOWN>(static_cast<IMiniportWaveCyclicStream*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IMiniportWaveCyclicStream)) {
        *ppVoid = static_cast<IMiniportWaveCyclicStream*>(this);
    } else {
        *ppVoid = nullptr;
        return STATUS_INVALID_PARAMETER;
    }
    reinterpret_cast<PUNKNOWN>(*ppVoid)->AddRef();
    return STATUS_SUCCESS;
}

CMiniportStream::~CMiniportStream()
{
    // Stop worker thread
    if (m_ThreadObject) {
        InterlockedExchange(&m_Running, 0);
        KeSetEvent(&m_StopEvent, 0, FALSE);
        KeWaitForSingleObject(m_ThreadObject, Executive, KernelMode, FALSE, nullptr);
        ObDereferenceObject(m_ThreadObject);
        m_ThreadObject = nullptr;
    }
    if (m_ThreadHandle) {
        ZwClose(m_ThreadHandle);
        m_ThreadHandle = nullptr;
    }

    CloseSharedMemory();

    if (m_ServiceGroup) {
        m_ServiceGroup->Release();
        m_ServiceGroup = nullptr;
    }
    if (m_DmaChannel) {
        m_DmaChannel->Release();
        m_DmaChannel = nullptr;
    }
    if (m_Port) {
        m_Port->Release();
        m_Port = nullptr;
    }
}

NTSTATUS CMiniportStream::Init(
    PPORTWAVECYCLIC Port, PSERVICEGROUP ServiceGroup,
    PKSDATAFORMAT DataFormat, PWAVEFORMATEX WaveFormat)
{
    UNREFERENCED_PARAMETER(DataFormat);
    UNREFERENCED_PARAMETER(WaveFormat);

    m_Port = Port;
    Port->AddRef();

    m_ServiceGroup = ServiceGroup;
    ServiceGroup->AddRef();

    // Allocate virtual DMA channel (system memory, no real DMA hardware)
    NTSTATUS status = CreateVirtualDmaChannel(PhoneMike_BUFFER_BYTES, &m_DmaChannel);
    if (!NT_SUCCESS(status)) return status;

    m_DmaBuffer     = (PBYTE)m_DmaChannel->SystemAddress();
    m_DmaBufferSize = m_DmaChannel->AllocatedBufferSize();
    RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);

    KeInitializeEvent(&m_StopEvent, NotificationEvent, FALSE);

    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportStream::SetFormat(PKSDATAFORMAT)
{
    return STATUS_SUCCESS;
}

STDMETHODIMP_(ULONG) CMiniportStream::SetNotificationFreq(ULONG Interval, PULONG FrameSize)
{
    m_NotifyInterval = Interval;
    *FrameSize       = PhoneMike_BUFFER_BYTES;
    return Interval;
}

STDMETHODIMP_(NTSTATUS) CMiniportStream::SetState(KSSTATE State)
{
    if (State == m_State) return STATUS_SUCCESS;

    DbgPrint("PhoneMike: CaptureStream SetState %d -> %d\n", m_State, State);
    WriteDriverLog("Capture SetState\n");

    switch (State) {
    case KSSTATE_RUN: {
        // Shared memory open is handled by WorkerThread only â€”
        // SetState runs in the calling app's process context, but the
        // kernel VA mapping must be done from a consistent context.

        // Start worker thread
        if (!m_ThreadHandle) {
            KeClearEvent(&m_StopEvent);
            InterlockedExchange(&m_Running, 1);

            OBJECT_ATTRIBUTES oa;
            InitializeObjectAttributes(&oa, nullptr, OBJ_KERNEL_HANDLE, nullptr, nullptr);
            HANDLE hThread = nullptr;
            NTSTATUS status = PsCreateSystemThread(
                &hThread, THREAD_ALL_ACCESS, &oa,
                nullptr, nullptr, WorkerThread, this);
            if (NT_SUCCESS(status)) {
                m_ThreadHandle = hThread;
                ObReferenceObjectByHandle(hThread, SYNCHRONIZE, *PsThreadType,
                                          KernelMode, &m_ThreadObject, nullptr);
            } else {
                DbgPrint("PhoneMike: PsCreateSystemThread failed: 0x%08X\n", status);
            }
        }
        break;
    }
    case KSSTATE_PAUSE:
    case KSSTATE_STOP:
    case KSSTATE_ACQUIRE:
        // Stop worker thread
        if (m_ThreadObject) {
            InterlockedExchange(&m_Running, 0);
            KeSetEvent(&m_StopEvent, 0, FALSE);
            KeWaitForSingleObject(m_ThreadObject, Executive, KernelMode, FALSE, nullptr);
            ObDereferenceObject(m_ThreadObject);
            m_ThreadObject = nullptr;
        }
        if (m_ThreadHandle) {
            ZwClose(m_ThreadHandle);
            m_ThreadHandle = nullptr;
        }
        m_WritePos = 0;
        if (State == KSSTATE_STOP) {
            RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
        }
        break;
    }

    m_State = State;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportStream::GetPosition(PULONG Position)
{
    *Position = m_WritePos;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportStream::NormalizePhysicalPosition(PLONGLONG PhysicalPosition)
{
    *PhysicalPosition = (*PhysicalPosition * PhoneMike_SAMPLE_RATE * PhoneMike_BLOCK_ALIGN)
                        / PhoneMike_BYTE_RATE;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(void) CMiniportStream::Silence(PVOID Buffer, ULONG ByteCount)
{
    RtlZeroMemory(Buffer, ByteCount);
}

void NTAPI CMiniportStream::WorkerThread(PVOID Context)
{
    CMiniportStream* self = static_cast<CMiniportStream*>(Context);

    // Boost thread priority above normal-class threads for glitch-free audio
    KeSetPriorityThread(KeGetCurrentThread(), 25);  // HIGH_PRIORITY

    LARGE_INTEGER interval;
    interval.QuadPart = -(LONGLONG)PhoneMike_NOTIFICATION_INTERVAL_100NS;

    LARGE_INTEGER perfFreq;
    LARGE_INTEGER lastTick = KeQueryPerformanceCounter(&perfFreq);
    ULONG retryCounter = 0;

    while (InterlockedCompareExchange(&self->m_Running, 1, 1) == 1) {
        // Open shared memory â€” try immediately, then every ~1s
        if (!IsSharedMemOpen()) {
            if (retryCounter == 0 || retryCounter % 100 == 0) {
                OpenSharedMemory();
            }
            retryCounter++;
        }

        // Measure actual elapsed time since last tick
        LARGE_INTEGER now = KeQueryPerformanceCounter(nullptr);
        LONGLONG elapsed = now.QuadPart - lastTick.QuadPart;
        lastTick = now;

        // Convert to bytes: elapsed / freq * BYTE_RATE
        // Use 64-bit math to avoid overflow
        ULONG bytesForElapsed = (ULONG)((elapsed * PhoneMike_BYTE_RATE) / perfFreq.QuadPart);
        bytesForElapsed &= ~1u;  // align to 16-bit sample boundary

        // Clamp: min 480 bytes (5ms), max 4800 bytes (50ms)
        if (bytesForElapsed < 480)  bytesForElapsed = 480;
        if (bytesForElapsed > 4800) bytesForElapsed = 4800;

        self->ServiceBuffer(bytesForElapsed);

        KeWaitForSingleObject(&self->m_StopEvent, Executive, KernelMode, FALSE, &interval);
    }

    PsTerminateSystemThread(STATUS_SUCCESS);
}

void CMiniportStream::ServiceBuffer(ULONG bytesToRead)
{
    if (m_State != KSSTATE_RUN || !m_DmaBuffer) return;
    if (!IsSharedMemOpen()) return;

    ULONG available = SharedMemAvailable();

    // Lag compensation: if >50ms buffered, skip to keep only ~10ms
    const ULONG LAG_THRESHOLD = PhoneMike_BYTE_RATE / 20;  // 50ms
    if (available > LAG_THRESHOLD) {
        ULONG skip = available - PhoneMike_NOTIFICATION_BYTES;
        SkipSharedMemBytes(skip);
        available = PhoneMike_NOTIFICATION_BYTES;
    }

    // Underrun: no data available â€” don't fill silence, don't advance
    if (available == 0) return;

    // Read min(requested, available), cap to DMA buffer size
    ULONG toRead = min(bytesToRead, available);
    toRead = min(toRead, m_DmaBufferSize);
    toRead &= ~1u;  // sample-align

    if (toRead == 0) return;

    ULONG remaining = toRead;
    while (remaining > 0) {
        ULONG toEnd = m_DmaBufferSize - m_WritePos;
        ULONG chunk = min(remaining, toEnd);
        ReadSharedMemBytes(m_DmaBuffer + m_WritePos, chunk);
        m_WritePos = (m_WritePos + chunk) % m_DmaBufferSize;
        remaining -= chunk;
    }

    if (m_ServiceGroup) {
        m_ServiceGroup->RequestService();
    }
}

// Factory â€” called by miniport NewStream
NTSTATUS CreateMiniportStream(
    PPORTWAVECYCLIC Port,
    PSERVICEGROUP   ServiceGroup,
    PKSDATAFORMAT   DataFormat,
    PWAVEFORMATEX   WaveFormat,
    IMiniportWaveCyclicStream** OutStream,
    PDMACHANNEL*    OutDmaChannel)
{
    CMiniportStream* stream = new(NonPagedPool, 'pmCS') CMiniportStream(nullptr);
    if (!stream) return STATUS_INSUFFICIENT_RESOURCES;

    stream->AddRef();
    NTSTATUS status = stream->Init(Port, ServiceGroup, DataFormat, WaveFormat);
    if (!NT_SUCCESS(status)) {
        stream->Release();
        return status;
    }

    *OutStream     = static_cast<IMiniportWaveCyclicStream*>(stream);
    *OutDmaChannel = stream->m_DmaChannel;
    if (*OutDmaChannel) (*OutDmaChannel)->AddRef();
    return STATUS_SUCCESS;
}