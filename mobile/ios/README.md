# StreamMyDrone for iOS

Native iOS app for receiving an RTMP publish from DJI Fly on a DJI RC 2 and forwarding the
stream to one or more RTMP platforms at the same time. It is the Android app
([`mobile/android`](../android/README.md)) on the iPhone: the same Rust relay, the same screens,
the same strings in the same 15 languages.

```text
DJI RC 2 -> rtmp://<iphone-ip>:1935/drone -> iPhone -> one or more RTMP targets
```

No DJI SDK, no WebView. SwiftUI for the screen, the Rust relay core
([`mobile/relay-core`](../relay-core)) for everything that touches the stream, linked as a static
library through a small C interface (`relay-core/src/ffi.rs`, [`RelayCore/relay_core.h`](RelayCore/relay_core.h)).
Video and audio are copied, never decoded or re-encoded on their way to the platforms.

## What it shares with Android

- **The relay.** Ingest on `:1935/drone` (DJI Fly's librtmp habits included), publisher takeover
  on `releaseStream`, the 20-second hold when the drone drops, one output per platform with its
  own retries, congestion skip-to-keyframe and stall detection, the GOP-cached preview tap, the
  stall and reconnect counters. All of it is the same Rust code.
- **The screen.** One dark camera-style screen: device card and state card at the top, picture
  size / bitrate / network chips, the connect card with the DJI Fly address until the picture
  comes, a switch per platform ("1/4 active"), Preview / Go live / More, one closable note at a
  time (go-live failures, drone lost, weak link, the platform's own tip). Whole picture with a
  blurred backdrop or filling the screen, double tap to switch, remembered. The three-page guide,
  the key screen with prefilled server addresses, the platforms sheet, the technical details,
  the five desktop themes and the language picker.
- **The platforms.** Instagram, TikTok, YouTube, Facebook, Twitch, Kick and custom RTMP/RTMPS,
  with the same default ingest addresses and brand tiles.
- **The test video.** Pick a video; one DJI Fly could not have sent (HEVC, HDR, rotated, over
  720p/31 fps/5 Mbps, or without AAC sound) is converted first to H.264 High 4.0, 720-pixel short
  side, 30 fps, 4 Mbps, a keyframe every second, no B-frames, AAC (silence when it has none), from
  its first 60 seconds, and cached. It is then published over loopback into the app's own
  receiver exactly as DJI Fly would.
- **The strings.** `scripts/import-android-strings.mjs` builds `Resources/<lang>.lproj` from the
  Android `values*/strings.xml` plus the few iOS-only strings in
  [`strings/ios-overrides.json`](strings/ios-overrides.json). Change a string on Android, run the
  script, commit both. CI fails when they differ, and `TranslationsTests` fails on a missing string
  or placeholder.

## Where iOS differs

| | Android | iOS |
| --- | --- | --- |
| Stream keys | AES-GCM with an Android Keystore key | Keychain items, this device only, readable while unlocked, never synced or backed up |
| RTMPS | System CAs exported to a temporary PEM for OpenSSL | OpenSSL does the handshake, then the chain is evaluated with `SecTrustEvaluateWithError` for the host name, before any RTMP byte is sent (see `vendor/librtmp2/PATCHES.md`) |
| Preview | MediaCodec onto a SurfaceView | VideoToolbox in real-time mode onto an `AVSampleBufferDisplayLayer`, frames shown on arrival, late frames skipped |
| In the background | Foreground service with a notification | Picture in Picture while live (below) |
| Key screen privacy | `FLAG_SECURE` | The key stays masked; the screen is covered while recorded or mirrored and in the app switcher |
| Language | Android 13+ per-app language | In-app picker, applied at once; "Phone's language" also follows iOS Settings → Stream My Drone → Language |
| Phone Wi-Fi in details | Signal, band and link speed | Not available to apps on iOS; only "this phone's hotspot" is shown |

### Background

iOS suspends an app shortly after it leaves the screen, which would freeze the receiver and every
platform connection. So:

- While live, leaving the app moves the drone's picture into **Picture in Picture**; an app that
  plays Picture in Picture keeps running, and the broadcast goes on. This is why the app declares
  the `audio` background mode (Picture in Picture requires it; the app plays no sound and mixes
  with other audio).
- Without Picture in Picture (closed, or not allowed in Settings) iOS gives about 30 seconds. After
  about 18 a notification asks the user to come back; when the time is up, the broadcast is ended
  cleanly (the platforms see a proper end, not a timeout) and the receiver closes. Back in the app,
  a note says what happened.
- Not live, the receiver closes when the grace period ends and opens again when the app returns.
- The screen stays on while the picture shows, as on Android.

The notification permission is asked for the first time the user goes live, only for that warning.

## Build

Requirements: Xcode 26 or later (iOS 17 SDK or later), and the Rust toolchain pinned in
[`rust-toolchain.toml`](../../rust-toolchain.toml) with the iOS targets:

```sh
rustup target add --toolchain 1.98.1 aarch64-apple-ios aarch64-apple-ios-sim
```

Open `mobile/ios/StreamMyDrone.xcodeproj` and run. The first build phase
(`scripts/build-relay-core.sh`) builds the Rust relay for the device or the simulator and
puts `libdji_relay_core.a` next to the app's other products; the first build takes a minute or two
for the vendored OpenSSL, later ones only recompile what changed. From the command line:

```sh
cd mobile/ios
xcodebuild test -project StreamMyDrone.xcodeproj -scheme StreamMyDrone \
  -destination 'platform=iOS Simulator,name=iPhone 16'
```

Signing uses team `GXDXLCQ92M` with automatic provisioning and the bundle id
`com.streammydrone.app` (the Android application id). The simulator has no DJI RC 2, but the test
video works there: it needs no Wi-Fi.

### Files

- `StreamMyDrone/App`: the app, `RootView` and `AppModel` (the screen's logic).
- `StreamMyDrone/Relay`: the relay controller (the Android service's counterpart), its state,
  and `NativeRelay` over the C interface.
- `StreamMyDrone/Media`: FLV/H.264/AAC helpers, the loopback RTMP publisher, the test video
  converter and streamer, the preview decoder and picture.
- `StreamMyDrone/Storage`: saved platforms and Keychain keys, UI preferences.
- `StreamMyDrone/UI`: the drone screen and its parts, the sheets, the guide, the key screen.
- `StreamMyDrone/Resources`: generated strings, the asset catalog (platform glyphs and the drone
  symbol converted from the Android vector drawables, the app icon from
  `branding/streammydrone-play-store.svg`), the privacy manifest.
- `StreamMyDroneTests`: phases, snapshot parsing, formatting, networks, picture fit, test video
  rules, FLV/RTMP/AMF, the profile store and the translations.

The project uses folder-synchronized groups: a file added under `StreamMyDrone/` or
`StreamMyDroneTests/` is part of the target without editing the project.

## Not yet verified on a device

The relay, the C interface, the RTMPS check, the test video converter and streamer, the preview
decoder and a live output to MediaMTX were exercised end to end on macOS (the same frameworks).
What only an iPhone can show still needs a field test: DJI Fly on an RC 2 connecting over Wi-Fi
and to the iPhone's Personal Hotspot (`172.20.10.1`), whether iOS asks for local network access
for incoming connections, and a long broadcast carried by Picture in Picture.
