# DJI Live Bridge for Android

Native Android application for receiving an RTMP publish from DJI Fly on a DJI RC 2 and
forwarding the stream to one or more RTMP/RTMPS destinations.

The Android app does not use DJI SDK, Tauri, or a WebView. Phase 2 contains the first live
vertical slice:

```text
DJI RC 2 -> rtmp://<android-ip>:1935/live/<random-key> -> native Rust RTMP core
```

The screen shows connection state, source address, received bytes, bitrate, codecs and packet
counts. Forwarding to an external RTMP destination is intentionally reserved for phase 3.

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
