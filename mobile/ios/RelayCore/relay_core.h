// The Rust relay (mobile/relay-core/src/ffi.rs) as the iOS app sees it.
//
// Strings are NUL-terminated UTF-8, borrowed for the call. A returned string belongs to the
// caller and is freed with djr_string_free; preview frame data with djr_bytes_free. Errors are
// codes such as "receiver_not_running", never user-facing text.

#ifndef DJI_RELAY_CORE_H
#define DJI_RELAY_CORE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/// Starts receiving DJI Fly on port 1935 with no platform attached; an error code or NULL.
char *_Nullable djr_start_receiver(void);

/// Sends the received stream to one more platform, known by output_id; an error code or NULL.
/// RTMPS certificates are checked against the system's trust store.
char *_Nullable djr_go_live(const char *_Nonnull output_id,
                            const char *_Nonnull server_url,
                            const char *_Nonnull stream_key);

/// Stops sending to the platform output_id, or to all of them when it is NULL or "".
void djr_end_live(const char *_Nullable output_id);

/// The relay's state as JSON; never NULL.
char *_Nonnull djr_snapshot_json(void);

/// Stops every platform output and the receiver.
void djr_stop(void);

void djr_string_free(char *_Nullable value);

/// One video tag of the preview: its RTMP timestamp and the FLV video tag body.
typedef struct {
    uint32_t timestamp;
    uint8_t *_Nullable data;
    size_t length;
} DjrPreviewFrame;

/// Starts a preview session on the relay's video and returns its number.
uint64_t djr_preview_start(void);

/// Waits up to timeout_ms for the session's next video tag; false when none arrived.
bool djr_preview_next(uint64_t session, uint32_t timeout_ms, DjrPreviewFrame *_Nonnull frame);

void djr_bytes_free(uint8_t *_Nullable data, size_t length);

/// Ends the preview session; a newer one is left alone.
void djr_preview_stop(uint64_t session);

#endif
