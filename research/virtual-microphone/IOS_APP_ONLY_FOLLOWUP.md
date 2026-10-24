# iPhone-app-only transport follow-up

Reviewed: 2026-10-25. Scope confirmed by the user: **ordinary iPhone app only, no external
device**. The acceptance criterion is removal of DJI Fly's microphone warning on RC 2; microphone
audio is not needed. This follow-up is a source/SDK audit, not a hardware test.

## Additional routes examined

| Candidate | Finding | Remaining requirement |
| --- | --- | --- |
| C/C++ or a portable Bluetooth stack | Protocol messages and state machines can be implemented in C. iPhoneOS 27's public `sys/socket.h` has no `AF_BLUETOOTH`, HCI/RFCOMM socket protocols or Bluetooth socket address types. `IOBluetooth` is absent from that SDK. | A supported app-accessible RFCOMM or controller transport, regardless of programming language |
| AccessoryTransportExtension and its entitlement | Feature payloads reach the app's extension; the extension must then send them through an accessory connection. The `.bluetooth` payload designation does not open an HFP stream. | Actual Bluetooth transport and the required HF role |
| AudioAccessoryKit, including iOS 27 changes | Describes a remote physical audio accessory's state and capabilities. | An API that presents the iPhone itself as an HF/HS accessory to RC 2 |
| AccessorySetupKit/proximity pairing/Classic bridging | Helps authorize, pair and activate supported connections. | Pairing alone does not establish the missing HFP control role |
| Developer capabilities | No documented requestable capability enabling local HFP-HF or arbitrary HFP RFCOMM access was found. | An exact Apple-provided API/capability and its eligibility conditions |

`IOKitLib.h` is present in the SDK; its generic user-client opening function does not establish
an accessible Bluetooth HCI user client. Do not infer controller access from that header alone.
Likewise, copying Linux's numeric socket family is invalid: Darwin's socket family 31 is `AF_NATM`.

The result is **no working app-only route found**, not a proof that every manufacturer-specific,
undocumented or future integration is impossible. No new connection code was added without a
demonstrated transport. Existing availability diagnostics do not satisfy the acceptance criterion.

## Sources

- [Apple: receiving notifications on an accessory](https://developer.apple.com/documentation/AccessoryTransportExtension/receiving-ios-notifications-on-an-accessory): the transport extension's code still sends to the accessory.
- [Accessory transport entitlement](https://developer.apple.com/documentation/BundleResources/Entitlements/com.apple.developer.accessory-transport-extension): authorizes that extension, not arbitrary HFP access.
- [AudioAccessoryKit](https://developer.apple.com/documentation/audioaccessorykit).
- [Apple proximity pairing](https://developer.apple.com/proximity-pairing).
- [Apple runtime security](https://support.apple.com/en-ca/guide/security/sec15bfe098e/web): ordinary third-party code uses OS-provided services and signed capabilities.
- [BTstack transport requirements](https://github.com/bluekitchen/btstack): a portable stack still needs controller access; its [historical iOS port](https://bluekitchen-gmbh.com/btstack/ports/existing_ports.html) requires jailbreak/Cydia and is outside this user's scope.
- Local SDK: `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS27.0.sdk`, especially `usr/include/sys/socket.h` and the public AccessoryTransportExtension / AudioAccessoryKit interfaces.

## Original Apple technical query

After the user's instruction to continue, a shorter reviewed version was submitted to Apple
Developer Support on 2026-10-25. **Case ID: 103007562941**. The [exact submitted text](APPLE_SUPPORT_QUERY_2026-10-25.txt)
and [submission record](APPLE_SUPPORT_SUBMISSION_2026-10-25.md) are saved separately. The longer
draft below was not sent verbatim. No technical response or granted capability has been received.

**Subject:** Supported iPhone app integration for HFP Hands-Free control-only connection to a remote Audio Gateway

We develop Stream My Drone, a native iOS streaming application. We need an ordinary iPhone
application, without external hardware or jailbreak, to establish and maintain an HFP Hands-Free
service-level control connection to a DJI RC 2 controller. Our goal is microphone recognition by
DJI Fly before livestreaming. We do not need microphone capture, speech transmission or SCO/eSCO.

On Android, an ordinary app opened the controller's advertised HFP Audio Gateway service UUID
`0000111f-0000-1000-8000-00805f9b34fb` using public RFCOMM. It successfully exchanged
`AT+BRSF=0`, `AT+CIND=?`, `AT+CIND?` and `AT+CMER=3,0,0,1`, with validated responses, and can
maintain the control channel in the Android implementation; the independently recorded hardware
test was a bounded exchange. The user reported that microphone recognition then worked;
we have not independently observed the DJI Fly screen during that report.

We reviewed the iPhoneOS 27.0 SDK and documentation for Core Bluetooth (including Classic GATT,
L2CAP channels and transport bridging), ExternalAccessory, AccessorySetupKit, AudioAccessoryKit
and AccessoryTransportExtension. We have not found an app-accessible RFCOMM stream to the remote
HFP gateway, or an API registering the local HFP HF role. We understand that AVAudioSession's
Bluetooth input routing handles the opposite accessory relationship and does not provide this
control stream. No iPhone-to-RC-2 HFP connection has been tested or claimed.

Please clarify:

1. Is there an existing public iOS API for an app to establish this control-only local HFP HF role
   and connect to a remote HFP Audio Gateway? If so, which API and supported OS/device versions?
2. Is a specific developer-requestable entitlement, managed capability or approved manufacturer
   integration available? If so, what is its identifier, application process and regional scope?
3. Does any current Core Bluetooth L2CAP API explicitly permit BR/EDR RFCOMM to this gateway,
   rather than LE credit-based channels or GATT over BR/EDR? Please provide the supported contract.
4. If this is not currently supported, please confirm that boundary and advise the appropriate
   enhancement/interoperability request category. We do not assume that iOS already exposes or
   internally uses this local HF role for Apple's own services.

Success would be a supported connection lifetime and reversal on disconnect, with RC 2's DJI Fly
microphone warning observed before/after. Bluetooth pairing, a GATT connection or an audio route
label alone is not our acceptance criterion.

The shorter query was sent through Apple's [developer support](https://developer.apple.com/contact/)
email form under **Development and Technical → Other Development or Technical Questions**.
Apple also documents [interoperability requests outside the EU](https://developer.apple.com/support/interoperability-requests/)
and a separate [EU process with technical reference queries](https://developer.apple.com/support/ios-interoperability/).
No formal interoperability or entitlement application was submitted. Those processes concern
clarification or new access, not already-granted capabilities; classification and eligibility
still need confirmation.
