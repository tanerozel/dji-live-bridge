# iOS RC 2 recognition-only feasibility

Reviewed on 2026-10-24 against **Xcode 27.0 (27A266a), iPhoneOS 27.0 SDK** and current Apple documentation. Scope: make RC 2 recognize the iPhone as a microphone, with **no audio capture or transmission**. This is a source/API audit, not an iPhone–RC 2 hardware result.

## Decision

The successful Android mechanism cannot currently be ported to an ordinary iOS app using a documented public equivalent. Android opens the RC 2 HFP Audio Gateway service (`111f`) with a public RFCOMM socket, validates `BRSF` / `CIND` / `CMER`, then keeps the control channel open. See [the actual control test](ANDROID_CONTROL_PROBE.md) and [recognition-only scope](RC2_RECOGNITION_ONLY.md).

The missing iOS bridge is **app-accessible RFCOMM to the RC 2 HFP gateway, or a public local HFP Hands-Free service API**. Removing audio from the goal removes the SCO requirement; it does not remove that control-channel requirement. The conclusion concerns the supported app API contract, not the iPhone radio's capabilities.

## What Core Bluetooth Classic actually provides

Core Bluetooth is **not BLE-only**. Apple's current [framework overview](https://developer.apple.com/documentation/corebluetooth) includes BR/EDR Classic. Apple's [WWDC19 explanation](https://developer.apple.com/videos/play/wwdc2019/901/) distinguishes system audio profiles such as HFP from the GATT interface made available over Classic. The same `CBPeripheral` service/characteristic operations work across the supported transports. Transport bridging activates system-supported Classic profiles; it does not register a new local HF role or hand the app their RFCOMM stream.

The current SDK agrees with that contract:

| Public SDK evidence | Meaning for this task |
| --- | --- |
| `CoreBluetooth.framework/Headers/CBPeripheral.h:133–208`: GATT discovery, characteristic reads/writes | A GATT service is not the RC 2's HFP RFCOMM service |
| `CBPeripheral.h:263–271`: `openL2CAPChannel:` accepts a PSM | A stream API exists; its name alone does not establish raw BR/EDR RFCOMM access |
| `CBPeripheralManager.h:287–297`: system allocates a published L2CAP PSM; app arranges discovery | Does not publish an HF SDP record or bind the RFCOMM service |
| `CBCentralManagerConstants.h`: `CBConnectPeripheralOptionEnableTransportBridgingKey` brings up Classic profiles when LE connects | Reconnects system-supported roles; no app-selectable HF registration or AT stream |
| `CBPeripheral.h:274–291`, `CBCentralManager.h:69`: iOS 27 additions are channel sounding | Ranging additions do not provide RFCOMM/HFP |

Paths above are relative to `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS27.0.sdk/System/Library/Frameworks/`.

### L2CAP is not a demonstrated escape hatch

Apple introduced these Core Bluetooth streams as **L2CAP Connection Oriented Channels for LE**, with PSM discovery associated with a GATT service: [WWDC17 session 712 slides, pages 57–62](https://devstreaming-cdn.apple.com/videos/wwdc/2017/712jqzhsxoww3zn/712/712_whats_new_in_core_bluetooth.pdf). The current `openL2CAPChannel` header has no selector for raw BR/EDR mode, RFCOMM, SDP or HFP. No current Apple documentation examined describes opening the RC 2's Classic RFCOMM service through that API.

It would therefore be speculative to pass the RFCOMM PSM `0x0003` and implement framing while claiming this is a supported HFP transport. A GATT connection, an allocated Core Bluetooth PSM or successful LE stream must never be labeled “HFP connected” or “RC 2 microphone recognized.” A firmware-provided, documented GATT-to-HFP bridge on RC 2 could change the architecture; no such bridge is established here.

## Other public frameworks examined

| Framework | Verified public scope | Why it does not supply the missing bridge |
| --- | --- | --- |
| [External Accessory](https://developer.apple.com/documentation/externalaccessory) | Manufacturer-approved MFi accessory protocols | `EAAccessory.h:5–11` explicitly says streams are wrapped in iAP; `EASession.h:24` selects an accessory protocol string. It is not an arbitrary RFCOMM socket to HFP `111f`. No DJI iAP protocol or manufacturer authorization is supplied. |
| [AccessorySetupKit](https://developer.apple.com/documentation/accessorysetupkit/discovering-and-configuring-accessories) | Discovery, user authorization and pairing; communication then uses Core Bluetooth / networking | `ASAccessory.h:44–50` describes transport bridging as activating Classic profiles from LE. Pairing permission does not add a new HFP service/socket API. |
| [AVAudioSession.allowBluetoothHFP](https://developer.apple.com/documentation/avfaudio/avaudiosession/categoryoptions-swift.struct/allowbluetoothhfp) | Makes a paired **remote** HF device available as the iPhone app's audio input | `AVAudioSessionTypes.h:504–520` describes input routing. It does not expose the iPhone as an HF microphone to another gateway; changing the audio session is unnecessary for the control-only goal. |
| [AudioAccessoryKit](https://developer.apple.com/documentation/audioaccessorykit) | A companion app reports a physical audio accessory's placement, switching and other capabilities | The iOS 27 public Swift interface adds spatialization/head tracking to accessory configuration; it exposes no raw RFCOMM or local HF profile registration. Registering a physical accessory's state does not turn the iPhone into that accessory. |
| [IOBluetooth](https://developer.apple.com/documentation/iobluetooth) | macOS RFCOMM/SDP/Hands-Free objects | These public objects are absent from the iPhoneOS 27 SDK. A macOS example is not an iOS implementation route. |

## Honest app scope and acceptance

An iOS screen may show the specific feature's availability and the actual Bluetooth authorization/power state. It must not fabricate a paired-device list, assume QCOM-BTD is a Core Bluetooth peripheral, advertise `111e` as a GATT service and call it HFP, or turn a generic connection/route into RC 2 recognition. A diagnostics module is not a working feature port.

To revise the unsupported control verdict, obtain a documented permitted transport or manufacturer integration that carries the required HFP control messages. Then implement the real exchange and test on an iPhone against this RC 2. Record separately: actual control connection and its lifetime, DJI Fly's before/after microphone warning, and disconnect reversal. No audio recording or public livestream is required for that recognition-only acceptance test.

No private API, entitlement bypass, jailbreak, Bluetooth identity impersonation, microphone capture, silent SCO or iOS hardware success is proposed or claimed by this review.
