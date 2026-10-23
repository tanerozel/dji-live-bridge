# Revised goal: RC 2 microphone recognition without audio

The user reduced the goal on 2026-10-24: it is sufficient for RC 2 to recognize the phone as a microphone; transmitting microphone audio is not required.

## Implementation and evidence boundary

The earlier [real S23/RC 2 control test](ANDROID_CONTROL_PROBE.md) completed the mandatory HFP initialization exchange. The application now exposes an explicit paired-device control connection and holds it open using a `connectedDevice` foreground service. It uses the same public RFCOMM mechanism and HF optional features zero, validates BRSF/CIND/CMER, and keeps reading control events until disconnect or failure. It does not open a microphone, send PCM, negotiate an audio codec, request SCO, change the Bluetooth name/class or make a call.

The application distinguishes **control connected** from **DJI Fly microphone recognition**. The first is reported only after the actual mandatory exchange succeeds; the second requires an observation on the RC 2. The old blanket third-party-app prohibition message is removed. Bluetooth permission, device selection and connect/disconnect are available in the normal screen; SDP details remain in developer diagnostics.

Closing the screen does not close the requested control session. The ongoing notification has a disconnect action. There is no automatic reconnect; reconnect is an explicit user action. Permission loss, adapter shutdown, unpairing, remote EOF and service destruction release the connection. No remote address or arbitrary AT response is written to diagnostics or preferences.

## Why this is worth testing

Android's reference audio service registers an active HFP headset input separately from its SCO audio state. This makes recognition without captured speech or an active SCO stream plausible. It does **not** establish DJI Fly's exact condition or RC 2's vendor behavior. [AOSP BtHelper](https://android.googlesource.com/platform/frameworks/base/+/master/services/core/java/com/android/server/audio/BtHelper.java).

Bluetooth device interaction with the user-granted `BLUETOOTH_CONNECT` permission qualifies for the `connectedDevice` foreground-service type. No microphone foreground-service type or microphone permission is needed for this control-only operation. [Android foreground-service types](https://developer.android.com/develop/background-work/services/fgs/service-types#connected-device).

## Hardware acceptance

1. On RC 2, open the same DJI Fly livestream setup and record the microphone warning / Start availability with the phone disconnected.
2. On S23, open **More → Virtual Bluetooth Microphone**, select the user-identified **QCOM-BTD**, then connect.
3. Wait for **control connection open**. Keep it open while reopening or refreshing the RC 2 livestream setup.
4. Record whether Fly removes the microphone warning or accepts the device as a microphone. A generic Bluetooth connected label is insufficient.
5. Disconnect from the app/notification and repeat the before/after check to exclude stale state. No public livestream is required to check the revised recognition criterion.

If recognition fails, investigate RC 2's active HFP device/input routing and exact Fly predicate. A silent SCO transport might be required by that implementation, but this cannot be assumed from the absence of microphone audio. Persistent RFCOMM alone must not be reported as microphone recognition.

## Verification record

Build, automated checks and actual persistent-session results will be recorded after running the final application. DJI Fly's microphone decision remains unverified until observed on the RC 2.
