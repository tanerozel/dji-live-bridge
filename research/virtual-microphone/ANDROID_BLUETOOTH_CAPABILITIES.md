# Android Bluetooth capability and access analysis

Checked 2026-10-24. AOSP is a reference implementation; Samsung firmware may differ. Device observations must be read separately from the source conclusions below.

## Source versions

The inspected `android16-release` Bluetooth branch resolved to commit `4b73ee6039271ffbf71ebdc8c109fc98eac8e137`. The framework HF class had blob `fea8f81c5f5dbc6b8398b063bece9d56aeb8fdd4`. Some service files were available on AOSP `main` or individual pinned changes; those are explicitly identified rather than represented as Samsung source. [Android 16 branch](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release), [HF framework source](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release/framework/java/android/bluetooth/BluetoothHeadsetClient.java).

An earlier Android 11 HF class was also hidden; a Java `public` declaration or older comment describing a public API does not make a class part of the public Android SDK. No Android release is claimed to expose this class as an ordinary public SDK API. [Android 11 HF source](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/android11-mainline-release/core/java/android/bluetooth/BluetoothHeadsetClient.java).

## Profile roles

| Profile / local role | Android surface | What a normal application can conclude |
| --- | --- | --- |
| HFP Audio Gateway | `BluetoothHeadset`, public `HEADSET=1`; system `hfp/HeadsetService` | Existing phone-to-headset state can be observed; this is the opposite local role from a microphone accessory |
| HFP Hands-Free | `BluetoothHeadsetClient`, `HEADSET_CLIENT=16` | `@hide` / `@SystemApi`; product activation and audio routing are platform concerns |
| HSP Audio Gateway | System headset service / legacy headset compatibility | No public role-switching API making the phone a headset |
| HSP Headset | No public application profile implementation | Must not infer support from generic HSP/HEADSET terminology |
| A2DP Source | Public `BluetoothA2dp`, `A2DP=2` | Sends playback to an existing sink; not a microphone input profile |
| A2DP Sink | `BluetoothA2dpSink`, `A2DP_SINK=11`, system API | Not a public role and does not establish HFP microphone transport |

The profile constants explicitly distinguish gateway/headset from HF client and A2DP sink. [AOSP profile definitions](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/main/framework/java/android/bluetooth/BluetoothProfile.java). The historical AOSP service table lists HFP AG/HSP but cannot establish current Samsung firmware roles or codec versions. [AOSP Bluetooth services](https://source.android.com/docs/core/connect/bluetooth/services).

## Framework and Binder boundaries

| API / component | Public SDK? / availability | Permission and actual boundary |
| --- | --- | --- |
| `BluetoothAdapter.getProfileProxy(context, listener, profile)` | Public since API 11 | Framework can construct HF proxy for numeric ID 16. A `true` return is request acceptance; service callback is separate. It does not activate an unsupported profile |
| `BluetoothAdapter.getSupportedProfiles()` | Hidden `@SystemApi` in inspected source | `BLUETOOTH_CONNECT` + `BLUETOOTH_PRIVILEGED`; not an ordinary app capability query |
| `BluetoothHeadsetClient.connect` / `disconnect` | Hidden; legacy compatibility annotation | Connect permission in inspected source; no claim that every HF method needs privileged permission |
| HF `connectAudio` / `disconnectAudio` / audio-state / route controls | Hidden | Connect permission in inspected source; depends on a live service, connection state, stack and route |
| HF connected-device/connection-state queries, connection-policy read/write, current AG event query | System/hidden surface | Service also explicitly checks privileged permission for these operations |
| `HeadsetClientService` | Bluetooth system process, not SDK | Service checks availability, system/active/managed user and data-delivery connect permission; selected methods add privileged enforcement |

The Android 16 adapter constructor table contains ID 16. `getProfileProxy` can return before a Binder exists, so a bounded diagnostic must distinguish request rejected, permission/error, callback received and no callback by timeout. [Android 16 adapter source](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release/framework/java/android/bluetooth/BluetoothAdapter.java).

The HF class is hidden/system API; its hidden audio methods do not accept application PCM. [HF framework source](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release/framework/java/android/bluetooth/BluetoothHeadsetClient.java). The method-specific Binder gates are in [AOSP HF service](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/main/android/app/src/com/android/bluetooth/hfpclient/HeadsetClientService.java).

These system-profile boundaries do not prohibit every application-level HFP control exchange. A public RFCOMM socket can carry AT commands to an accessible remote AG service without going through `BluetoothHeadsetClient`. The real S23 test below demonstrates that limited path; it does not provide the system profile's audio transport or a complete HF accessory implementation.

`BLUETOOTH_PRIVILEGED` is `signature|privileged`, not a user-grantable nearby-device permission. Its manifest declaration alone cannot grant it; privileged integration requires a suitable system-image application and permission allowlist, or a qualifying signature grant. ADB runtime permission grants do not convert an ordinary APK into that application. [Android 16 framework manifest](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/android16-release/core/res/AndroidManifest.xml), [SDK permission reference](https://developer.android.com/reference/android/Manifest.permission#BLUETOOTH_PRIVILEGED), [AOSP privileged permission allowlists](https://source.android.com/docs/core/permissions/perms-allowlist).

## Discovery, SDP and sockets

Public APIs support paired-device enumeration, user-mediated pairing and remote UUID discovery. `getUuids()` is cached; `fetchUuidsWithSdp()` starts an asynchronous attempt; `ACTION_UUID` supplies its result. Failure, a timeout, an empty result and absence of the AG UUID must remain distinguishable. A UUID list is not the full SDP record: it does not prove RFCOMM channel, feature attributes, HFP version, SLC or audio. [BluetoothDevice API](https://developer.android.com/reference/android/bluetooth/BluetoothDevice#fetchUuidsWithSdp()).

`createRfcommSocketToServiceRecord(111f)` is a public client socket path to the remote HFP AG service. On the tested S23/RC 2 pair, it connected and carried the mandatory HFP control exchange under the target app UID with `BLUETOOTH_PRIVILEGED` ungranted. This establishes usable RFCOMM control on that pair, independently of an AOSP HF-client proxy. It does not publish local HF SDP, activate a system HF profile, expose SCO PCM or establish audio. See the [recorded control result](evidence/rc2-qcom-btd-control-2026-10-24.json).

The SDK's `BluetoothSocket.TYPE_SCO` constant is public (API 23), but there is no public SCO socket factory in this surface. Android 16 `BluetoothDevice.createScoSocket()` is `@hide` / `@UnsupportedAppUsage`. Public RFCOMM sockets carry RFCOMM data, not synchronous audio. [Socket API](https://developer.android.com/reference/android/bluetooth/BluetoothSocket), [Android 16 device source](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release/framework/java/android/bluetooth/BluetoothDevice.java).

**Android 16 socket-settings check:** API 36 adds `BluetoothSocketSettings.Builder` and `createUsingSocketSettings`. Its generated parameter enum mentions SCO, but the method prose limits valid types to RFCOMM/LE. The actual Android 16 setter throws `IllegalArgumentException` for SCO; this is also an explicit CTS expectation. Thus the newer builder is not a public SCO workaround. A safe device diagnostic can call the builder only and report the observed exception without creating a connection. [API 36 builder reference](https://developer.android.com/reference/android/bluetooth/BluetoothSocketSettings.Builder#setSocketType(int)), [Android 16 builder implementation](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/android16-release/framework/java/android/bluetooth/BluetoothSocketSettings.java), [AOSP socket-settings CTS](https://android.googlesource.com/platform/cts/+/refs/heads/main/tests/tests/bluetooth/src/android/bluetooth/cts/BluetoothSocketSettingsTest.java).

`listenUsingRfcommWithServiceRecord` does not provide an application HFP stack with its required full SDP attributes, connection policy and PCM transport. Advertising a standard UUID by itself cannot establish profile compliance or satisfy DJI Fly's microphone check. It is deliberately absent from the product implementation.

## Native, controller and audio path

Android NDK/JNI code runs under the application's UID and SELinux domain. AOSP Android 16 policy prohibits untrusted applications from using the `bluetooth_socket` class and from setting system properties. Native code cannot turn a Linux `socket(AF_BLUETOOTH, ..., BTPROTO_HCI/SCO)` example into authorized stock-Android controller access. The Samsung kernel/HAL might differ, but no permitted raw endpoint has been demonstrated. [Android 16 application neverallow policy](https://android.googlesource.com/platform/system/sepolicy/+/android16-release/private/app_neverallows.te).

The NDK exposes audio capture/playback via AAudio/Oboe and IPC via Binder, not a stable HFP accessory/controller API. Calling an internal Binder or copying an AOSP JNI wrapper still requires its corresponding service and permission checks. [NDK native APIs](https://developer.android.com/ndk/guides/stable_apis).

The requested `HeadsetClientNativeInterface.java` name is not the inspected AOSP name: `hfpclient/NativeInterface.java` wraps system-stack native connect/audio/control functions. It depends on `AdapterService`. Copying it into the APK does not install a Bluetooth service or inherit its native stack state. [Pinned native-interface source](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/a5ba99a1c93b7b0fac7dfb4723e5508b545b8f00/android/app/src/com/android/bluetooth/hfpclient/NativeInterface.java).

HF state management calls platform HFP audio controls and applies 8 kHz/16 kHz configuration for narrow/wideband speech. This is system audio routing, separate from `AudioRecord` samples. The precise Samsung uplink/offload implementation requires device/vendor evidence. [Pinned HF state machine](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/43ebbf5565851af3e0de28b2089af7c98ad07c65/android/app/src/com/android/bluetooth/hfpclient/HeadsetClientStateMachine.java).

Public `startBluetoothSco()` (API 8, deprecated 34) requests audio to/from an existing Bluetooth SCO headset. `setCommunicationDevice()` (API 31) chooses an already available communication output and its matching source. Neither API installs the phone's HF role nor attaches arbitrary captured PCM to an AG peer. A SCO route in the gateway role is not the requested HF uplink. [AudioManager reference](https://developer.android.com/reference/android/media/AudioManager.html).

## Properties and profile activation

| Property / check | Interpretation |
| --- | --- |
| `bluetooth.profile.hfp.hf.enabled` | AOSP vendor/product configuration for HF role; empty device output is `UNKNOWN`, not `false` |
| `bluetooth.profile.hfp.ag.enabled` | AG activation; `true` does not imply HF activation |
| `bluetooth.hfp.software_datapath.enabled` | Stack audio datapath selection; not an application SCO entitlement and not a synonym for HF |
| `dumpsys bluetooth_manager` profile list | Runtime configuration evidence; inspect HF/client names, not just generic `HeadsetService` |
| Proxy ID 16 callback | Evidence of a service proxy only; no SLC/SCO/microphone proof |
| `getenforce` | SELinux operating mode; an Enforcing result does not enumerate every Samsung rule |

These properties are vendor-set and read during Bluetooth initialization. [Pinned property definitions](https://android.googlesource.com/platform/system/libsysprop/+/eaff33a31e57a82072b3a42da4c349a69a36def9/srcs/android/sysprop/BluetoothProperties.sysprop). The inspected AOSP HF service chooses `orElse(false)` for its own optional property; that describes that implementation's fallback, not a universal interpretation of empty Samsung output. [HF service](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/main/android/app/src/com/android/bluetooth/hfpclient/HeadsetClientService.java).

## Permissions and background capture

The diagnostic only needs permissions for operations it actually performs. Paired-device state/SDP uses `BLUETOOTH_CONNECT` on Android 12+; active scanning would additionally require `BLUETOOTH_SCAN`; making the phone discoverable would require the appropriate advertise flow. These are not HFP activation permissions. [Bluetooth permission guidance](https://developer.android.com/develop/connectivity/bluetooth/bt-permissions).

Do not add `RECORD_AUDIO`, microphone foreground-service permissions or a capture service just to show an unsupported transmitter. If transport becomes viable, request microphone permission from an explicit visible user action; declare the microphone FGS type/permission, start within while-in-use restrictions, keep the notification and stop capture on disconnect/stop. Background-start exceptions must be assessed for the real target version. [Foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types#microphone).

## Real S23 observation

Two sanitized USB/ADB snapshots were collected on 2026-10-24 local time. Both record `SM-S911B`, Android 16 / SDK 36, `Enforcing`, AG property `true`, and unknown HF/software-datapath properties:

- [Initial Bluetooth-off report](evidence/galaxy-s23-2026-10-24.json): only `GattService` appeared. This snapshot cannot establish the enabled profile inventory with Bluetooth on.
- [Later Bluetooth-on report](evidence/galaxy-s23-bluetooth-on-2026-10-24.json): 20 profile headings appeared, including `HeadsetService` and `A2dpService`; no `HeadsetClientService` mention appeared. This supports absence of that AOSP-named HF service from the active dump at collection time. It does not prove silicon incapability, exclude all Samsung-specific service implementations, or substitute for a profile-16 proxy/access experiment. The agent did not toggle Bluetooth and no proxy experiment was performed.

Package-dump absence of a permission string is not a permission-grant measurement; the app uses `checkSelfPermission`. RC 2 was not inspected by either collection. An actual Turkish diagnostic UI hierarchy subsequently showed developer diagnostics on, Bluetooth on and the paired-device list; this establishes UI/state inspection only, not HFP/SCO or microphone transmission. Build/lint/unit-test outcomes are recorded in [VERIFICATION.md](VERIFICATION.md).

The subsequent 2026-10-24 local-time RC 2 experiment is separate from those snapshots. After the user identified paired `QCOM-BTD` as the controller, the [UI SDP query](evidence/rc2-qcom-btd-sdp-2026-10-24.json) received `111f` and `1112` (with possible cached data and no broadcast request ID). The [test-only public RFCOMM probe](evidence/rc2-qcom-btd-control-2026-10-24.json) then ran under the ordinary target application UID, confirmed `BLUETOOTH_PRIVILEGED` was not granted, and observed:

- RFCOMM connection to remote service `111f`.
- `AT+BRSF=0` answered with a valid AG feature value `3943` and `OK`.
- Seven required `CIND` definitions with valid ranges, matching valid current values, and their `OK` responses.
- `AT+CMER=3,0,0,1` answered `OK`, yielding `MANDATORY_SLC_EXCHANGE_OBSERVED`; the socket was closed and worker stopped.

The peer identity is user-confirmed; selection required exactly one paired device with that name. The probe advertised no optional HF features, so it did not perform optional codec/three-way/HF-indicator branches. This is real, bounded control-plane evidence, not a full SDP dump or an ongoing profile lifecycle test. It neither used nor demonstrated `HeadsetClientService`, local HF SDP, SCO/eSCO, codec negotiation, microphone capture, a system audio route, DJI Fly microphone recognition or RTMP sound. No profile-16 probe or Linux hardware test is established by it.

No codec, latency, battery, CPU, screen-lock continuity or audio quality result is established by this document.
