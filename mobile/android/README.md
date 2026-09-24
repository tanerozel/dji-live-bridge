# DJI Live Bridge for Android

Native Android application for receiving an RTMP publish from DJI Fly on a DJI RC 2 and
forwarding the stream to one or more external RTMP destinations at the same time.

The Android app does not use DJI SDK, Tauri, or a WebView. Phase 8 contains a secure,
reconnecting single-target stream-copy relay hosted by an Android foreground service:

```text
DJI RC 2 -> rtmp://<android-ip>:1935/drone -> Android -> one or more RTMP targets
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
The activity passes only the selected profile IDs to the service, which decrypts each key
immediately before it attaches that platform to the native relay. Editing a profile without entering a new key preserves the
existing encrypted credential.

The receiver opens whenever the app is on screen, so DJI Fly can connect and its picture shows on
the phone before anything goes anywhere; the stream reaches a platform only between the user's
"Go live" and "End broadcast". Both run in a foreground service with a status
notification whose action ends the broadcast while live and closes the receiver otherwise. Swiping
the app away closes the receiver unless a broadcast is on, and `START_NOT_STICKY` keeps Android
from restarting a stopped receiver on its own. The storage file is excluded from cloud backup and
device transfer; if the Keystore key is unavailable or invalidated, the app reports the condition
instead of replacing or exposing the saved ciphertext.

The DJI Fly ingress URL deliberately matches the desktop product's fixed `/drone` path. It has no
separate inbound password, so the listener should only be used on a trusted local Wi-Fi network; it
is open only while the receiver's foreground service runs. The vendored `librtmp2` patch allows an
explicitly authorized application-only RTMP route (`app=drone`, empty publish name), which is how
clients such as FFmpeg and DJI Fly encode the single-segment `/drone` URL.

### DJI Fly compatibility

DJI Fly's RTMP client behaves like librtmp, and two of its habits broke the vendored `librtmp2`
server until they were fixed there (sessions captured from an RC 2 showed both):

- It answers the server's ping on chunk stream 2 with a type 1 header, although nothing was sent on
  that stream before. The reader now opens an unknown chunk stream from a type 1 header (message
  stream 0), as nginx-rtmp and SRS do; type 2 and 3 headers there are still protocol errors.
- It never sends Set Chunk Size, so every message it sends stays in 128-byte chunks. The server
  used to apply its own announced size (4096) to what it read as well, which cut the first
  keyframe apart and closed the connection a second after publishing. Only our own chunks change
  size now; the peer's change only when it announces one.

`librtmp2`'s tests replay those captured bytes, and CI runs them.

## User interface

Until the drone's picture arrives, the home screen follows the order of a flight: two numbered
cards and one button. The first card, "Connect the drone", shows the RTMP address to type into DJI
Fly on the RC 2 with the DJI Fly menu path, and folds away the usual reasons it does not connect
under "Not connecting?". The second card is the platform grid: Instagram, TikTok, YouTube, Facebook, Twitch,
Kick and a custom RTMP server, drawn as the desktop app's brand tiles. Tapping a platform opens a key-only screen whose server address is prefilled with the
platform's published ingest; TikTok and custom servers hand out their own address, so they ask for
it. The prefilled address can still be changed:

| Platform | Default server address |
| --- | --- |
| Instagram | `rtmps://live-upload.instagram.com:443/rtmp` |
| YouTube | `rtmps://a.rtmps.youtube.com/live2` |
| Facebook | `rtmps://live-api-s.facebook.com:443/rtmp` |
| Twitch | `rtmp://live.twitch.tv/app` |
| Kick | `rtmps://fa723fc1b171.global-contribute.live-video.net:443/app` (the dashboard value wins if it differs) |

Tapping a saved platform adds it to the broadcast or takes it out, so one or several can be picked;
a long press edits it. The broadcast goes to all of them at once. Each platform has its own
connection with its own retries, congestion handling and 20-second hold, so a slow or failing one
never holds up the others. Each one uploads the whole stream, which is why the card shows the
combined upload once two or more are picked. A three-page guide opens on first launch and again
from the help button.

### Drone screen

Once the drone's picture arrives, it takes the whole screen, the way a camera app shows its
viewfinder, and stays there while live:

- Top: what the picture is ("Preview", or the live badge and running time) and the stream's state
  ("Ready · Not broadcasting", "Live · 4.2 Mbps", "Reconnecting", "Waiting for the drone ·
  broadcast on").
- Bottom: a card that says what is going on ("Drone and camera are ready.", "You're live on
  Instagram", "Drone disconnected") with the resolution, bitrate and network, the most useful tip
  (such as Instagram's own "Go live" reminder), and the platforms' tiles, each with a state dot once
  live. Next to it the one big button: "Go live" ("Go live on 2 platforms"), "Choose a platform" or
  "End broadcast".
- The platform tiles open a sheet: before going live the same platform grid as the home screen,
  while live the bitrate, sent bytes and codecs, each platform with its own "End", and the technical
  details. The ⋮ menu has the same sheets, language, theme and help.
- The picture is shown whole by default, and the rest of the screen shows its colors, blurred and
  dimmed (a 32×18 copy of the frame taken with `PixelCopy` every 0.7 s). A double tap or the
  full-screen button fills the screen edge to edge instead, cutting off the picture's sides when the
  phone is upright; the choice is remembered. Turning the phone sideways gives the picture the whole
  screen; the activity handles the rotation itself, so the decoder keeps running.
- A picture that stops coming is dimmed so it never looks live, and the screen waits 2.5 seconds
  before going back to the home screen, so a short drop does not flip screens. The screen stays on
  while the picture shows.

"End broadcast" ends only the broadcast: the drone stays connected, ready to go live again.

The Rust core keeps a bounded tap of the ingest's video tags for it (about three
seconds); when the viewer falls behind, the tap drops its queue and restarts from the latest codec
header and the next keyframe, so the preview can never slow the relay. Each viewer has a session
number, so a closing view cannot stop its replacement. The phone's hardware decoder renders the
H.264 (legacy AVC, which DJI Fly sends) onto a SurfaceView, only while the app is visible (see
[Latency and continuity](#latency-and-continuity)).

"No drone? Try a test video" stands in for the drone, like the desktop app's test video:
its picture shows as the drone's would, and going live works the same way. The picked H.264/AAC
video is published over loopback to the app's own `rtmp://127.0.0.1:1935/drone` ingest,
exactly as DJI Fly would: legacy handshake, `connect`/`createStream`/`publish`, answers to
librtmp2's pings, samples copied without re-encoding, paced in real time and looped. It therefore
takes the same relay and destination path as a real flight and needs no Wi-Fi. B-frames work
(decode timestamps are rebuilt from the sorted presentation times); H.265/HEVC files are refused
with an explanation, because DJI Fly sends H.264. Phone videos shot in portrait may appear sideways,
since FLV carries no rotation.

The themes match the desktop app: Light (the default), Dark, Midnight blue, Sand and System, picked
from the palette button and stored on the device. Every theme's text/background pairs meet WCAG AA
contrast, and the status bar follows the chosen theme rather than the system setting.

The network card prefers the Wi-Fi client address, then the phone's own hotspot, and never offers
mobile-data, VPN or 464XLAT addresses, which the RC 2 cannot reach. The address refreshes every few
seconds while the app is visible. The key screen sets `FLAG_SECURE` while it is open, so stream
keys stay out of screenshots, screen recordings and the recents thumbnail. The launcher, themed and
notification icons are vector versions of the desktop icon (`src-tauri/icons/source.svg`).

## Languages

The app has the desktop app's 15 languages: English (the default), Español, 中文（简体）, 中文（繁體）,
العربية, हिन्दी, Português, Русский, Français, Deutsch, 日本語, 한국어, Bahasa Indonesia, Italiano and
Türkçe. It follows the phone's language. On Android 13 and later the globe button in the top bar
picks another one, stored by Android as the app's own language (the same choice as Settings → Apps
→ DJI Live Bridge → Language, which `res/xml/locales_config.xml` feeds); it applies at once,
including to the notification. Arabic is laid out right to left, and arrows that show a direction
are mirrored.

- The strings are in `res/values*/strings.xml`: English in `values`, Indonesian in `values-in`,
  Chinese in `values-b+zh+Hans` and `values-b+zh+Hant`. The terms follow the desktop translations,
  including the DJI Fly menu path. `TranslationsTest` fails if a language misses a string, changes
  a placeholder or lacks one of its plural forms, or if the language button and
  `locales_config.xml` disagree.
- Platform names go into sentences through phrases (`on_instagram` is "on Instagram" and
  "Instagram'da", `to_instagram` is "to Instagram" and "Instagram'a"), so languages that inflect
  names read naturally. They are written for the middle of a sentence; a sentence that starts with
  one is capitalized.
- Messages from the service and the Rust core are put into words where they are shown (`UiText`),
  so switching the language also changes a message that is already on screen. The core sends codes
  (`receiver_not_running`, `listen_failed: <detail>`, output reasons such as `publish_rejected`);
  the app words them and adds the core's English technical detail in brackets.
- Numbers use the language's format ("4,2 Mbps" in Turkish, "4.2 Mbps" in English). Units stay
  as they are, as on the desktop.

## Latency and continuity

The preview aims for the lowest delay the phone allows:

- The decoder is a hardware one, preferring a component that advertises low latency (some
  Snapdragon phones list a separate `c2.qti.avc.decoder.low_latency`). It is tuned the way Moonlight's game-streaming
  client does it: `KEY_LOW_LATENCY`, the vendor switches for Qualcomm, Exynos, HiSilicon and
  Amlogic, and realtime priority. Qualcomm's decode-order output is only switched on when the SPS
  rules out reordering (`pic_order_cnt_type` 2, which DJI Fly sends). A decoder that rejects
  the tuning is started again without it.
- Input and output run on separate threads. A decoded frame is shown at once, and when several
  are ready only the newest is shown. Before this, a decoded frame waited for the next packet:
  on the emulator with a 720p30 stream, arrival-to-render fell from 36–45 ms to 11–17 ms on
  average (median 34 → 10 ms).
- The picture is a SurfaceView (`AndroidExternalSurface`), which the system composites directly:
  one frame less delay and no extra GPU copy compared with a TextureView. It stays in place while
  the app is in the background: Android takes its surface away and gives it back on return, and
  the decoder stops and starts with it.
- The Rust tap keeps the current GOP (up to 300 frames or 16 MB). A preview that opens mid-stream
  replays it at full decoder speed and skips frames more than 100 ms behind the newest input, so
  the live picture appears in about half a second instead of after the next keyframe.
- While the receiver runs, a low-latency Wi-Fi lock keeps the radio out of power save; otherwise
  the access point would hold the remote's packets until the next beacon. Android honors it
  while the app is on screen.
- The ingest and output threads run at nice −8 and the decoder threads at `DISPLAY` and
  `URGENT_DISPLAY` priority. RTMP sockets use `TCP_NODELAY`.

The broadcast survives what a field session throws at it:

- The core fans every frame out to the platform outputs as one shared buffer. A platform that
  falls behind loses only its own frames, and ending one leaves the others running.
- If the drone drops (DJI Fly usually reconnects within seconds after a Wi-Fi hiccup), each
  platform connection stays open for up to 20 seconds. The new source's codec headers go out at
  its first keyframe, and the platform's timeline continues across the gap.
- DJI Fly sends `releaseStream` before `publish`. The receiver lets that take over the `/drone`
  route from a stale connection and closes the old one, instead of refusing the reconnect until
  the old one times out. Any device on the network can do the same, which is another reason to
  use a trusted network.
- A slow uplink no longer grows the delay or drops the connection. Unsent output is measured
  including the kernel's TCP queue (`TIOCOUTQ`). Beyond about 1.5 seconds of stream, video skips
  to the next keyframe while audio goes on, like OBS's frame dropping.
- `librtmp2`'s client used to report a full socket during `poll(0)` as a timeout, which turned
  every burst on a slow uplink into a reconnect (15 in 40 seconds on a 1.5 Mbps test link). That
  is fixed. A platform that takes no data for 10 seconds is reconnected. A backlog in the
  internal queue skips to the next keyframe instead of reconnecting.

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
lifetime to `MainActivity`. While live, the service holds a six-hour partial wake lock so the CPU
can continue handling the stream after screen lock; ending the broadcast and every stop/error/timeout
path release it. The receiver alone holds none, only the Wi-Fi lock described above. On Android 15 and newer, the platform limits `dataSync` foreground services to a total of six
background hours per 24-hour period; the service handles that timeout by stopping the native relay
and releasing its sockets. Battery/vendor settings can still impose additional device-specific
restrictions.

When the destination connection drops, the relay retries with bounded exponential backoff
(1, 2, 4, 8, then 15 seconds). It caches only public codec/metadata headers, waits for the next
video keyframe after reconnect, rebases timestamps for the new RTMP session and exposes reconnect
and dropped-packet counters in the UI. A source restart is no longer required after a temporary
target or Wi-Fi interruption. The same cache lets a broadcast start while the drone is already
streaming: the platform gets the codec headers and metadata first, then the stream from the next
keyframe.

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
