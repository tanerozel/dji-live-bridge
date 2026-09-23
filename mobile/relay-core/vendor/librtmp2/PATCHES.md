# Local compatibility patch

This is `librtmp2` 0.8.1 under its MIT license, reduced to the source files needed by
the Android relay.

DJI Live Bridge uses the same single-segment ingest URL as the desktop product:
`rtmp://<device-ip>:1935/drone`. FFmpeg and compatible RTMP clients encode that URL as
application `drone` with an empty publish name. Upstream rejects all empty publish names
before calling the server authorization callback.

The local patch in `src/session/conn.rs` permits an empty publish name only when the
embedding server installed an authorization callback. The app callback then accepts only
the exact `(app="drone", stream_name="")` route. Empty names remain rejected for servers
without an authorization callback.
