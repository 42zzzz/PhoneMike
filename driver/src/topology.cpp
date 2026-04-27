#include "driver.h"

//
// Topology miniport for PhoneMike.
// Capture only: bridge mic (pin 0) → mic node → stream out (pin 1)
//

static PCCONNECTION_DESCRIPTOR g_TopoConnections[] = {
    // bridge pin 0 → mic node (node 0) input pin 1
    { PCFILTER_NODE, 0,    0,             1 },
    // mic node (node 0) output pin 0 → capture pin 1
    { 0,             0,    PCFILTER_NODE, 1 },
};

static GUID g_MicNodeType = KSNODETYPE_MICROPHONE;

static PCNODE_DESCRIPTOR g_TopoNodes[] = {
    { 0, nullptr, &g_MicNodeType, nullptr },   // node 0: microphone
};

// Bridge pin data ranges (analog)
static KSDATARANGE g_TopoBridgeRange = {
    sizeof(KSDATARANGE), 0, 0, 0,
    STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
    STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
    STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
};
static PKSDATARANGE g_TopoBridgeRanges[] = { &g_TopoBridgeRange };

// Stream pin data ranges (analog)
static KSDATARANGE g_TopoStreamRange = {
    sizeof(KSDATARANGE), 0, 0, 0,
    STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
    STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
    STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
};
static PKSDATARANGE g_TopoStreamRanges[] = { &g_TopoStreamRange };

static PCPIN_DESCRIPTOR g_TopoPins[2];
static PCFILTER_DESCRIPTOR g_TopoFilterDesc;
static bool g_TopoInitialized = false;

static void InitTopology()
{
    if (g_TopoInitialized) return;
    g_TopoInitialized = true;

    RtlZeroMemory(g_TopoPins, sizeof(g_TopoPins));

    // Pin 0: bridge mic (data flows IN from microphone source)
    g_TopoPins[0].KsPinDescriptor.DataRangesCount = SIZEOF_ARRAY(g_TopoBridgeRanges);
    g_TopoPins[0].KsPinDescriptor.DataRanges      = g_TopoBridgeRanges;
    g_TopoPins[0].KsPinDescriptor.DataFlow        = KSPIN_DATAFLOW_IN;
    g_TopoPins[0].KsPinDescriptor.Communication   = KSPIN_COMMUNICATION_NONE;
    g_TopoPins[0].KsPinDescriptor.Category        = &KSNODETYPE_MICROPHONE;

    // Pin 1: capture stream (data flows OUT to wave filter)
    g_TopoPins[1].KsPinDescriptor.DataRangesCount = SIZEOF_ARRAY(g_TopoStreamRanges);
    g_TopoPins[1].KsPinDescriptor.DataRanges      = g_TopoStreamRanges;
    g_TopoPins[1].KsPinDescriptor.DataFlow        = KSPIN_DATAFLOW_OUT;
    g_TopoPins[1].KsPinDescriptor.Communication   = KSPIN_COMMUNICATION_NONE;
    g_TopoPins[1].KsPinDescriptor.Category        = &KSCATEGORY_AUDIO;

    RtlZeroMemory(&g_TopoFilterDesc, sizeof(g_TopoFilterDesc));
    g_TopoFilterDesc.Version          = 0;
    g_TopoFilterDesc.PinSize          = sizeof(PCPIN_DESCRIPTOR);
    g_TopoFilterDesc.PinCount         = SIZEOF_ARRAY(g_TopoPins);
    g_TopoFilterDesc.Pins             = g_TopoPins;
    g_TopoFilterDesc.NodeSize         = sizeof(PCNODE_DESCRIPTOR);
    g_TopoFilterDesc.NodeCount        = SIZEOF_ARRAY(g_TopoNodes);
    g_TopoFilterDesc.Nodes            = g_TopoNodes;
    g_TopoFilterDesc.ConnectionCount  = SIZEOF_ARRAY(g_TopoConnections);
    g_TopoFilterDesc.Connections      = g_TopoConnections;
}

//
// CMiniportTopology
//
class CMiniportTopology : public IMiniportTopology, public CUnknown
{
public:
    DECLARE_STD_UNKNOWN()
    DEFINE_STD_CONSTRUCTOR(CMiniportTopology)

    STDMETHODIMP_(NTSTATUS) GetDescription(PPCFILTER_DESCRIPTOR* Description) override;
    STDMETHODIMP_(NTSTATUS) DataRangeIntersection(
        ULONG PinId, PKSDATARANGE DataRange, PKSDATARANGE MatchingDataRange,
        ULONG OutputBufferLength, PVOID ResultantFormat,
        PULONG ResultantFormatLength) override;

    STDMETHODIMP_(NTSTATUS) Init(PUNKNOWN UnknownAdapter, PRESOURCELIST ResourceList,
                                  PPORTTOPOLOGY Port) override;

private:
    PPORTTOPOLOGY m_Port = nullptr;
};

STDMETHODIMP_(NTSTATUS) CMiniportTopology::NonDelegatingQueryInterface(REFIID riid, PVOID* ppVoid)
{
    if (IsEqualGUIDAligned(riid, IID_IUnknown)) {
        *ppVoid = static_cast<PUNKNOWN>(static_cast<IMiniportTopology*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IMiniport)) {
        *ppVoid = static_cast<PMINIPORT>(static_cast<IMiniportTopology*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IMiniportTopology)) {
        *ppVoid = static_cast<IMiniportTopology*>(this);
    } else {
        *ppVoid = nullptr;
        return STATUS_INVALID_PARAMETER;
    }
    reinterpret_cast<PUNKNOWN>(*ppVoid)->AddRef();
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportTopology::Init(
    PUNKNOWN, PRESOURCELIST, PPORTTOPOLOGY Port)
{
    m_Port = Port;
    Port->AddRef();
    InitTopology();
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportTopology::GetDescription(PPCFILTER_DESCRIPTOR* Desc)
{
    InitTopology();
    *Desc = &g_TopoFilterDesc;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportTopology::DataRangeIntersection(
    ULONG, PKSDATARANGE, PKSDATARANGE,
    ULONG, PVOID, PULONG)
{
    return STATUS_NOT_IMPLEMENTED;
}

// Factory
NTSTATUS CreateMiniportTopology(
    PUNKNOWN* Unknown, REFCLSID, PUNKNOWN UnknownOuter, POOL_TYPE PoolType)
{
    UNREFERENCED_PARAMETER(UnknownOuter);
    CMiniportTopology* topo = new(PoolType, 'pmTp') CMiniportTopology(nullptr);
    if (!topo) return STATUS_INSUFFICIENT_RESOURCES;
    topo->AddRef();
    *Unknown = static_cast<PUNKNOWN>(static_cast<IMiniportTopology*>(topo));
    return STATUS_SUCCESS;
}