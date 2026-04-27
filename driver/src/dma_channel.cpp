#include "driver.h"

//
// CVirtualDmaChannel — IDmaChannel backed by system memory (NonPaged).
// For virtual audio devices with no real DMA hardware.
// Replaces Port->NewMasterDmaChannel() which fails on devices without
// physical DMA resources (STATUS_DEVICE_CONFIGURATION_ERROR).
//

class CVirtualDmaChannel : public IDmaChannel, public CUnknown
{
public:
    DECLARE_STD_UNKNOWN()
    DEFINE_STD_CONSTRUCTOR(CVirtualDmaChannel)
    ~CVirtualDmaChannel();

    // IDmaChannel
    STDMETHODIMP_(NTSTATUS) AllocateBuffer(ULONG BufferSize, PPHYSICAL_ADDRESS PhysicalAddressConstraint) override;
    STDMETHODIMP_(void)     FreeBuffer() override;
    STDMETHODIMP_(ULONG)    TransferCount() override;
    STDMETHODIMP_(ULONG)    MaximumBufferSize() override;
    STDMETHODIMP_(ULONG)    AllocatedBufferSize() override;
    STDMETHODIMP_(ULONG)    BufferSize() override;
    STDMETHODIMP_(void)     SetBufferSize(ULONG BufferSize) override;
    STDMETHODIMP_(PVOID)    SystemAddress() override;
    STDMETHODIMP_(PHYSICAL_ADDRESS) PhysicalAddress() override;
    STDMETHODIMP_(PADAPTER_OBJECT)  GetAdapterObject() override;
    STDMETHODIMP_(void)     CopyTo(PVOID Destination, PVOID Source, ULONG ByteCount) override;
    STDMETHODIMP_(void)     CopyFrom(PVOID Destination, PVOID Source, ULONG ByteCount) override;

private:
    PVOID              m_Buffer      = nullptr;
    PHYSICAL_ADDRESS   m_PhysAddr    = {};
    ULONG              m_AllocSize   = 0;
    ULONG              m_BufferSize  = 0;
};

STDMETHODIMP_(NTSTATUS) CVirtualDmaChannel::NonDelegatingQueryInterface(REFIID riid, PVOID* ppVoid)
{
    if (IsEqualGUIDAligned(riid, IID_IUnknown)) {
        *ppVoid = static_cast<PUNKNOWN>(static_cast<IDmaChannel*>(this));
    } else if (IsEqualGUIDAligned(riid, IID_IDmaChannel)) {
        *ppVoid = static_cast<IDmaChannel*>(this);
    } else {
        *ppVoid = nullptr;
        return STATUS_INVALID_PARAMETER;
    }
    reinterpret_cast<PUNKNOWN>(*ppVoid)->AddRef();
    return STATUS_SUCCESS;
}

CVirtualDmaChannel::~CVirtualDmaChannel()
{
    FreeBuffer();
}

STDMETHODIMP_(NTSTATUS) CVirtualDmaChannel::AllocateBuffer(ULONG BufferSize, PPHYSICAL_ADDRESS)
{
    if (m_Buffer) FreeBuffer();

    // NonPaged pool — virtual device doesn't need physically contiguous memory
    m_Buffer = ExAllocatePool2(POOL_FLAG_NON_PAGED, BufferSize, 'pmDB');
    if (!m_Buffer) return STATUS_INSUFFICIENT_RESOURCES;

    m_PhysAddr.QuadPart = 0;  // no real physical address for virtual device
    m_AllocSize = BufferSize;
    m_BufferSize = BufferSize;
    RtlZeroMemory(m_Buffer, BufferSize);
    return STATUS_SUCCESS;
}

STDMETHODIMP_(void) CVirtualDmaChannel::FreeBuffer()
{
    if (m_Buffer) {
        ExFreePoolWithTag(m_Buffer, 'pmDB');
        m_Buffer = nullptr;
    }
    m_AllocSize = 0;
    m_BufferSize = 0;
}

STDMETHODIMP_(ULONG) CVirtualDmaChannel::TransferCount()   { return m_BufferSize; }
STDMETHODIMP_(ULONG) CVirtualDmaChannel::MaximumBufferSize() { return m_AllocSize; }
STDMETHODIMP_(ULONG) CVirtualDmaChannel::AllocatedBufferSize() { return m_AllocSize; }
STDMETHODIMP_(ULONG) CVirtualDmaChannel::BufferSize()       { return m_BufferSize; }

STDMETHODIMP_(void) CVirtualDmaChannel::SetBufferSize(ULONG BufferSize)
{
    if (BufferSize <= m_AllocSize) m_BufferSize = BufferSize;
}

STDMETHODIMP_(PVOID) CVirtualDmaChannel::SystemAddress()    { return m_Buffer; }
STDMETHODIMP_(PHYSICAL_ADDRESS) CVirtualDmaChannel::PhysicalAddress() { return m_PhysAddr; }
STDMETHODIMP_(PADAPTER_OBJECT) CVirtualDmaChannel::GetAdapterObject() { return nullptr; }

STDMETHODIMP_(void) CVirtualDmaChannel::CopyTo(PVOID Dest, PVOID Src, ULONG ByteCount)
{
    RtlCopyMemory(Dest, Src, ByteCount);
}

STDMETHODIMP_(void) CVirtualDmaChannel::CopyFrom(PVOID Dest, PVOID Src, ULONG ByteCount)
{
    RtlCopyMemory(Dest, Src, ByteCount);
}

// Factory — allocates CVirtualDmaChannel and pre-allocates the buffer.
NTSTATUS CreateVirtualDmaChannel(ULONG BufferSize, PDMACHANNEL* OutChannel)
{
    CVirtualDmaChannel* ch = new(NonPagedPool, 'pmDC') CVirtualDmaChannel(nullptr);
    if (!ch) return STATUS_INSUFFICIENT_RESOURCES;

    ch->AddRef();
    NTSTATUS status = ch->AllocateBuffer(BufferSize, nullptr);
    if (!NT_SUCCESS(status)) {
        ch->Release();
        return status;
    }

    *OutChannel = static_cast<PDMACHANNEL>(ch);
    return STATUS_SUCCESS;
}
