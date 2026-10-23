# Virtual microphone blockers and system integration boundary

Status date: 2026-10-24. This is an implementation decision record, not a request to modify the phone.

## Current blockers

| ID | Blocker | Evidence needed to close it |
| --- | --- | --- |
| B1 | Full local HF SDP advertisement, profile compliance and lifecycle are not implemented/verified; public RFCOMM control alone has passed | A permitted full-profile mechanism and interoperability/lifecycle evidence; OEM integration if the platform requires it |
| B2 | The Bluetooth-on S23 dump contains gateway/A2DP services but no `HeadsetClientService` mention; no usable system HF profile established | OEM profile/implementation evidence or a real profile-16 callback, followed by service permission and connection evidence; this is separate from the working public RFCOMM control probe |
| B3 | No application PCM-to-HF SCO uplink established | System audio/HAL route or supported bounded PCM endpoint; signal correlation at the receiving AG |
| B4 | RC 2 AG UUIDs and mandatory SLC exchange observed; full SDP attributes, codec and microphone behavior remain unmeasured | Full SDP attributes, baseline known-headset RFCOMM/HCI and SCO timeline with firmware/DJI Fly versions |
| B5 | Stock-device end-to-end acceptance incomplete | S23 microphone audible in outgoing RTMP with no added microphone; lock/reconnect/loss tests |
| B6 | Linux hardware proof requires a Linux Bluetooth audio host and RC 2 | Execute the Linux harness/procedure; preserve real logs independently of offline fixture results |

The framework, Binder, SELinux, property and audio-route evidence underlying B1–B3 is linked in [ANDROID_BLUETOOTH_CAPABILITIES.md](ANDROID_BLUETOOTH_CAPABILITIES.md). The [first S23 report](evidence/galaxy-s23-2026-10-24.json) was collected with Bluetooth off. The [later Bluetooth-on report](evidence/galaxy-s23-bluetooth-on-2026-10-24.json) contains 20 profile headings, including `HeadsetService` and `A2dpService`, but no `HeadsetClientService` mention. This narrows B2 to the observed active dump; it neither closes B2 nor proves the controller lacks HF hardware capability. No profile-16 proxy/access experiment was performed.

The later [ordinary-app RC 2 control test](evidence/rc2-qcom-btd-control-2026-10-24.json) connects to user-identified `QCOM-BTD` through public RFCOMM service `111f` with `BLUETOOTH_PRIVILEGED=0`. It validates `BRSF`, seven `CIND` definitions/current values and `CMER`, returning `MANDATORY_SLC_EXCHANGE_OBSERVED` before cleanup. Therefore lack of all application-level HFP control is not a blocker. No local HF SDP, SCO/eSCO, codec negotiation, microphone route or RTMP audio was tested; B1–B6 remain open at their stated scope.

## Minimum concrete system work if an OEM chooses to support this

The following is a proposed OEM/system-profile integration map, **not a validated Samsung patch or a prerequisite for the already demonstrated public-RFCOMM control exchange**. Samsung source and exact vendor audio/HCI topology are not available here. Setting one property is insufficient. Do not ship these changes in the normal app or apply them to the user's phone.

1. **Activate an implemented HF profile in the product image.** Product/vendor configuration must enable `bluetooth.profile.hfp.hf.enabled=true` before Bluetooth initialization, include the HF service and its native support, and verify how `android/app/src/com/android/bluetooth/btservice/Config.java` includes `HeadsetClientService.isEnabled()` / `HEADSET_CLIENT`. Review `android/app/src/com/android/bluetooth/hfpclient/HeadsetClientService.java` and preserve its service lifecycle and permission checks. Older products may use resource overlays instead of the newer property path. The exact S23 activation mechanism must be found in Samsung's image; it must not be guessed from AOSP. [AOSP profile configuration change](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/dff8f67c3454040d38c8b06218825b9a692d4984%5E2..dff8f67c3454040d38c8b06218825b9a692d4984/).

2. **Provide authorized profile control.** Prefer a small OEM system broker with a signature-bound interface for this application's approved operations. If a client itself needs privileged HF APIs, install it in an appropriate `priv-app` partition, compile against the system SDK, declare only needed permissions and grant `BLUETOOTH_PRIVILEGED` in the same partition's `etc/permissions/privapp-permissions-*.xml` as required. A platform signature is another qualifying path for signature permissions; normal APK signing is not. Do not make the Bluetooth service broadly exported or remove its gates. [AOSP allowlist rules](https://source.android.com/docs/core/permissions/perms-allowlist).

3. **Build the microphone uplink, separately from profile control.** Inspect `hfpclient/HeadsetClientStateMachine.java`, `hfpclient/NativeInterface.java`, its JNI implementation and the stack's `system/bta/hf_client/` and `system/btif/src/btif_hf_client.cc`. Then inspect the product's `vendor/etc/audio_policy_configuration.xml`, its included routes, audio HAL HFP/SCO handling, Bluetooth audio HAL and controller PCM/HCI transport. Determine whether a system HF route can send the built-in microphone directly, or whether a new broker-managed PCM endpoint is required. Internal software-datapath code under the AG path must not be assumed to provide an HF uplink. [AOSP AG SCO implementation](https://android.googlesource.com/platform/packages/modules/Bluetooth/+/refs/heads/main/system/bta/ag/bta_ag_sco.cc).

4. **Keep controller ownership and security bounded.** Retain Android's Bluetooth service as controller owner. A normal app must not open raw HCI or run a competing BlueZ controller stack. If a new native system daemon is genuinely needed, give it a dedicated SELinux domain, exact Binder/hardware/audio access, lifecycle and permission checks; do not set SELinux permissive or grant untrusted apps raw Bluetooth access. Raw controller privileges would expand the attack surface and still not supply the audio route automatically. [Android 16 untrusted-app policy](https://android.googlesource.com/platform/system/sepolicy/+/android16-release/private/app_neverallows.te).

5. **Verify actual audio and arbitration.** Validate CVSD/mSBC configuration against the negotiated codec, release the route on stop/disconnect, handle telephony and microphone privacy switches, and test concurrent relay use, device lock, app termination and reconnect. Measure latency, loss, CPU and battery on the exact S23 firmware. A broker service or custom ROM that pairs is not complete until the RC 2 recording and RTMP result contain the microphone signal.

Installing a broker/privileged app, changing product properties or audio policy, signing a system module and enabling a vendor route require OEM/system-image control. Bootloader/root-based experiments are separate from the requested stock/no-root consumer APK and are not performed by this work.

## Native module decision

No C++ Bluetooth engine, JNI wrapper or CMake tree is added merely to fill the requested folder names. Public RFCOMM already permits the bounded Kotlin control probe; there is no demonstrated authorized SCO/PCM endpoint for a native audio engine to own. Reuse existing system implementations first. If a future integration exposes an actual PCM endpoint, these interfaces can become concrete:

| Module | Input → output | Required authority | Verification |
| --- | --- | --- | --- |
| HCI/L2CAP/SDP | Profile commands ↔ controller events | System Bluetooth service; not app UID | HCI capture plus full SDP record |
| RFCOMM/HFP control | Features/indicators ↔ mandatory SLC exchange | Public RFCOMM client demonstrated on S23/RC 2; system HF service or Linux BlueZ for fuller profile integration | Validated AT responses and cleanup observed; full profile lifecycle remains separate |
| SCO/codec/routing | Negotiated codec + PCM ↔ synchronous link | Bluetooth/audio HAL or supported Linux backend | Controller event + peer recording; RFCOMM bytes are insufficient |
| Capture/DSP | Built-in mic → mono PCM/levels/mute | User microphone permission and suitable lifecycle | Local signal, silence/mute and buffer checks; not transport proof |
| JNI bridge | Kotlin commands ↔ a real supported native endpoint | Same UID/permissions as caller | Denied access stays an error; no fabricated success |

PipeWire/BlueZ handle the Linux profile and SCO implementation; AOSP HF client is the system Android candidate. The test-only AT parser has demonstrated mandatory control exchange over public RFCOMM, but cannot close the missing SCO/audio-path blockers. Use built-in Android effects/AAudio or Oboe only when the actual capture/transport architecture requires them; measure rather than promise their latency.

## Consumer release rule

The application may expose truthful capability/connection diagnostics. It must display the requested unsupported message when its HF transmitter backend is unavailable:

`This Android device does not allow third-party apps to emulate a Bluetooth HFP microphone.`

This describes the supported application path. It must not be paired with fabricated connection/capture indicators or a claim that every OEM/private service is absent. Play publication cannot grant platform permissions or install an OEM Bluetooth/audio capability. No Play approval has been requested or obtained for this feature.
