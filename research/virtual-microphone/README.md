# Virtual Bluetooth Microphone research and diagnostics

Date: 2026-10-24 (Europe/Istanbul).

**Current scope:** the user now requires only RC 2 microphone recognition, without audio transmission. See [RC2_RECOGNITION_ONLY.md](RC2_RECOGNITION_ONLY.md) for the persistent control-session implementation and its separate acceptance test. The original full-audio research below remains as historical engineering context.

The requested stock Galaxy S23 → RC 2 Bluetooth microphone product is **not implemented or verified**. The public Android SDK does not expose a complete local HF accessory service and outgoing SCO microphone path. A follow-up test did complete the mandatory HFP control exchange over public RFCOMM; that narrower operation does not require the system HF-client API. JNI does not grant the missing audio capabilities. The application provides a diagnostic screen, the test APK provides the control probe, and the Linux directory provides an executable lab harness using the existing BlueZ/PipeWire stack.

## Deliverables

- [FEASIBILITY.md](FEASIBILITY.md): feasibility decision, boundaries and primary sources.
- [ANDROID_BLUETOOTH_CAPABILITIES.md](ANDROID_BLUETOOTH_CAPABILITIES.md): SDK/permission/audio-path analysis and alternatives.
- [HFP_PROTOCOL_ANALYSIS.md](HFP_PROTOCOL_ANALYSIS.md): role hypothesis, SDP/SLC/SCO distinction and Linux protocol work.
- [BLOCKERS.md](BLOCKERS.md): concrete system changes and outstanding evidence.
- [VERIFICATION.md](VERIFICATION.md): checks actually run and hardware acceptance procedure.
- [ANDROID_CONTROL_PROBE.md](ANDROID_CONTROL_PROBE.md): actual public-RFCOMM mandatory HFP control exchange with the user-identified QCOM-BTD/RC 2; test-only source and reproduction commands.
- [IOS_FEASIBILITY.md](IOS_FEASIBILITY.md): current public iOS SDK transport boundary for the recognition-only goal; Core Bluetooth Classic is not a generic RFCOMM/HFP stream.
- [IOS_APP_ONLY_FOLLOWUP.md](IOS_APP_ONLY_FOLLOWUP.md): follow-up native-code/accessory-entitlement audit for the user's iPhone-only constraint and the Apple technical query.
- [APPLE_SUPPORT_SUBMISSION_2026-10-25.md](APPLE_SUPPORT_SUBMISSION_2026-10-25.md): confirmed Apple Developer Support case 103007562941, exact submitted message and remaining proof boundary.
- [IOS_VERIFICATION.md](IOS_VERIFICATION.md): implemented iOS availability diagnostics and the checks actually run, separate from missing physical iPhone/RC 2 proof.
- [android/collect-capabilities.py](android/collect-capabilities.py): read-only ADB evidence collector, excluding Bluetooth addresses, serials and raw dumps.
- [evidence/galaxy-s23-2026-10-24.json](evidence/galaxy-s23-2026-10-24.json): actual connected S23 observations, including Bluetooth OFF and unknown/absent properties.
- [evidence/galaxy-s23-bluetooth-on-2026-10-24.json](evidence/galaxy-s23-bluetooth-on-2026-10-24.json): second observation with Bluetooth ON; gateway/headset service present, no HF-client service in the dump.
- [linux/](linux/): Linux setup, discovery, microphone routing and evidence collection harness; see its own README.

## Android diagnostic screen

Open the **More** menu → **Virtual Bluetooth Microphone**. Grant Nearby devices access, select an already paired controller and explicitly connect. The control service keeps the validated HFP session open until disconnect/failure; the app and ongoing notification both provide disconnect. QCOM-BTD is preselected only when exactly one paired device has that user-identified name. The screen separates the actual control connection from unverified DJI Fly microphone recognition. **Developer diagnostics** adds cached UUIDs, a separate SDP query and a sanitized report. Pairing is done in Android Bluetooth settings; the app does not scan or rename the phone.

Cached UUIDs and the bounded asynchronous SDP query are separate. `0x111F` or `0x1112` is evidence of an advertised gateway service, not an established HFP connection, SCO channel or usable DJI Fly microphone. Android's UUID broadcast has no request identifier and cannot guarantee that a response came from this exact query. The copy action excludes remote names, addresses, custom UUIDs, exception messages and audio. No microphone permission, microphone recording, microphone foreground service, hidden API, JNI engine or fabricated HFP state is added.

## Reproduce the Android shell observations

Choose the intended authorized device explicitly; do not reuse a historical serial:

```sh
adb devices -l
python3 research/virtual-microphone/android/collect-capabilities.py \
  --serial YOUR_DEVICE_SERIAL --output /tmp/smd-bluetooth-capabilities.json
python3 -m unittest discover -s research/virtual-microphone/android -v
```

`--adb /absolute/path/to/adb` is supported. A missing property is saved as `null`, not `false`. ADB runs as the shell user and is diagnostic evidence only; its ability to read a dump does not establish access for an ordinary APK. Do not publish raw `dumpsys`, HCI snoop, audio or stream keys. Bluetooth snoop collection requires separate deliberate hardware testing and local private storage.

## Phase status

| Phase | Result |
| --- | --- |
| 1: Feasibility | Primary-source analysis, actual S23 observations, RC 2 gateway UUIDs and mandatory control exchange provided. SCO and OEM audio HAL remain unmeasured. |
| 2: Protocol PoC | Mandatory Android RFCOMM/HFP control exchange observed; runnable Linux lab harness provided. No Linux Bluetooth/audio execution or audible stream proof on this macOS host. |
| 3: Android microphone implementation | Blocked by stock third-party API/system audio boundaries; no fake native transport added. |
| 4: App integration | Capability/SDP diagnostics and explicit persistent HFP control integrated into the native Compose app. The recognition-only goal has separate verification below. |
| 5: Verification | Automated checks and diagnostic UI evidence recorded separately from pending hardware criteria. |

Follow-up hardware result: after the user identified QCOM-BTD as RC 2, the S23 query returned HFP/HSP gateway UUIDs, and an isolated test APK completed BRSF/CIND/CMER over public RFCOMM with `BLUETOOTH_PRIVILEGED` ungranted. This establishes that a minimal HFP control exchange is possible without the system HF-client API. The complete accessory/microphone path remains unimplemented and unverified; no SCO or audio was attempted. See the control-probe report above.

The current debug APK was built, installed and launched on the connected S23 with `env DJI_ANDROID_DEVICE_SERIAL=<explicit-device> ./start.sh`. The Turkish diagnostic UI and Bluetooth-ON paired-device list were observed in its UI hierarchy. Android 137 unit tests and all implemented lab-tool tests passed. See the verification record for full results and untested cases.
