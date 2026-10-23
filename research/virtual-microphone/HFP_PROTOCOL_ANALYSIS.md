# HFP protocol analysis — 2026-10-24

## Result and evidence boundary

**RC 2 HFP gateway control behavior is now observed; the microphone audio path remains unverified.** The user identified paired `QCOM-BTD` as the RC 2. Its Android UUID response included `111f`/`1112`, and a separate ordinary-app public RFCOMM probe completed the mandatory HFP service-level exchange. This is stronger than UUID evidence alone. It is not a full SDP dump, local HF advertisement, SCO trace or remote audio recording. The supplied AirPods report still does not establish its transport, microphone direction or DJI Fly acceptance. Linux hardware tests remain unexecuted. See the [recorded control result](evidence/rc2-qcom-btd-control-2026-10-24.json) and [Linux experiment](linux/README.md).

DJI documents Bluetooth 5.2 on RC 2 and a speaker without a built-in microphone. Its current livestream guide requires a connected microphone for DJI Fly 1.16.0 and later, but does not publish the Bluetooth profile or app audio-device condition used for this check. The guide's wording about plugging in a microphone must not be treated as proof that an arbitrary Bluetooth headset microphone is accepted. [DJI RC 2 specifications](https://www.dji.com/rc-2/specs), [RC 2 FAQ](https://www.dji.com/rc-2/faq), [DJI livestream guide](https://repair.dji.com/help/content?customId=en-us03400006727&documentType=artical&lang=en&paperDocType=paper&re=US&spaceId=34).

## Role and transport checklist

| Purpose | Local role required by this project | RC 2 counterpart to verify | Service class |
|---|---|---|---|
| HFP | Hands-Free (HF) | Audio Gateway (AG) | HF `0x111E`; AG `0x111F` |
| HSP fallback, separate experiment | Headset (HS) | Headset AG | HS `0x1108`; AG `0x1112` |

Use full base UUIDs, e.g. AG `0000111f-0000-1000-8000-00805f9b34fb`. BlueZ profile registration supplies SDP/RFCOMM plumbing; registering a UUID alone does not implement the HFP state machine or audio. `Device1.ConnectProfile` takes the **remote service UUID**, so a local HF connects to the RC 2 AG UUID `111F`, not its own `111E`. BlueZ device connection success can mean only one supported profile connected. [BlueZ Profile API](https://bluez.readthedocs.io/en/latest/profile-api/), [Device API](https://bluez.readthedocs.io/en/latest/device-api/).

HFP has separate RFCOMM control and SCO/eSCO audio paths. SLC requires protocol initialization, not merely an open socket. Normal initialization exchanges `BRSF`, conditionally `BAC`, then `CIND=?`, `CIND?`, and `CMER`; negotiated three-way/HF-indicator features can require `CHLD`/`BIND`. `BCC` requests codec connection; `+BCS: n` selects and `AT+BCS=n` acknowledges a codec. `VGM`/`VGS` control gain, not transport establishment. CVSD is codec ID 1; mSBC is ID 2, with 16 kHz speech PCM. Version/features and controller support determine the applicable branch. [Bluetooth SIG HFP 1.8, §§4.2, 4.11, 4.29, 5.7 and Appendix B](https://www.bluetooth.org/DocMan/handlers/DownloadDoc.ashx?doc_id=489628).

BR/EDR ACL, SDP and RFCOMM evidence must be distinguished from the synchronous voice link. BLE GATT advertisement/service, A2DP playback, a pairing bond, phone name or RFCOMM PCM bytes do not satisfy this project's microphone acceptance gates. Do not impersonate AirPods addresses, manufacturer data or identity.

## Observed S23 → RC 2 control exchange

On 2026-10-24 local time, test-only instrumentation ran under the ordinary target application UID with `BLUETOOTH_PRIVILEGED=0`. It required exactly one paired device named `QCOM-BTD`, connected using public `createRfcommSocketToServiceRecord` with remote AG UUID `111f`, and validated the following in order:

1. `AT+BRSF=0` → valid `+BRSF: 3943` and `OK`.
2. `AT+CIND=?` → seven required, uniquely named indicators with valid ranges and `OK`.
3. `AT+CIND?` → the matching number of values within those ranges and `OK`.
4. `AT+CMER=3,0,0,1` → `OK`.

The outcome was `MANDATORY_SLC_EXCHANGE_OBSERVED`; the recorded worker stopped after closing the socket at 1,733 ms. HF optional features were zero, so this run did not require the optional `BAC`, `CHLD` or `BIND` branches and negotiated no codec. It made no SCO/audio request, local HF SDP registration, microphone capture, call-control operation or identity change. The sanitized evidence retains validation events and feature/count values, not a raw over-the-air transcript. This demonstrates a bounded mandatory control exchange on this pair, not complete HFP compliance, persistent connection/reconnect behavior or audio acceptance.

This result corrects a broader control-impossibility reading of the Android API boundary: the system HF client is not needed for this public RFCOMM experiment. The missing phone microphone-to-SCO route remains a separate platform question. The integrated diagnostic UI continues to describe only its own non-connecting module; the successful probe lives in `androidTest`.

## Existing implementation selected for the Linux experiment

Use BlueZ + PipeWire + WirePlumber's existing native HFP implementation. WirePlumber 0.5 configuration exposes local `hfp_hf`, native backend and software SCO controls; the provided experiment temporarily selects only that role. Do not run an additional custom HFP daemon or a second session manager competing for the same profiles. The user's active logind seat also matters for Bluetooth device ownership. [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html).

PipeWire already implements HF feature/codec exchange and AT progression in `backend-native.c`; rewriting the parser/codec stack is unnecessary for this proof of concept. This is **Linux capability**, not evidence that Android's sandbox exposes equivalent services or sockets. [PipeWire native backend](https://raw.githubusercontent.com/PipeWire/pipewire/master/spa/plugins/bluez5/backend-native.c).

### Direction matters

The `bluez5.roles = [ hfp_hf ]` setting describes the **local** role. PipeWire transport/node `api.bluez5.profile` describes the **remote** role; for an AG peer its value is `headset-audio-gateway` (the AG family includes HSP). This naming is not a role reversal. Current source creates SCO nodes dynamically when an AG transport reaches pending/active state. SLC alone may therefore expose no routable sink; record this as an audio-activation blocker. [PipeWire profile names](https://raw.githubusercontent.com/PipeWire/pipewire/master/spa/plugins/bluez5/defs.h), [device node creation](https://raw.githubusercontent.com/PipeWire/pipewire/master/spa/plugins/bluez5/bluez5-device.c).

For the local HF, playing into its Bluetooth **Audio/Sink** sends the HF microphone/uplink toward the AG. Capturing its Bluetooth **Audio/Source** reads the AG/downlink. The lab routes a verified live ALSA microphone source into that AG sink using `pw-loopback`; it refuses A2DP, the opposite remote role, another peer and a missing/unknown voice codec. No default-source/sink change is needed. PipeWire can adapt the PCM graph to the selected transport format, but negotiated format, links and remote sound must be measured on the target installation. [PipeWire loopback](https://docs.pipewire.org/page_module_loopback.html), [pw-loopback options and serial target implementation](https://raw.githubusercontent.com/PipeWire/pipewire/master/src/tools/pw-loopback.c).

The current speech codec definitions expose node codec names `cvsd` (S16LE, mono, 8 kHz PCM) and `msbc` (S16, mono, 16 kHz PCM). HFP codec IDs differ from HCI coding-format values; do not copy a controller-format number into `AT+BCS`. [PipeWire CVSD codec](https://raw.githubusercontent.com/PipeWire/pipewire/master/spa/plugins/bluez5/hfp-codec-cvsd.c), [mSBC codec](https://raw.githubusercontent.com/PipeWire/pipewire/master/spa/plugins/bluez5/hfp-codec-msbc.c).

## What RC 2 hardware evidence must resolve

| Question | Sufficient evidence to record | Current result |
|---|---|---|
| Does RC 2 expose an AG service? | UUID result plus real AG protocol exchange; full SDP for channel/version/features | `111f`/`1112` UUIDs observed (cache/correlation limits); public `111f` RFCOMM and mandatory exchange passed; full SDP attributes not captured |
| Can ordinary S23 app code exchange mandatory SLC commands? | Ordered, validated `BRSF`/`CIND`/`CMER` responses using an ordinary app UID | Observed with HF features `0`, AG features `3943`, seven indicators, then socket cleanup; no persistent profile/audio claim |
| Was the reported AirPods connection voice-capable? | RC 2 firmware/Fly version, HFP SDP + AT + synchronous link and remote recording with AirPods | User report only |
| When does Fly detect a microphone? | Timestamped UI outcome before pairing, after SLC, after SCO, after active PCM | Unknown |
| Does RC 2 request/open SCO? | Matching HCI synchronous connection completion with status success, correct peer/handle | Unknown |
| Which codec is used? | AT codec negotiation (when supported), synchronous parameters, PipeWire codec/format | Unknown |
| What does the Fly start guard require? | Repeatable UI/audio route observations, or authorized platform diagnostics/source | Not published in reviewed DJI docs |
| Does speech arrive in RTMP? | Remote recording with unique spoken markers and mute/unmute control | Unknown |

An HCI connection-complete event with an error status is a failure, not a connection. SCO packet counters/active graph links show transport activity, not intelligible microphone sound. Capture decoded control messages and event status locally; the BlueZ monitor supports btsnoop capture/read, with privileged access depending on system policy. [BlueZ btmon manual](https://github.com/bluez/bluez/blob/master/doc/btmon.rst).

## Evidence-driven state policy

These are product/lab interpretations to implement only when their actual signals exist:

| State | Minimum observed signal |
|---|---|
| `UNSUPPORTED_PLATFORM` | No permitted local HFP-HF/profile + audio route on the execution platform |
| `DISCOVERABLE` | Adapter confirms discoverable and HF SDP registration exists |
| `PAIRING` / `PAIRED` | Real pairing progress / persisted authenticated bond |
| `SERVICE_CONNECTING` | An actual HFP control connection request, through a profile implementation or an explicitly scoped RFCOMM probe |
| `SERVICE_CONNECTED` | Required negotiated SLC steps complete without terminal error; record control-only scope and actual lifetime separately from audio |
| `AUDIO_CONNECTING` | A real codec/synchronous setup request |
| `AUDIO_CONNECTED` | Successful corresponding synchronous link event |
| `STREAMING` | Confirmed local microphone capture and links to the selected HF transport; remote audibility reported separately |
| `DISCONNECTED` / `ERROR` | Actual disconnect/status error, or expired stage deadline; stop capture |

The harness does not synthesize these states from generic BlueZ `Connected: yes`. Its reports keep hardware verdicts `NOT_TESTED`; a human must attach and assess the actual trace/UI/recording evidence. Suggested experiment deadlines are 60 s for discovery, 60 s for user pairing, 20 s for SLC and 20 s for audio setup. These are operator time limits, not measured DJI timing or protocol requirements. Stop after one failed stage, preserve the error, then retry deliberately; never loop pairing or capture indefinitely.

## Work portable from a successful Linux proof

The reusable outputs would be RC 2 service/feature requirements, an interoperability transcript, codec/PCM requirements, measured audio activation behavior and acceptance recordings. Android mandatory control has already been observed through public RFCOMM; it must not be conflated with OEM/system HF-client availability or privileged profile access. Android would still need a permitted full-profile implementation and an actual microphone-to-SCO audio route. Linux BlueZ D-Bus configuration and its ALSA/PipeWire graph do not exist as a normal Android APK interface. See [Android capability analysis](ANDROID_BLUETOOTH_CAPABILITIES.md) and [blockers](BLOCKERS.md) before attempting an Android audio transport implementation.
