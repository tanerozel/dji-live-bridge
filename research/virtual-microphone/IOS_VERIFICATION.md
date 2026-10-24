# iOS availability diagnostics verification

Date: 2026-10-24. Xcode 27.0 (27A266a), iOS 27.0 simulator, iPhone 18 Pro.

## Implemented scope

The iOS **More → Virtual Bluetooth Microphone** sheet reports the current app's unavailable
HFP control transport separately from unverified DJI Fly microphone recognition. An explicit
button creates a Core Bluetooth manager and reads its real radio/authorization state, with
a five-second bound on an indeterminate initial result and cleanup when dismissed. A denied
permission links to the app's supported Settings URL. No manager is created at launch or merely
by opening the sheet.

The copy action contains only fixed diagnostic codes and the numeric iOS version. It excludes
device names, Bluetooth addresses, UUIDs, stream keys, exception messages and audio. All 15 app
languages include the iOS copy and Bluetooth privacy description through the existing generator.

There is no iOS RFCOMM/HFP connection implementation, paired-controller selector, scanner,
microphone capture or audio-session change. This is a diagnostics implementation, not a
successful port of Android's RC 2 recognition feature. See [the API analysis](IOS_FEASIBILITY.md).

## Automated checks

- `xcodebuild test` on the existing iPhone 18 Pro / iOS 27.0 simulator: **93 passed, 0 failed, 0 skipped**.
  This includes nine new availability/report tests covering initial state, authorization
  precedence, distinct radio states, timeout/late callback, refresh and report privacy.
- Xcode reported a runtime QoS warning in the existing `TestVideoConverterTests.swift`; there
  were no test failures. The new Bluetooth state tests do not initialize a Bluetooth manager.
- String importer: all 45 generated files across 15 languages match their sources.
- Desktop regression checks: Rust formatting and clippy passed; **79 tests passed, 6 ignored**;
  TypeScript typecheck and ESLint passed. These do not establish iOS Bluetooth compatibility.

Local test artifact: `/tmp/smd-ios-vmic-tests-20261024.xcresult`. This temporary path is not a
release artifact and is not committed.

## Simulator UI observation

Opened the newly built app in Device Hub's iPhone 18 Pro simulator and navigated through
**More → Virtual Bluetooth Microphone**. The initial screen showed Bluetooth **not checked**,
HFP control **unavailable in this iOS app**, and DJI Fly recognition **unverified**. No Bluetooth
prompt appeared when opening the app or sheet. After the explicit check, Core Bluetooth returned
**unsupported** in this simulator; this is not a physical iPhone capability result. Expanding
diagnostics displayed those same separate states and `audio_transmission=not_started`. The
explicit copy button changed to **Report copied**. The sheet's explanatory cards and controls
were visible without clipped text in the default English display.

## Hardware boundary

Device inventory contained simulators only; no physical iPhone was connected. No iPhone–RC 2
HFP handshake, sustained control connection, DJI Fly warning reversal or microphone recognition
was performed. Simulator state/UI checks cannot establish those results. Before claiming the
requested feature works, a documented permitted transport is still needed, followed by the
physical acceptance procedure in [IOS_FEASIBILITY.md](IOS_FEASIBILITY.md).
