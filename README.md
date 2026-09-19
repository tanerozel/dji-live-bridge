# DJI Live Bridge

**English** · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

A macOS app that receives the RTMP stream from DJI Fly on a Mac in the same LAN, shows a local preview through MediaMTX, and feeds a built-in FFmpeg production pipeline that needs no OBS. Rust only handles processes, state, security and orchestration; video frames never pass through Rust. OBS is an optional advanced integration.

## What it does

1. Finds the active default-route IPv4 interface; loopback, link-local and `utun`-style interfaces are excluded from the default choice.
2. Verifies and starts the bundled MediaMTX.
3. Shows `rtmp://<LAN_IPV4>:1935/drone` and a QR code for the DJI RC 2.
4. Watches the `/drone` publisher through the MediaMTX Control API; it never restarts MediaMTX just to reconnect.
5. Tries a direct WHEP preview. When a codec/ICE problem appears, or preview audio for AAC is required, it can start an H.264 + Opus FFmpeg fallback for `drone-preview` only.
6. The built-in production pipeline applies the layout with VideoToolbox or libx264 based on an FFmpeg capability probe, mixes drone audio with an optional AVFoundation microphone, and produces the `/production` path.
7. Direct RTMP starts only with `START LIVE`. One production encode is forwarded to several enabled Instagram/TikTok/custom RTMP destinations at once. Each stream key is read from the Keychain into memory, handed to a temporary MediaMTX forward list over the loopback Control API, and removed on `STOP LIVE`; it never appears in FFmpeg argv.
8. The bundled Core Media I/O camera extension publishes `/drone` video as a 1080×1920/30 fps video-only camera named `DJI Live Bridge Camera`. TikTok LIVE Studio picks its microphone itself; OBS is not needed.
9. The optional OBS WebSocket 5.x integration does runtime feature detection with `GetVersion.availableRequests`; scene, recording and OBS Virtual Camera features are available when OBS is installed.

DJI Fly path: **GO FLY → Transmission → Live Streaming Platforms → RTMP**. RC 2 / DJI Fly 1.16+ may require a separate microphone connected to the RC 2 before it starts streaming. That microphone is not the Mac/OBS commentary microphone.

RC 2 input can be 720p, and the UI shows the real ffprobe metadata. Choosing a 1080p or portrait production layout upscales the 720p input; the app never presents that as native 1080p input.

## Install (users)

1. Download the latest `.dmg` from [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Drag **DJI Live Bridge** into **Applications**. The virtual camera only works from there.
3. FFmpeg is not bundled. On first run the app detects it is missing and installs it for you with one click (through Homebrew), showing the progress log. To do it yourself instead: `brew install ffmpeg`.

The build is signed and notarized by Apple, so it opens without warnings.

**Requirements:** Apple Silicon Mac (Intel build path is kept), macOS 13 or newer, and FFmpeg/ffprobe 8.1.2 or newer — installed for you on first run, or with `brew install ffmpeg`.

## Stream to Instagram / TikTok (Direct RTMP)

The `Go live` tab is a single screen with three steps:

1. **Connect your drone** — paste the shown `rtmp://…/drone` address into DJI Fly (or use `Try with a video file`). The step turns green when video arrives.
2. **Where do you want to stream?** — pick Instagram, TikTok or Custom RTMP and paste the server URL and stream key from the platform. On Instagram: instagram.com → Create (+) → Live video. Instagram issues a new key for every broadcast; refresh it with `Update key` on the destination card. When several destinations are enabled, the same picture goes to all of them at once.
3. **Picture and sound** — `Vertical 9:16` (the default for Instagram and TikTok), framing, and an optional microphone. The choices are remembered.

The `START LIVE` button on the right unlocks once three conditions are met (local server, drone video, a selected destination); it prepares the production pipeline and starts the stream in one click. While live you see the elapsed time, each destination's state (`Live` / `Connecting` / `Connection failed`) and the data sent; one failing destination does not stop the others. On Instagram you still have to press `Go live` there to make the broadcast public. Press `End stream` to finish.

The app never signs in to a platform account and never clicks through the platform's own start/confirm screens.

## TikTok LIVE Studio — without OBS

This flow needs no TikTok server URL or stream key:

1. Move the signed `DJI Live Bridge.app` to `/Applications` and open it from there.
2. Connect the Mac and the DJI controller to the same phone hotspot. Use the `rtmp://<hotspot-IP>:1935/drone` address shown in the app as the DJI Fly streaming address.
3. Start the DJI Fly stream, or verify the picture with `Try with a video file`.
4. In the `TikTok LIVE Studio` tab, press `Enable Virtual Camera` once.
5. If macOS asks, allow the system extension under **System Settings → General → Login Items & Extensions → Camera Extensions**. When the card reads `Ready to start`, press `Start Virtual Camera`.
6. Open TikTok LIVE Studio with the `Open / download LIVE Studio` button.
7. In TikTok LIVE Studio add a Camera source and select **DJI Live Bridge Camera**.
8. Set that source's `More settings → Audio capture` to `None`.
9. Select exactly one physical microphone in TikTok LIVE Studio's main microphone control. This camera deliberately carries video only; DJI Live Bridge captures no audio and sends none to TikTok in this mode.
10. Use headphones if you monitor the stream, so the microphone does not pick the speakers up again.

The picture path is:

```text
DJI / Test Drone → MediaMTX /drone → FFmpeg NV12 1080×1920@30
                 → Localhost NV12 stream → Core Media I/O Camera Extension
                 → DJI Live Bridge Camera → TikTok LIVE Studio
```

Camera activation is part of the macOS security model: the app must live in `/Applications`, and the first use needs an administrator's approval. Replacing the app in `/Applications` makes macOS deactivate the extension, so press `Start Virtual Camera` again afterwards — the button re-requests activation itself. `Go Live` stays manual inside TikTok LIVE Studio.

## Test Drone

`Try with a video file` picks a local video and uses the real pipeline:

```text
file → FFmpeg H.264/AAC (long side 1280, 30 fps) → rtmp://127.0.0.1:1935/drone
     → MediaMTX → WHEP preview
     → FFmpeg layout/audio mix → rtsp://127.0.0.1:8554/production
     → MediaMTX runtime forwards → Instagram + TikTok + custom RTMP
```

No fake `connected` state is produced. If FFmpeg/ffprobe is missing, the feature is reported as unavailable.

## Languages and themes

The app ships in English, Türkçe, Español, 中文 (Simplified and Traditional), العربية, हिन्दी, Português, Русский, Français, Deutsch, 日本語, 한국어, Bahasa Indonesia and Italiano. On first run it follows the system language and falls back to English. The picker sits in the header and the choice is stored locally. Arabic switches the whole layout to right-to-left, while technical values (URLs, ports, codecs) stay left-to-right. Device names, codecs, protocols and raw error details are never translated, so diagnostics stay accurate.

Five themes are available: System (follows macOS), Light, Dark, Midnight and Sand.

## Built-in production and streaming behaviour

- The default engine is `NativeFfmpeg`. Landscape is `1920x1080`, portrait is `1080x1920`; framing defaults to `Fit` with no crop.
- Picture quality: constant 30 fps output, 6 Mbps CBR, H.264 High, 2-second keyframes (required by Instagram/Facebook ingest). The VideoToolbox hardware encoder is preferred, which leaves the CPU free so the RTSP reader never falls behind; libx264 is the fallback. `Whole picture` fills the empty area with a blurred copy of the video instead of black bars, and scaling uses lanczos. The MediaMTX reader queue (`writeQueueSize`) is 4096 so a short stall does not drop frames.
- Test Drone keeps the video's own orientation (long side 1280, 30 fps); a vertical test clip is no longer squeezed into a landscape frame.
- The microphone is captured through AVFoundation only when the user selects one. Volume, non-negative sync delay, FFT noise reduction, compressor and limiter are applied subject to a capability probe. The macOS Microphone permission is requested only then.
- When the drone has no audio track and no microphone is selected, valid AAC stereo silence is produced. With a microphone selected it is mixed with the drone audio through `amix`.
- Recording writes the real `/production` mix while live, otherwise the `/drone` stream, as a stream-copy MKV under `~/Movies/DJI Live Bridge/`.
- Each Instagram/TikTok/custom RTMP stream key lives in its own macOS Keychain entry. The destination list is added at runtime through the MediaMTX Control API; keys never reach the config file or a child process argv.
- The single FFmpeg production encode is forwarded by MediaMTX to each enabled destination separately. Every destination's `state`, `lastError` and `outboundBytes` come from real Control API data; one failing destination does not stop another.
- Multi-destination streaming uses the built-in `NativeFfmpeg` engine. The optional OBS engine is limited to one RTMP destination from inside this app.

## Optional OBS behaviour

- The source type `ffmpeg_source` is verified in the OBS `GetInputKindList` reply; defaults come from `GetInputDefaultSettings`.
- The OBS media source URL is `rtsp://127.0.0.1:8554/drone`.
- The Virtual Camera carries video only and the FFmpeg feed disables audio explicitly with `-an`. Keep `Audio capture = None` on the TikTok LIVE Studio camera source; the user picks a single microphone in LIVE Studio's main audio control. The app installs no driver and no virtual audio device.
- CoreAudio input devices are watched without starting a capture, and connect/disconnect changes reach the UI through a Tauri state event.
- If the OBS engine is explicitly prepared, Direct RTMP carries the OBS video/audio mix; snapshot/restore behaviour of the stream service is preserved.
- The stream service settings are never changed while a stream is active. A snapshot is taken before `START LIVE`; `STOP LIVE`, a source disconnect and app shutdown all try to restore it.
- If OBS is missing, the app opens the official download page. OBS checks are never reported as passing on a machine without it.
- Recording is managed idempotently through the `GetRecordStatus` / `StartRecord` / `StopRecord` calls OBS advertises at runtime. The output path comes from the active OBS profile and the real `outputPath` from the `StopRecord` reply is shown; it is never forced through undocumented profile keys.

TikTok LIVE Studio's current official download page offers a macOS 12+ build. The app discovers `/Applications/TikTok LIVE Studio.app`, opens it when present and points to the official page otherwise. The OBS-free camera comes from the Core Media I/O camera extension in this repository. An unsigned/ad-hoc build is no substitute for an Apple Developer Team distribution signature and is never reported as a successful activation. There is no TikTok login, private API, bypass or Go Live automation.

## Security and local ports

| Port | Bind | Purpose |
| --- | --- | --- |
| 1935 | LAN (`:1935`) | DJI Fly RTMP ingest |
| 8554 | `127.0.0.1` | FFmpeg/OBS RTSP reader and `/production` publisher |
| 8889 | `127.0.0.1` | WHEP preview |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX Control API |
| 9998 | `127.0.0.1` | MediaMTX metrics |

The Tauri CSP allows only the localhost WHEP address it needs. Capabilities are limited to core and the video file picker. `NSLocalNetworkUsageDescription` is present, and `NSMicrophoneUsageDescription` for the optional built-in commentary capture.

Non-secret config: `~/Library/Application Support/DJI Live Bridge/`
Logs: `~/Library/Logs/DJI Live Bridge/`
Secrets: macOS Keychain

## Development

```sh
npm ci
rustup toolchain install 1.98.1
rustup target add aarch64-apple-darwin x86_64-apple-darwin --toolchain 1.98.1
./scripts/download-mediamtx.sh arm64        # x86_64 for the Intel sidecar
npm run tauri -- dev
```

The MediaMTX binary is not checked in; the script downloads the official release archive, verifies it against the official SHA-256 list and installs it under the `mediamtx-<target-triple>` name Tauri requires.

Toolchain versions are recorded in [versions.lock.json](versions.lock.json); variable dependency sources and checksums are in [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md). Building the native camera extension needs the Xcode Command Line Tools with the macOS SDK. Agent-facing build rules live in [AGENTS.md](AGENTS.md).

## Verification and production build

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run typecheck
npm run lint
npm run build
npm run build:mac
```

> **`npm run build:mac` is the only supported build command.** A bare `tauri build` is blocked (`scripts/tauri.sh`), because Tauri applies the app's restricted entitlements (`system-extension.install`, `application-groups`) to the `mediamtx` sidecar as well. Without a provisioning profile macOS kills that sidecar at launch with `SIGKILL` (exit 137), and the app then fails with `error sending request for url (http://127.0.0.1:9997/v3/paths/list)`. `build:mac` re-signs the sidecar with empty entitlements, verifies the bundle inside both the `.app` and the `.dmg` with `scripts/verify-bundle.sh`, and refuses to call an unverified build successful. To audit an installed app: `npm run verify:bundle -- "/Applications/DJI Live Bridge.app"`.

RTMP config serialization, secret separation, URL validation and the case where one destination fails while another keeps running are covered by Rust unit tests. Streaming to a real Instagram/TikTok account still has to be verified by a person with valid platform keys.

## Packaging, signing and notarization

Because of Tauri's `externalBin` target-triple rule, the correct sidecar file must exist for each target before the build. The MediaMTX executable bit (`0755`) must be preserved. `.app` and `.dmg` generation pulls the sidecar in; a build meant for distribution needs Apple Developer ID signing and notarization covering both the main executable and the sidecar.

**Notarization is mandatory, not optional.** macOS only activates a Developer ID signed system extension when the app is notarized; otherwise the virtual camera fails with `code=8 domain=OSSystemExtensionErrorDomain desc=code signature invalid`. `npm run build:mac` notarizes and staples both the `.app` and the `.dmg` through the notarytool keychain profile `dji-live-bridge` (override with `NOTARY_PROFILE`). Create the profile once with `xcrun notarytool store-credentials`; never commit the `.p8` key. `NOTARIZE=0` is only for quick UI-only iteration.

The sidecar signature is corrected by `scripts/sign-sidecars.sh` using the empty `src-tauri/entitlements-sidecar.plist`; this runs inside `npm run build:mac` and should not be called by hand. The DMG is produced by `build:mac` from the fixed `.app` rather than by Tauri's own DMG step, which would otherwise ship the broken sidecar. On exit (including `RunEvent::Exit`) the app stops all child processes, and at startup it clears a leftover MediaMTX started with its own config file.

`beforeBundleCommand` compiles the Swift camera extension and places it under `Contents/Library/SystemExtensions/`. In production `APPLE_SIGNING_IDENTITY` overrides the Tauri setting and the extension uses the same identity. The production bundle must keep the System Extension + App Group entitlements from the app's `entitlements.plist` and the same App Group entitlement on the extension. After notarization, `codesign --verify --deep --strict` and a real user-approved activation test on macOS are still required.

FFmpeg is not bundled, so the app redistributes nothing; the in-app installer only runs the user's own Homebrew. The LGPL/GPL options and codec licences of the user's installed build belong to that build's distributor. If FFmpeg is ever bundled, binary provenance, configure flags, the source offer and the related LGPL/GPL obligations must be tracked per release.

## Licence

MIT — see [LICENSE](LICENSE).

## About

**Taner Özel** — Developer and maintainer

- E-mail: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

This project is not affiliated with DJI, Instagram or TikTok.
