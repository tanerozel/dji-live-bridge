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

## Platform certificate verification (iOS)

iOS keeps its trust store to itself, so OpenSSL has no CA file to verify RTMPS servers with.
`transport::set_peer_chain_verifier` installs a function that receives the chain the server
sent (DER, leaf first) and the host name. A client connection given no CA file then lets OpenSSL
finish the handshake without its own verification and calls that function before returning the
transport; a chain it rejects fails the connect with `ErrorCode::Handshake`, before any RTMP
byte is sent. The relay installs a verifier on Apple systems only (`SecTrustEvaluateWithError`
with an SSL policy for the host name). A CA file, `insecure` and servers are unaffected.
