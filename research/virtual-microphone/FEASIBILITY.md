# Virtual Bluetooth Microphone: feasibility decision

Research date: 2026-10-24. Target: an ordinary Stream My Drone APK on a stock Galaxy S23, using its built-in controller as an HFP Hands-Free microphone for DJI RC 2.

## Decision

**There is no demonstrated complete stock-S23 microphone-to-RC 2 audio path.** A real test now demonstrates the mandatory HFP service-level control exchange through a public RFCOMM socket under the ordinary app UID, without `BLUETOOTH_PRIVILEGED`. That corrects any broader claim that Android application-level HFP control is impossible. It does not establish a complete HF accessory, SCO/eSCO, a microphone route or RTMP sound. The product still provides capability/SDP diagnostics; the control probe is test-only and Linux audio research remains separate.

This is a conclusion about the application contract and evidence collected, not a claim that the S23's Bluetooth silicon cannot transmit SCO. AOSP contains an HF client implementation. Whether Samsung ships/enables it, whether a normal UID can control it, and whether its audio HAL provides the needed microphone uplink are different questions. The exact API and permission evidence is in [ANDROID_BLUETOOTH_CAPABILITIES.md](ANDROID_BLUETOOTH_CAPABILITIES.md).

In particular, hidden does not mean every underlying Binder operation demands a signature permission. Some AOSP HF control operations only check Bluetooth connect permission; other HF operations explicitly check privileged permission. Neither fact turns the hidden interface into a public third-party SDK or establishes an audio input route. Android restricts access to non-SDK interfaces, including through reflection and JNI. [Android non-SDK restrictions](https://developer.android.com/guide/app-compatibility/restrictions-non-sdk-interfaces).

## Evidence boundary

| Finding | Evidence class | Consequence |
| --- | --- | --- |
| Existing Android app is Kotlin/Compose, application ID `com.streammydrone.app`, min SDK 28, target SDK 36, arm64; existing JNI is the Rust RTMP relay | Repository `mobile/android/app/build.gradle.kts` | Keep current architecture; an unrelated native Bluetooth engine would add no missing authority |
| Public APIs can inspect adapter/paired devices and request remote UUID discovery | Android SDK | Implement useful diagnosis with bounded operations |
| Public RFCOMM to user-identified RC 2 `QCOM-BTD` completed `BRSF`, validated `CIND` definitions/values and `CMER` | [Ordinary-app device test](evidence/rc2-qcom-btd-control-2026-10-24.json) | Minimal HFP control is feasible without the system HF client; full profile and audio support do not follow |
| AOSP includes `BluetoothHeadsetClient` and a real HF service | Source analysis | A system/OEM implementation is technically plausible; app availability is not implied |
| Public `BluetoothProfile.HEADSET` represents the phone's gateway connection to a headset | Android roles/API | It does not turn the phone into the requested headset |
| A microphone meter proves local samples only | Test interpretation | It must never produce `STREAMING` or `AUDIO_CONNECTED` |
| RC 2 AG service UUIDs and mandatory control exchange | [SDP UI observation](evidence/rc2-qcom-btd-sdp-2026-10-24.json) plus public RFCOMM test | AG control behavior observed; full SDP attributes, SCO and actual outgoing RTMP audio remain unverified |
| AirPods transport and DJI Fly's exact microphone predicate | Unverified hypothesis | Need a known-headset baseline and synchronized Fly/audio observations |

The [initial sanitized USB/ADB report](evidence/galaxy-s23-2026-10-24.json) identifies Samsung `SM-S911B`, Android 16 / SDK 36, SELinux Enforcing and AG property `true`. HF and software-datapath properties returned empty and remain unknown. Bluetooth was off in that first snapshot.

The [later Bluetooth-on snapshot](evidence/galaxy-s23-bluetooth-on-2026-10-24.json) contains 20 profile headings, including `HeadsetService` and `A2dpService`, with no `HeadsetClientService` mention. This is stronger evidence that the expected AOSP-named HF service was not present in the active dump on this firmware at that time. It does not establish every Samsung implementation detail, lack of silicon capability, or the result of a profile-16 proxy probe; no such probe was performed. Neither snapshot inspected RC 2 or established an audio path. Source inspection and diagnostic UI verification are not hardware microphone acceptance tests.

On 2026-10-24 local time, the user identified paired `QCOM-BTD` as the RC 2. Its UUID response included HFP AG `0x111f` and HSP AG `0x1112`, subject to Android's cache/request-correlation limits. A separate test-only instrumentation run then connected to the `111f` service through `createRfcommSocketToServiceRecord`, sent `AT+BRSF=0`, validated AG feature value `3943`, seven required indicator definitions and their current values, and received `OK` for `AT+CMER=3,0,0,1`. It returned `MANDATORY_SLC_EXCHANGE_OBSERVED` and stopped its worker after closing the socket. No local HF SDP record, system HF proxy, codec negotiation, SCO request, microphone capture or audio transmission was part of this test.

## Delivery gates

1. **Phase 1 / diagnosis:** document public API boundaries; inspect the real phone and its SDK; show permission denial, absent service or unknown state honestly; fetch a selected paired peer's UUIDs. Diagnostics may report `UNSUPPORTED_PLATFORM` for this application's current transmitter capability while leaving OEM HF availability `UNKNOWN`.
2. **Phase 2 / Linux proof:** use the existing BlueZ/PipeWire HF implementation, record SDP, service-level control, synchronous audio establishment and actual microphone arrival. See [HFP_PROTOCOL_ANALYSIS.md](HFP_PROTOCOL_ANALYSIS.md) and the Linux harness in this directory. Running an offline parser test is not a Bluetooth proof.
3. **Phase 3 / Android transmitter:** proceed only after a permitted, reproducible end-to-end microphone uplink and the required HF profile lifecycle are established. Public RFCOMM control has passed its bounded experiment; it does not supply the missing synchronous audio route. An OEM-supported route or a separately authorized system build remains a possible integration path, not evidence already obtained.
4. **Phase 4 / product UI:** enable capture, mute, gain, disconnect and foreground microphone service only when a real transport exists. Until then show the supported diagnostic UI and the explicit unsupported explanation.
5. **Phase 5 / acceptance:** mark each hardware criterion only after time-correlated evidence. The eight RC 2 tests and final livestream recording remain necessary.

No root, bootloader, property mutation, firmware replacement, Bluetooth identity spoofing, hidden-API exemption, or persistent system modification is part of the ordinary APK implementation.

The integrated Android diagnostic module performs public adapter/permission/paired-device/UUID diagnostics. It does not request the numeric HF proxy, invoke hidden APIs, open RFCOMM/SCO sockets or capture the microphone. The separate `androidTest` control probe opens a bounded public RFCOMM socket and closes it after the mandatory AT exchange; it is not a product transmitter. Numeric proxy and API-36 socket-builder checks remain possible bounded investigations, not executed device results.

## Alternative paths

These are engineering assessments, not Google Play review outcomes.

| Path | Required environment / root | Stock S23 assessment | Play / security assessment | Meets final target? |
| --- | --- | --- | --- | --- |
| Public paired-device/SDP diagnostics | Ordinary APK; no root | Supported subject to user permission and peer response | Candidate for normal distribution; explain collected device data | Diagnosis only |
| Public RFCOMM + bounded HFP AT control | Ordinary target-app UID; no root or privileged Bluetooth grant | Mandatory SLC exchange observed with this RC 2; no local HF SDP or audio | Public socket API; test-only scope does not establish full profile compliance or Play review | Control only |
| Public HEADSET/A2DP / audio communication selection | Ordinary APK; no root | Uses roles and endpoints already provided by the OS | Public APIs; does not supply HF emulation | No |
| Existing `BluetoothHeadsetClient` through a numeric profile probe | Ordinary APK probe; no root | May report accepted/no callback, proxy, or error; enabled service and permission behavior must be measured | Useful bounded diagnostic, not a supported transmitter contract | No proof |
| Hidden HF methods via reflection/JNI | Non-SDK access; service must already exist | Version/OEM-dependent and restricted; cannot activate missing profile or create audio route | Not selected as a maintainable public SDK product path | No validated path |
| JNI + raw HCI/SCO/BlueZ in app UID | Would require permissions/kernel facilities absent from the normal sandbox | SELinux/native boundary blocks the proposed escape; controller ownership also matters | Broad hardware/stack access is a system integration concern | No |
| Samsung-documented OEM integration | An actual supported OEM interface and authorization | No suitable Samsung microphone-role interface demonstrated | Could preserve consumer no-root behavior if Samsung ships a broker; review separately | Conditional future path |
| Companion device association | Ordinary APK with user association | Helps discovery/background association; does not implement a Bluetooth profile or audio route | Public API | No |
| OEM system service + signed/privileged client | Vendor-supplied system image/update; user need not root | Requires Samsung support, profile activation and audio HAL route | Preinstalled integration; Play update alone cannot install the system capability | Conditional, not current stock APK |
| Custom ROM/system module | Unlock/root or custom signed image, depending on device | S23 vendor Bluetooth/audio support must be ported and tested | Separate experiment; security/OTA consequences | Violates stock/no-root requirements |
| Root prototype | Root plus explicit system/audio changes | Root alone is not a complete profile or PCM implementation | Separate experiment; no consumer claim | Violates requirements |
| Linux BlueZ + PipeWire | Linux HF-capable controller; adapter allowed for research | Establishes peer protocol only | Separate research host | Protocol proof only |

Companion association itself creates neither a Bluetooth connection nor a continuous scan. [Android companion device pairing](https://developer.android.com/develop/connectivity/bluetooth/companion-device-pairing).

## What would change the decision

The real public-RFCOMM exchange has already changed the control-plane conclusion. An OEM-documented audio route, or a reproducible authorized stock-S23 experiment proving the remaining profile/audio layers, would justify revisiting the blocked full-transmitter result. An AT parser, an SDP record with `0x111e`, an `AudioRecord` loop, an accessible internal class, or a successful Linux demo does not independently establish the missing Android audio transport. Concrete system integration candidates are described in [BLOCKERS.md](BLOCKERS.md).
