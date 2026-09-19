// Layout of the shared memory the app writes frames into and the DirectShow
// filter reads them from. Both sides compile this header, so the two can never
// drift apart.
//
// The app is the only writer. Readers (one per app using the camera) take the
// newest complete frame; they never block the writer, and a reader that falls
// behind simply misses frames instead of stalling the stream.

#pragma once

#include <stdint.h>

// Kernel object names. "Global\" would need admin rights, so the camera works
// per user session, which is what a desktop capture app needs.
#define DJI_FRAME_MAPPING_NAME L"Local\\DJILiveBridgeFrame"
#define DJI_FRAME_EVENT_NAME L"Local\\DJILiveBridgeFrameReady"

#define DJI_FRAME_WIDTH 1080
#define DJI_FRAME_HEIGHT 1920
#define DJI_FRAME_FPS 30

// NV12: a full-size luma plane followed by half-height interleaved chroma.
#define DJI_FRAME_BYTES ((DJI_FRAME_WIDTH) * (DJI_FRAME_HEIGHT) * 3 / 2)

#define DJI_FRAME_MAGIC 0x444A4931u // "DJI1"

#pragma pack(push, 4)
typedef struct DjiFrameHeader {
    uint32_t magic;     // DJI_FRAME_MAGIC once the app has initialised it
    uint32_t width;     // frame geometry, so a reader can reject a mismatch
    uint32_t height;
    uint32_t fps;
    // Odd while a frame is being written, even when one is complete: a reader
    // that sees the same even value before and after a copy knows the frame it
    // copied was not torn.
    volatile uint32_t sequence;
    uint32_t reserved;
    uint64_t timestamp_100ns; // when the app produced the frame
} DjiFrameHeader;
#pragma pack(pop)

#define DJI_FRAME_MAPPING_BYTES (sizeof(DjiFrameHeader) + DJI_FRAME_BYTES)
