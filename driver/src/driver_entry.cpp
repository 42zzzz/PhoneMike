#include "driver.h"

// Forward
NTSTATUS CreateMiniportPhoneMike(
    PUNKNOWN* Unknown, REFCLSID, PUNKNOWN UnknownOuter, POOL_TYPE PoolType);
NTSTATUS CreateMiniportTopology(
    PUNKNOWN* Unknown, REFCLSID, PUNKNOWN UnknownOuter, POOL_TYPE PoolType);

//
// StartDevice â€” creates topology + WaveCyclic capture port/miniport pairs.
//
NTSTATUS StartDevice(
    PDEVICE_OBJECT DeviceObject,
    PIRP           Irp,
    PRESOURCELIST  ResourceList)
{
    UNREFERENCED_PARAMETER(Irp);

    PPORT       topoPort        = nullptr;
    PUNKNOWN    topoMiniport    = nullptr;
    PPORT       waveCapturePort = nullptr;
    PUNKNOWN    waveCaptureMini = nullptr;
    NTSTATUS    status;

    // --- Topology subdevice ---
    status = PcNewPort(&topoPort, CLSID_PortTopology);
    if (!NT_SUCCESS(status)) goto done;

    status = CreateMiniportTopology(&topoMiniport, CLSID_NULL, nullptr, NonPagedPool);
    if (!NT_SUCCESS(status)) goto done;

    status = topoPort->Init(DeviceObject, Irp, topoMiniport, nullptr, ResourceList);
    if (!NT_SUCCESS(status)) goto done;

    status = PcRegisterSubdevice(DeviceObject, L"PhoneMikeTopo", topoPort);
    if (!NT_SUCCESS(status)) goto done;

    // --- WaveCyclic capture subdevice ---
    status = PcNewPort(&waveCapturePort, CLSID_PortWaveCyclic);
    if (!NT_SUCCESS(status)) goto done;

    status = CreateMiniportPhoneMike(&waveCaptureMini, CLSID_NULL, nullptr, NonPagedPool);
    if (!NT_SUCCESS(status)) goto done;

    status = waveCapturePort->Init(DeviceObject, Irp, waveCaptureMini, nullptr, ResourceList);
    if (!NT_SUCCESS(status)) goto done;

    status = PcRegisterSubdevice(DeviceObject, L"PhoneMikeWaveCapture", waveCapturePort);
    if (!NT_SUCCESS(status)) goto done;

    // --- Physical connection: topo pin 1 (OUT) â†’ wave-capture pin 0 (IN) ---
    PcRegisterPhysicalConnection(DeviceObject,
        topoPort, 1,
        waveCapturePort, 0);

done:
    if (topoPort)        topoPort->Release();
    if (topoMiniport)    topoMiniport->Release();
    if (waveCapturePort) waveCapturePort->Release();
    if (waveCaptureMini) waveCaptureMini->Release();

    return status;
}

//
// AddDevice â€” maxObjects = 2 (topology + wave-capture)
//
NTSTATUS AddDevice(PDRIVER_OBJECT DriverObject, PDEVICE_OBJECT PhysicalDeviceObject)
{
    return PcAddAdapterDevice(DriverObject, PhysicalDeviceObject, StartDevice, 2, 0);
}

//
// DriverEntry
//
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT DriverObject, PUNICODE_STRING RegistryPath)
{
    return PcInitializeAdapterDriver(DriverObject, RegistryPath, AddDevice);
}