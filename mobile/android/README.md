# DJI Live Bridge for Android

Native Android application for receiving an RTMP publish from DJI Fly on a DJI RC 2 and
forwarding the stream to one external RTMP destination.

The Android app does not use DJI SDK, Tauri, or a WebView. Phase 7 contains a secure,
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

TikTok, YouTube and custom RTMP destinations can be saved as profiles. Profile metadata plus the
encrypted credential blob are written atomically to the app's non-backed-up private directory. Each
stream key is encrypted with AES-256-GCM using a non-exportable Android Keystore key and a fresh,
random IV; the profile ID is authenticated as additional data. Plaintext keys are never written to
storage, logs, notifications or intent extras. The activity passes only the selected profile ID to
the service, which decrypts the key immediately before starting the native relay. Editing a profile
without entering a new key preserves the existing encrypted credential.

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

## Phase 7 device validation

The debug build was exercised on a Samsung SM-S911B running Android 16 (API 36). The
single-segment `/drone` ingest accepted H.264/AAC over both an ADB USB tunnel and direct Wi-Fi,
reported live bitrate plus codec/packet counters, survived activity backgrounding, and retained
its foreground service, partial wake lock and listener while the device entered doze. The profile
was reloaded after a process stop; its metadata remained readable while the plaintext test key was
absent from the private profile file. Graceful stop released the service, wake lock and port 1935.

The physical-device pass also corrected Android 16 safe-area handling and made the profile editor
scrollable/compact while the keyboard is visible. A longer RC 2 stream against a real external
destination remains the final field test before release packaging.

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
