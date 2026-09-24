# DJI Live Bridge for Android

Native Android application for receiving an RTMP publish from DJI Fly on a DJI RC 2 and
forwarding the stream to one external RTMP destination.

The Android app does not use DJI SDK, Tauri, or a WebView. Phase 8 contains a secure,
reconnecting single-target stream-copy relay hosted by an Android foreground service:

```text
DJI RC 2 -> rtmp://<android-ip>:1935/drone -> Android -> external RTMP target
```

The screen shows independent ingest/output state, source address, received and forwarded bytes,
bitrate, codecs and packet counts. Video/audio packets are forwarded without decoding or
re-encoding. Both `rtmp://` and `rtmps://` destinations are supported. RTMPS uses the current
Android platform trust roots, validates the full certificate chain and hostname, and provides no
insecure bypass. The temporary public-CA bundle is stored only in the app's non-backed-up private
directory while the service is active and is deleted on stop.

Instagram, TikTok, YouTube, Facebook, Twitch, Kick and custom RTMP destinations can be saved as
profiles. Profile metadata plus the encrypted credential blob are written atomically to the app's
non-backed-up private directory. Each stream key is encrypted with AES-256-GCM using a
non-exportable Android Keystore key and a fresh, random IV; the profile ID is authenticated as
additional data. Plaintext keys are never written to storage, logs, notifications or intent extras.
The activity passes only the selected profile ID to the service, which decrypts the key immediately
before starting the native relay. Editing a profile without entering a new key preserves the
existing encrypted credential.

The service is started only by the user's button, uses a persistent status notification with a Stop
action, and returns `START_NOT_STICKY` so Android cannot restart a stopped relay without a new user
action. Profile selection and editing are locked while a relay is active. The storage file is
excluded from cloud backup and device transfer; if the Keystore key is unavailable or invalidated,
the app reports the condition instead of replacing or exposing the saved ciphertext.

The DJI Fly ingress URL deliberately matches the desktop product's fixed `/drone` path. It has no
separate inbound password, so the listener should only be used on a trusted local Wi-Fi network and
is available only while the user-started foreground service is running. The vendored `librtmp2`
patch allows an explicitly authorized application-only RTMP route (`app=drone`, empty publish name),
which is how clients such as FFmpeg encode the single-segment `/drone` URL.

## User interface

The home screen is two cards and one button. The first card is the platform grid: Instagram,
TikTok, YouTube, Facebook, Twitch, Kick and a custom RTMP server, drawn as the desktop app's brand
tiles. Tapping a platform opens a key-only screen whose server address is prefilled with the
platform's published ingest; TikTok and custom servers hand out their own address, so they ask for
it. The prefilled address can still be changed:

| Platform | Default server address |
| --- | --- |
| Instagram | `rtmps://live-upload.instagram.com:443/rtmp` |
| YouTube | `rtmps://a.rtmps.youtube.com/live2` |
| Facebook | `rtmps://live-api-s.facebook.com:443/rtmp` |
| Twitch | `rtmp://live.twitch.tv/app` |
| Kick | `rtmps://fa723fc1b171.global-contribute.live-video.net:443/app` (the dashboard value wins if it differs) |

The second card shows the RTMP address to type into DJI Fly on the RC 2, with the DJI Fly menu path.
While the bridge runs, the screen shows one status (waiting for the remote, live with a timer,
reconnecting, error), the remote → phone → platform hops, bitrate, sent bytes and codecs, and the
platform's own go-live reminder; the counters stay under "Teknik ayrıntılar". A three-page guide
opens on first launch and again from the help button.

"Test videosuyla dene" goes live without a drone, like the desktop app's test video. The picked
H.264/AAC video is published over loopback to the app's own `rtmp://127.0.0.1:1935/drone` ingest,
exactly as DJI Fly would: legacy handshake, `connect`/`createStream`/`publish`, answers to
librtmp2's pings, samples copied without re-encoding, paced in real time and looped. It therefore
takes the same relay and destination path as a real flight and needs no Wi-Fi. B-frames work
(decode timestamps are rebuilt from the sorted presentation times); H.265/HEVC files are refused
with an explanation, because DJI Fly sends H.264. Phone videos shot in portrait may appear sideways,
since FLV carries no rotation.

The themes match the desktop app: Açık (the default), Koyu, Gece mavisi, Kum and Sistem, picked
from the palette button and stored on the device. Every theme's text/background pairs meet WCAG AA
contrast, and the status bar follows the chosen theme rather than the system setting.

The network card prefers the Wi-Fi client address, then the phone's own hotspot, and never offers
mobile-data, VPN or 464XLAT addresses, which the RC 2 cannot reach. The address refreshes every few
seconds while the app is visible. The key screen sets `FLAG_SECURE` while it is open, so stream
keys stay out of screenshots, screen recordings and the recents thumbnail. The launcher, themed and
notification icons are vector versions of the desktop icon (`src-tauri/icons/source.svg`).

## Phase 7 device validation

The debug build was exercised on a Samsung SM-S911B running Android 16 (API 36). The
single-segment `/drone` ingest accepted H.264/AAC over both an ADB USB tunnel and direct Wi-Fi,
reported live bitrate plus codec/packet counters, survived activity backgrounding, and retained
its foreground service, partial wake lock and listener while the device entered doze. The profile
was reloaded after a process stop; its metadata remained readable while the plaintext test key was
absent from the private profile file. Graceful stop released the service, wake lock and port 1935.

The physical-device pass also corrected Android 16 safe-area handling and made the profile editor
scrollable/compact while the keyboard is visible. A longer RC 2 stream against a real external
destination remains the final end-to-end field test.

Moving the app to the background, switching apps or locking the screen no longer ties relay
lifetime to `MainActivity`. While active, the service holds a six-hour partial wake lock so the
CPU can continue handling the live stream after screen lock; every stop/error/timeout path releases
it. On Android 15 and newer, the platform limits `dataSync` foreground services to a total of six
background hours per 24-hour period; the service handles that timeout by stopping the native relay
and releasing its sockets. Battery/vendor settings can still impose additional device-specific
restrictions.

When the destination connection drops, the relay retries with bounded exponential backoff
(1, 2, 4, 8, then 15 seconds). It caches only public codec/metadata headers, waits for the next
video keyframe after reconnect, rebases timestamps for the new RTMP session and exposes reconnect
and dropped-packet counters in the UI. A source restart is no longer required after a temporary
target or Wi-Fi interruption.

## Toolchain

- Android Studio Quail 4 Patch 1 (2026.1.4.8)
- Android Gradle Plugin 9.4.1
- Gradle 9.6.0 (through the checked-in wrapper)
- Android SDK 36.1, target SDK 36, minimum SDK 28
- Java 17 bytecode
- Rust `aarch64-linux-android` target and `cargo-ndk`

## Build

```sh
cd mobile/android
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" ./gradlew assembleDebug lintDebug
```

The Gradle build compiles and packages the native `arm64-v8a` RTMP library automatically. The
debug APK is written to `app/build/outputs/apk/debug/app-debug.apk`.

## Phase 8 release packaging

The Android SDK/NDK and Rust/cargo-ndk versions are pinned, and CI runs Rust formatting, linting and
tests together with Android lint plus debug APK, minified release APK and release AAB builds. CI
artifacts are deliberately unsigned.

Distribution builds are signed only from environment variables; passwords are never accepted as
Gradle command-line properties or stored in the repository. Keep the release/upload keystore and
its backup outside the checkout, then export these values locally:

```sh
export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
export DJI_ANDROID_KEYSTORE_PATH="/absolute/path/to/private-release-key.jks"
export DJI_ANDROID_KEYSTORE_PASSWORD="..."
export DJI_ANDROID_KEY_ALIAS="..."
export DJI_ANDROID_KEY_PASSWORD="..."
```

Build and cryptographically verify both distributable formats with:

```sh
cd mobile/android
./scripts/build-release.sh
```

The script produces `app/build/outputs/apk/release/app-release.apk` and
`app/build/outputs/bundle/release/app-release.aab`, verifies both signatures and prints their
SHA-256 digests. Never commit a keystore or its credentials. Losing the signing key prevents future
updates outside Play App Signing, so creating and backing up the production key remains an explicit
release-owner step.
