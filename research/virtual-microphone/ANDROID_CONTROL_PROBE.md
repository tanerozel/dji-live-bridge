# Actual S23 → QCOM-BTD / RC 2 control probe

Test date: 2026-10-24 (Europe/Istanbul). The user identified the paired Bluetooth name **QCOM-BTD** as the DJI RC 2. The selected device returned gateway UUIDs `0x111F` (HFP) and `0x1112` (HSP) in the Android diagnostic query. See [SDP observation](evidence/rc2-qcom-btd-sdp-2026-10-24.json).

## Recorded result

The [recorded control run](evidence/rc2-qcom-btd-control-2026-10-24.json) returned `MANDATORY_SLC_EXCHANGE_OBSERVED`, `passed=true`, instrumentation result `-1`:

| Measured event | Result |
| --- | --- |
| Ordinary target application UID | Confirmed; no root/shell UID |
| `BLUETOOTH_PRIVILEGED` | Not granted |
| Exact paired QCOM-BTD name matches | One |
| Public RFCOMM socket to remote HFP AG `111F` | Connected |
| `AT+BRSF=0` | Valid remote feature mask `3943`, then `OK` |
| `AT+CIND=?` | Seven mandatory indicator definitions/ranges validated, then `OK` |
| `AT+CIND?` | Values validated against their definitions, then `OK` |
| `AT+CMER=3,0,0,1` | `OK` |
| Worker cleanup | Worker stopped after socket-close cleanup |
| Total recorded probe duration | 1733 ms; this is **not audio latency** |

The HF advertises zero optional features, so the negotiated initialization branch does not require BAC/CHLD/BIND. No codec-connection request, voice recognition, call action, SCO connection, audio capture or audio routing is attempted. The result proves this mandatory control exchange over public RFCOMM on the actual S23/RC 2 pair. It does **not** prove local HF SDP advertising, a system HF profile, sustained interoperability, SCO/eSCO, a negotiated audio codec, DJI Fly microphone detection or phone speech in RTMP.

An earlier invocation used display-mode instrumentation output and the collector did not retain its result. No outcome is attributed to it. A parser-only on-device run then passed; the recorded run used raw result output. There is no automatic Bluetooth retry loop.

## Source and isolation

- `mobile/android/app/src/androidTest/java/com/streammydrone/app/hfpcontrolprobe/HfpControlProbeInstrumentation.kt`
- `mobile/android/app/src/androidTest/java/com/streammydrone/app/hfpcontrolprobe/HfpControlProtocol.kt`

These sources are included only in the Android test APK. The production/release APK receives no transmitter, permissions, sockets or background service from this probe. No new dependency or native library is added. The production diagnostic screen still makes no RFCOMM connection.

The runner requires user-granted Bluetooth connect permission, Bluetooth ON, exactly one paired QCOM-BTD, and ordinary target application credentials. It also refuses a live relay in its process. **Instrumentation restarts the target app**, so this in-process check cannot validate the app's state before launch. The operator must separately confirm no relay/stream is active first. For the recorded run there was no active relay service; before the first run the existing notification said it was waiting for the drone.

## Reproduce deliberately

1. Stop any active broadcast/receiver before instrumentation. Preserve the app's profiles and keys. Open the existing developer diagnostics and grant Nearby devices access if needed; pair the intended RC 2 in Android settings.
2. Select the intended authorized S23 explicitly with `adb devices -l`; replace `YOUR_SERIAL` below.
3. From `mobile/android`, with the existing JDK/SDK environment:

```sh
./gradlew :app:assembleDebugAndroidTest
```

4. From the repository root:

```sh
adb -s YOUR_SERIAL install -r -t mobile/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s YOUR_SERIAL shell am instrument -w -r -e targetDeviceName QCOM-BTD \
  com.streammydrone.app.test/com.streammydrone.app.hfpcontrolprobe.HfpControlProbeInstrumentation
```

Check `probe_report_json`, `probe_exit_code` and `INSTRUMENTATION_CODE`; ADB's shell exit status alone is insufficient. Success is `probe_exit_code=0` and instrumentation code `-1`; failures remain failures. Without `-r`, Android displays the JSON stream rather than raw bundle fields. Reports contain only fixed events/numbers, not remote addresses, arbitrary response text, stream keys or audio.

Parser-only self-check, without Bluetooth operations:

```sh
adb -s YOUR_SERIAL shell am instrument -w -r -e parserOnly true \
  com.streammydrone.app.test/com.streammydrone.app.hfpcontrolprobe.HfpControlProbeInstrumentation
```

Connect has an 8-second deadline, each AT exchange a 4-second deadline, and the runner an overall 28-second wait plus bounded cleanup. It closes the socket after either completion or failure. Do not interpret a timeout as absent HFP support. No stream or microphone is started by either command.

Remove only the test package after the experiment, then reopen the normal app:

```sh
adb -s YOUR_SERIAL uninstall com.streammydrone.app.test
adb -s YOUR_SERIAL shell am start -W -n com.streammydrone.app/.MainActivity
```
