#pragma once

extern "C" {
#include <wdm.h>
}
#include <portcls.h>
#include <stdunk.h>
#include <ksmedia.h>

// Audio format constants â€” must match Android side
#define PhoneMike_SAMPLE_RATE     48000
#define PhoneMike_CHANNELS        1
#define PhoneMike_BITS_PER_SAMPLE 16
#define PhoneMike_BLOCK_ALIGN     (PhoneMike_CHANNELS * PhoneMike_BITS_PER_SAMPLE / 8)
#define PhoneMike_BYTE_RATE       (PhoneMike_SAMPLE_RATE * PhoneMike_BLOCK_ALIGN)

// DMA buffer: 40ms of audio at 48kHz mono 16-bit = 3840 bytes
// (larger buffer absorbs jitter from Windows ~15.6ms timer resolution)
#define PhoneMike_BUFFER_FRAMES   1920
#define PhoneMike_BUFFER_BYTES    (PhoneMike_BUFFER_FRAMES * PhoneMike_BLOCK_ALIGN)

// Notification interval: 10ms
#define PhoneMike_NOTIFICATION_INTERVAL_100NS  100000

// Bytes per notification tick: 10ms at 48kHz mono 16-bit = 960 bytes
#define PhoneMike_NOTIFICATION_BYTES  ((PhoneMike_BYTE_RATE / 1000) * (PhoneMike_NOTIFICATION_INTERVAL_100NS / 10000))

// Virtual DMA channel â€” system memory, no real DMA hardware
NTSTATUS CreateVirtualDmaChannel(ULONG BufferSize, PDMACHANNEL* OutChannel);