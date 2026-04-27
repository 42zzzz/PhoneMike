#include "driver.h"

// Forward declare capture stream factory
NTSTATUS CreateMiniportStream(
    PPORTWAVECYCLIC Port,
    PSERVICEGROUP   ServiceGroup,
    PKSDATAFORMAT   DataFormat,
    PWAVEFORMATEX   WaveFormat,
    IMiniportWaveCyclicStream** OutStream,
    PDMACHANNEL*    OutDmaChannel);

//
// KS data format â€” 48kHz, mono, 16-bit PCM
//
static KSDATAFORMAT_WAVEFORMATEX g_WaveFormat = {
    {
        sizeof(KSDATAFORMAT_WAVEFORMATEX),
        0, 0, 0,
        STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
        STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM),
        STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
    },
    {
        WAVE_FORMAT_PCM,
        PhoneMike_CHANNELS,
        PhoneMike_SAMPLE_RATE,
        PhoneMike_BYTE_RATE,
        PhoneMike_BLOCK_ALIGN,
        PhoneMike_BITS_PER_SAMPLE,
        0
    }
};

// Bridge pin data range â€” analog (for bridge pins)
static KSDATARANGE g_BridgeDataRange = {
    sizeof(KSDATARANGE),
    0, 0, 0,
    STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
    STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
    STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
};
static PKSDATARANGE g_BridgeDataRanges[] = { &g_BridgeDataRange };

// Capture pin data range
static KSDATARANGE_AUDIO g_CaptureDataRange;
static PKSDATARANGE      g_CaptureDataRanges[1];

static void InitDataRange()
{
    RtlZeroMemory(&g_CaptureDataRange, sizeof(g_CaptureDataRange));
    g_CaptureDataRange.DataRange.FormatSize  = sizeof(KSDATARANGE_AUDIO);
    g_CaptureDataRange.DataRange.MajorFormat = KSDATAFORMAT_TYPE_AUDIO;
    g_CaptureDataRange.DataRange.SubFormat   = KSDATAFORMAT_SUBTYPE_PCM;
    g_CaptureDataRange.DataRange.Specifier   = KSDATAFORMAT_SPECIFIER_WAVEFORMATEX;
    g_CaptureDataRange.MaximumChannels        = PhoneMike_CHANNELS;
    g_CaptureDataRange.MinimumBitsPerSample   = PhoneMike_BITS_PER_SAMPLE;
    g_CaptureDataRange.MaximumBitsPerSample   = PhoneMike_BITS_PER_SAMPLE;
    g_CaptureDataRange.MinimumSampleFrequency = PhoneMike_SAMPLE_RATE;
    g_CaptureDataRange.MaximumSampleFrequency = PhoneMike_SAMPLE_RATE;
    g_CaptureDataRanges[0] = &g_CaptureDataRange.DataRange;
}

//
// CMiniportPhoneMike â€” PortCls miniport for PhoneMike
//
class CMiniportPhoneMike : public IMiniportWaveCyclic, public CUnknown
{
public:
    DECLARE_STD_UNKNOWN()
    DEFINE_STD_CONSTRUCTOR(CMiniportPhoneMike)
    ~CMiniportPhoneMike();

    // IMiniport
    STDMETHODIMP_(NTSTATUS) GetDescription(PPCFILTER_DESCRIPTOR* Description) override;
    STDMETHODIMP_(NTSTATUS) DataRangeIntersection(
        ULONG PinId, PKSDATARANGE DataRange, PKSDATARANGE MatchingDataRange,
        ULONG OutputBufferLength, PVOID ResultantFormat,
        PULONG ResultantFormatLength) override;

    // IMiniportWaveCyclic
    STDMETHODIMP_(NTSTATUS) Init(PUNKNOWN UnknownAdapter, PRESOURCELIST ResourceList,
                                  PPORTWAVECYCLIC Port) override;
    STDMETHODIMP_(NTSTATUS) NewStream(PMINIPORTWAVECYCLICSTREAM* Stream,
                                       PUNKNOWN OuterUnknown, POOL_TYPE PoolType,
                                       ULONG Pin, BOOLEAN Capture,
                                       PKSDATAFORMAT DataFormat,
                                       PDMACHANNEL* DmaChannel,
                                       PSERVICEGROUP* ServiceGroup) override;

private:
    PPORTWAVECYCLIC m_Port = nullptr;
};

STDMETHODIMP_(NTSTATUS) CMiniportPhoneMike::NonDelegatingQueryInterface(REFIID riid, PVOID* ppVoid)
{
    if (IsEqualGUIDAligned(riid, IID_IUnknown)) {
        *ppVoid = static_cast<PUNKNOWN>(static_cast<IMiniportWaveCyclic*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IMiniport)) {
        *ppVoid = static_cast<PMINIPORT>(static_cast<IMiniportWaveCyclic*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IMiniportWaveCyclic)) {
        *ppVoid = static_cast<IMiniportWaveCyclic*>(this);
    } else {
        *ppVoid = nullptr;
        return STATUS_INVALID_PARAMETER;
    }
    reinterpret_cast<PUNKNOWN>(*ppVoid)->AddRef();
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportPhoneMike::GetDescription(PPCFILTER_DESCRIPTOR* Desc)
{
    // Capture-only wave filter: 2 pins
    // Pin 0: bridge capture IN (from topology)
    // Pin 1: capture stream OUT (apps read)
    static PCPIN_DESCRIPTOR PinDescriptors[2];
    static PCCONNECTION_DESCRIPTOR Connections[1];
    static PCFILTER_DESCRIPTOR FilterDesc;
    static bool s_Initialized = false;

    if (!s_Initialized) {
        s_Initialized = true;
        InitDataRange();

        RtlZeroMemory(PinDescriptors, sizeof(PinDescriptors));

        // Pin 0: bridge capture (mic source â€” data enters filter)
        PinDescriptors[0].KsPinDescriptor.DataRangesCount = SIZEOF_ARRAY(g_BridgeDataRanges);
        PinDescriptors[0].KsPinDescriptor.DataRanges      = g_BridgeDataRanges;
        PinDescriptors[0].KsPinDescriptor.DataFlow        = KSPIN_DATAFLOW_IN;
        PinDescriptors[0].KsPinDescriptor.Communication   = KSPIN_COMMUNICATION_NONE;
        PinDescriptors[0].KsPinDescriptor.Category        = &KSCATEGORY_AUDIO;

        // Pin 1: capture stream (apps read from here)
        PinDescriptors[1].MaxGlobalInstanceCount          = 1;
        PinDescriptors[1].MaxFilterInstanceCount          = 1;
        PinDescriptors[1].KsPinDescriptor.DataRangesCount = SIZEOF_ARRAY(g_CaptureDataRanges);
        PinDescriptors[1].KsPinDescriptor.DataRanges      = g_CaptureDataRanges;
        PinDescriptors[1].KsPinDescriptor.DataFlow        = KSPIN_DATAFLOW_OUT;
        PinDescriptors[1].KsPinDescriptor.Communication   = KSPIN_COMMUNICATION_SINK;
        PinDescriptors[1].KsPinDescriptor.Category        = &KSCATEGORY_CAPTURE;

        // Connection: bridge pin 0 â†’ capture stream pin 1
        Connections[0].FromNode    = PCFILTER_NODE;
        Connections[0].FromNodePin = 0;
        Connections[0].ToNode      = PCFILTER_NODE;
        Connections[0].ToNodePin   = 1;

        RtlZeroMemory(&FilterDesc, sizeof(FilterDesc));
        FilterDesc.Version         = 0;
        FilterDesc.PinSize         = sizeof(PCPIN_DESCRIPTOR);
        FilterDesc.PinCount        = SIZEOF_ARRAY(PinDescriptors);
        FilterDesc.Pins            = PinDescriptors;
        FilterDesc.ConnectionCount = SIZEOF_ARRAY(Connections);
        FilterDesc.Connections     = Connections;
    }

    *Desc = &FilterDesc;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportPhoneMike::DataRangeIntersection(
    ULONG, PKSDATARANGE, PKSDATARANGE,
    ULONG OutputBufferLength, PVOID ResultantFormat, PULONG ResultantFormatLength)
{
    *ResultantFormatLength = sizeof(KSDATAFORMAT_WAVEFORMATEX);
    if (OutputBufferLength == 0) return STATUS_BUFFER_OVERFLOW;
    if (OutputBufferLength < sizeof(KSDATAFORMAT_WAVEFORMATEX)) return STATUS_BUFFER_TOO_SMALL;
    RtlCopyMemory(ResultantFormat, &g_WaveFormat, sizeof(KSDATAFORMAT_WAVEFORMATEX));
    return STATUS_SUCCESS;
}

CMiniportPhoneMike::~CMiniportPhoneMike()
{
    if (m_Port) {
        m_Port->Release();
        m_Port = nullptr;
    }
}

// Write diagnostic string to C:\ProgramData\PhoneMike\driver_log.txt
void WriteDriverLog(const char* msg)
{
    UNICODE_STRING path;
    RtlInitUnicodeString(&path, L"\\??\\C:\\ProgramData\\PhoneMike\\driver_log.txt");
    OBJECT_ATTRIBUTES oa;
    InitializeObjectAttributes(&oa, &path, OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE, nullptr, nullptr);
    IO_STATUS_BLOCK iosb;
    HANDLE hFile = nullptr;
    NTSTATUS st = ZwCreateFile(&hFile, FILE_APPEND_DATA | SYNCHRONIZE, &oa, &iosb,
        nullptr, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
        FILE_OPEN_IF, FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE, nullptr, 0);
    if (NT_SUCCESS(st)) {
        ULONG len = 0;
        while (msg[len]) len++;
        ZwWriteFile(hFile, nullptr, nullptr, nullptr, &iosb, (PVOID)msg, len, nullptr, nullptr);
        ZwClose(hFile);
    }
}

STDMETHODIMP_(NTSTATUS) CMiniportPhoneMike::Init(
    PUNKNOWN, PRESOURCELIST, PPORTWAVECYCLIC Port)
{
    m_Port = Port;
    Port->AddRef();
    InitDataRange();

    WriteDriverLog("Init() called\n");

    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportPhoneMike::NewStream(
    PMINIPORTWAVECYCLICSTREAM* Stream, PUNKNOWN OuterUnknown,
    POOL_TYPE, ULONG Pin, BOOLEAN Capture,
    PKSDATAFORMAT DataFormat, PDMACHANNEL* DmaChannel, PSERVICEGROUP* ServiceGroup)
{
    UNREFERENCED_PARAMETER(OuterUnknown);

    if (Pin != 1 || !Capture) return STATUS_INVALID_DEVICE_REQUEST;

    PSERVICEGROUP svcGroup = nullptr;
    NTSTATUS status = PcNewServiceGroup(&svcGroup, nullptr);
    if (!NT_SUCCESS(status)) return status;

    IMiniportWaveCyclicStream* stream = nullptr;
    PDMACHANNEL dmaChannel = nullptr;

    WriteDriverLog("NewStream: capture\n");
    status = CreateMiniportStream(
        m_Port, svcGroup,
        DataFormat, &g_WaveFormat.WaveFormatEx,
        &stream, &dmaChannel);

    if (!NT_SUCCESS(status)) {
        static const char hex[] = "0123456789ABCDEF";
        char buf[40] = "Capture FAIL 0x________\n";
        ULONG v = (ULONG)status;
        for (int i = 7; i >= 0; i--)
            buf[15 + (7 - i)] = hex[(v >> (i * 4)) & 0xF];
        WriteDriverLog(buf);
        svcGroup->Release();
        return status;
    }
    WriteDriverLog("Capture OK\n");

    *DmaChannel   = dmaChannel;
    *ServiceGroup = svcGroup;
    *Stream       = static_cast<PMINIPORTWAVECYCLICSTREAM>(stream);
    return STATUS_SUCCESS;
}

// Factory â€” called from driver_entry.cpp
NTSTATUS CreateMiniportPhoneMike(
    PUNKNOWN* Unknown, REFCLSID, PUNKNOWN UnknownOuter, POOL_TYPE PoolType)
{
    UNREFERENCED_PARAMETER(UnknownOuter);
    CMiniportPhoneMike* miniport = new(PoolType, 'pmMp') CMiniportPhoneMike(nullptr);
    if (!miniport) return STATUS_INSUFFICIENT_RESOURCES;
    miniport->AddRef();
    *Unknown = static_cast<PUNKNOWN>(static_cast<IMiniportWaveCyclic*>(miniport));
    return STATUS_SUCCESS;
}
