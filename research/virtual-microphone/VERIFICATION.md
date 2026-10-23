# Verification and hardware acceptance

Date: 2026-10-24 (Europe/Istanbul). This file distinguishes source/build/diagnostic verification from the requested Bluetooth audio acceptance.

## Actual Galaxy S23 observations

The read-only [sanitized evidence](evidence/galaxy-s23-2026-10-24.json) was collected from the explicitly selected, authorized physical Samsung SM-S911B:

- Android 16, API 36; SELinux Enforcing.
- `bluetooth.profile.hfp.ag.enabled=true`.
- `bluetooth.profile.hfp.hf.enabled` and `bluetooth.hfp.software_datapath.enabled` were empty: unknown, not false.
- Bluetooth was OFF at collection time. Only `GattService` had a profile dump heading; absence of `HeadsetClientService` in this state does not establish its provisioning or hardware support.
- During verification Bluetooth became ON. The separate [Bluetooth-ON snapshot](evidence/galaxy-s23-bluetooth-on-2026-10-24.json) contains 20 profile headings, including `HeadsetService` and `A2dpService`, but no `HeadsetClientService` mention. This supports the absence of an active exposed HF-client profile in that dump; it does not prove a hardware limitation or enumerate every Samsung private service. No system Bluetooth setting was changed by the collector.
- `cmd package check-permission` is unsupported on this firmware (`Unknown command: check-permission`, exit 255). No permission conclusion is drawn from it. The app uses the supported `checkSelfPermission` API.
- Those initial shell snapshots did not measure controller chipset/firmware, Qualcomm vendor HAL behavior, RC 2 services, HCI, RFCOMM, SCO or microphone routing. Subsequent RC 2 service/control observations are recorded separately below.

## Follow-up: user-identified QCOM-BTD / RC 2

The user identified QCOM-BTD as the RC 2. The installed S23 diagnostic UI confirmed the selected target, and an explicitly repeated service query returned `RECEIVED`, with both HFP AG `111F` and HSP AG `1112` in its cached list and response. `BLUETOOTH_PRIVILEGED` was false. The later report snapshot was scrolled past the target row; the preceding selected-row observation is recorded separately. See [sanitized SDP evidence](evidence/rc2-qcom-btd-sdp-2026-10-24.json). Android broadcasts lack a query ID, so this is not a guarantee of a newly retrieved full SDP record.

An isolated test APK then established public RFCOMM to AG `111F` and observed valid BRSF/CIND/CMER responses for the mandatory HF-features-zero initialization branch. The recorded result is `MANDATORY_SLC_EXCHANGE_OBSERVED`, `passed=true`, under the ordinary application UID with privileged permission ungranted. Remote feature mask `3943` and seven valid indicator definitions were observed. The complete probe took 1733 ms, not an audio-latency measurement. [Control evidence and reproduction](ANDROID_CONTROL_PROBE.md).

This **passes the isolated mandatory control-exchange test**. It does not demonstrate local HF advertising, a production system/profile session, sustained interoperability, SCO/eSCO, codec negotiation, microphone routing, Fly microphone acceptance or RTMP speech. The probe closed its socket after the experiment. The temporary test package was removed and the normal application reopened. No root, firmware, Bluetooth identity or system audio configuration was changed.

## Hardware acceptance status

Complete product acceptance remains pending. The isolated mandatory control exchange above passed; a diagnostic screen, successful build, UUID, connected RFCOMM socket or Linux graph alone does not satisfy the complete microphone criteria.

- [ ] Galaxy S23 is discoverable as HFP-HF — blocked/unverified for a stock third-party APK.
- [ ] DJI RC 2 successfully pairs with this HF implementation — not tested.
- [ ] Product HFP service level connection established — mandatory HF-zero control exchange observed over public RFCOMM; accessory registration and production session integration remain unverified.
- [ ] SCO/eSCO audio channel established — not tested.
- [ ] Phone microphone routed to Bluetooth — blocked/unverified.
- [ ] DJI Fly recognizes microphone — not tested.
- [ ] RTMP livestream starts using the phone as its microphone — not tested.
- [ ] Phone microphone is audible in livestream — not tested.
- [ ] No additional microphone required — not demonstrated.
- [ ] Works on stock Android without root — not demonstrated; no supported product implementation identified.

## Reproducible hardware procedure

1. Record S23 model/Android/One UI and Bluetooth firmware when available; record RC 2 firmware, DJI Fly version, drone model/firmware and Linux BlueZ/PipeWire/WirePlumber versions. Keep identifying addresses and raw captures private.
2. Establish a baseline with the user's known-working Bluetooth headset, if available solely as a reference test. Capture which RC 2 service UUIDs, SLC/AT exchanges, synchronous link events and codec are actually present. Do not infer HFP from successful pairing or A2DP playback. Without this baseline, the AirPods report remains user-reported.
3. Run the Android diagnostic screen with permission denied, then granted; test Bluetooth OFF, ON, return from settings, no paired peers, peer selection, empty/cached UUIDs, query completion/rejection/timeout, cancellation, backgrounding and adapter disable. Only report states actually observed. Check that no microphone privacy indicator or capture starts.
4. With the RC 2 already paired in Android settings, select it and query UUIDs. A fresh gateway UUID is preliminary evidence only; absent UUIDs are inconclusive. Do not proceed to an Android audio success claim.
5. On a Linux host, follow [linux/README.md](linux/README.md). Verify local HF role, remote AG record, actual SLC, codec negotiation and SCO/eSCO separately. If the AG does not open an audio transport, retain that failure and stop; do not replace it with RFCOMM PCM or a null sink.
6. Route the actual Linux microphone to the verified outbound HF sink, speak a unique timed phrase, and verify it at an RTMP receiver. Record a private short consented receiver sample and synchronized HCI/graph evidence. A changing volume meter alone is insufficient. Linux success does not satisfy stock S23 success.
7. Only after an OEM/system HF path with a real microphone route exists on the S23, repeat steps 5–6 there. Verify DJI Fly starts RTMP with no accessory microphone, and that the received speech came from the S23 by mute/unmute and physical speech-source isolation.
8. Test screen lock for at least 10 minutes; an incoming/outgoing call; another app opening the microphone; Bluetooth disable/re-enable; leaving range/returning; RC 2 restart; app foreground/background/exit. Record whether Fly pauses, stops or continues and whether audio/capture is released promptly. These tests require a functioning audio implementation and remain pending.
9. Measure end-to-end latency with a common visual/acoustic reference and receiver recording, report repeated samples and receiver buffering. Record actual dropped frames, PCM underruns, reconnection times, process CPU and battery consumption under a fixed test duration. No latency, stability, CPU or power number has been measured for this feature.

## Automated checks

Unit tests cover only the implemented diagnostic state/SDP handling and lab tooling; there is no implemented Android HFP AT parser, codec pipeline or SCO audio buffer to test.

| Check actually executed | Result |
| --- | --- |
| Android `./gradlew testDebugUnitTest lintDebug assembleDebug`, then final-source test/lint rerun | 137 tests passed, zero failures/errors/skips; debug APK built; lint 0 errors / 8 warnings in existing Gradle/resource declarations |
| `env DJI_ANDROID_DEVICE_SERIAL=<explicit-device> ./start.sh` | Final APK rebuilt; S23 install `Success`; launch `Status: ok`, cold launch; nonempty application PID confirmed |
| Android diagnostic UI on S23 | Turkish unsupported explanation and developer mode observed; Bluetooth ON, paired-device list visible. No selected RC 2/SDP query or outgoing microphone tested |
| Follow-up RC 2 SDP UI query | Selected QCOM-BTD confirmed; repeated query `RECEIVED` with HFP/HSP AG UUIDs; no query-ID/full-record guarantee |
| Follow-up Android test APK | `assembleDebugAndroidTest` built, 137 unit tests still passed, lint 0 errors / 8 warnings; parser-only self-check passed on S23 |
| Follow-up actual RFCOMM/HFP mandatory exchange | Passed under ordinary application UID with privileged permission ungranted; structured control evidence saved; audio and RTMP verdicts remain `NOT_TESTED` |
| `python3 -m unittest discover -s research/virtual-microphone/android -v` | 3 tests passed: unknown properties, privacy allowlist, service-mention evidence boundary |
| `python3 -m unittest discover -s research/virtual-microphone/linux -v` | 12 offline tests passed; graph direction/role/peer/codec, disconnect cleanup and evidence safety only |
| Linux harness host preflight | Actual Darwin `UNSUPPORTED_HOST`, exit 2; `NOT_TESTED` hardware verdict |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | Passed |
| `cargo clippy --locked --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` | Passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 79 passed, 6 ignored. Initial sandbox run failed the existing loopback-listener test; the same suite passed with authorized local TCP access |
| `npm run typecheck && npm run lint` | Passed |
| `git diff --check` | Passed |

Android commands used Android Studio's Java runtime and the existing Android SDK. No macOS application bundle was built. The 6 ignored Rust tests need separate model/hardware fixtures and were not claimed as passed. Runtime permission denial, local HF accessory discovery/registration, physical SCO/audio, screen-lock capture, telephony arbitration and reconnect hardware scenarios remain unexecuted. Existing pairing was observed; a fresh HF-accessory pairing flow was not exercised.

An optional emulator install did not complete and its owned ADB client was terminated. No emulator UI success is claimed. A later physical-device screenshot showed the screen asleep; the S23 diagnostic-screen observation above is based on its earlier UI hierarchy, not a visual screenshot acceptance test.
