# App Store listing - English (U.S.)

Last updated: 2026-10-01. Character counts are checked against App Store Connect's limits.

## Name (30)

```text
Stream My Drone
```

## Subtitle (30)

```text
Take DJI Fly live on any RTMP
```

## Promotional text (170)

Can be changed at any time without a new version.

```text
Send your drone's live picture from DJI Fly to several platforms at once, with a live preview, RTMPS and automatic reconnects. No account, no subscription, no tracking.
```

## Description (4000)

```text
Stream your drone footage live, directly from DJI Fly to the platforms you choose.

Stream My Drone turns your iPhone or iPad into a local RTMP bridge. It receives the Custom RTMP feed from a compatible DJI remote controller, shows a live preview, and forwards the same H.264/AAC stream to one or more RTMP or RTMPS destinations, without re-encoding it.

HOW IT WORKS
1. Connect your iPhone and the DJI remote controller to the same trusted Wi-Fi network, or use your iPhone's Personal Hotspot.
2. Enter the RTMP address shown by the app in DJI Fly.
3. Add your destination's server address and stream key.
4. Preview the feed, switch on your platforms, and tap Go live.

KEY FEATURES
• Send one live feed to several destinations at the same time
• Ready-made profiles for popular live platforms and any custom RTMP or RTMPS server
• Secure RTMPS with certificate and host name validation
• Live preview with resolution, bitrate, codec and connection details
• Automatic reconnection after short network interruptions, while the drone's last picture is held
• Test-video mode to check the whole path before you fly
• Keep broadcasting in Picture in Picture while you use another app
• Available in 15 languages
• Light, dark, midnight blue, sand and system themes

PRIVACY AND SECURITY
Stream keys are kept in the iOS Keychain on this device only. They are never synced, backed up, logged or sent to us. Stream My Drone has no account and no server of its own, and collects no data: video goes from your iPhone directly to the destinations you set up. RTMP is unencrypted, so use RTMPS whenever your platform supports it.

GOOD TO KNOW
• Live drone use needs a compatible DJI remote controller or app with Custom RTMP publishing
• Each platform decides who may go live, and some issue a new stream key for every broadcast
• Every active destination uses its own upload bandwidth
• Use only a trusted local network for the controller-to-iPhone connection
• Stream My Drone is a streaming bridge, not a flight-control app

Stream My Drone is an independent product and is not affiliated with, endorsed by or sponsored by DJI or any streaming platform. DJI and the platform names are trademarks of their respective owners.
```

## Keywords (100)

Comma-separated, no spaces. App Review rejects other companies' or apps' names here
(Guideline 2.3.7), so no DJI or platform names.

```text
rtmp,rtmps,live,broadcast,multistream,restream,aerial,quadcopter,fpv,go live,streaming,video,remote
```

## URLs and other fields

| Field | Value |
| --- | --- |
| Support URL | `https://tanerozel.github.io/dji-live-bridge/` |
| Marketing URL | (optional) `https://tanerozel.github.io/dji-live-bridge/` |
| Privacy Policy URL (App Information) | `https://tanerozel.github.io/dji-live-bridge/privacy-policy.html` |
| Copyright | `2026 Taner Özel` |
| Primary category | Photo & Video |
| Secondary category | (optional) Utilities |
| Age rating | Every questionnaire answer "None" / "No" → 4+ |
| App Privacy | Data Not Collected |
| Price | Paid, one-time: USD 4.99 (base country United States, other countries equalized by Apple) |

## App Review information

- Sign-in required: **off** (the app has no account).
- Contact: name, phone and e-mail of the person App Review can reach.

Notes:

```text
Stream My Drone receives the live RTMP feed that the DJI Fly app on a DJI remote controller (for example the DJI RC 2) sends over the local network, and forwards it to RTMP/RTMPS servers the user sets up. It needs no account and collects no data.

TESTING WITHOUT A DRONE
1. Open the app and tap "Skip" on the short guide.
2. On the connect card, tap "No drone? Try a test video" and pick any video from the photo library. The app publishes it into its own receiver exactly as DJI Fly would, and the preview appears.
3. To try a live output, switch on a platform (or "Custom RTMP" for any RTMP/RTMPS server), enter its stream key, then tap "Go live".

BACKGROUND MODE
The "audio" background mode is declared because Picture in Picture requires it. While live, leaving the app moves the drone's picture into Picture in Picture so the broadcast keeps running; the app plays no sound. Without Picture in Picture, the broadcast is ended cleanly after the short time iOS allows.

LOCAL NETWORK
The local network permission is used only to accept the incoming RTMP connection from DJI Fly on the remote controller.

PLATFORM NAMES AND ICONS
Platform names and icons are shown only so users can recognise which destination a saved stream key belongs to. The app is not affiliated with DJI or any of these platforms, as stated in the description.
```

### Reply to Guideline 2.1 "Information Needed" (2026-10-02)

The first submission came back with the standard request for new accounts: a screen recording on
a physical device plus answers to five questions, sent as a reply **and** put into the Notes
field above (this block replaces the Notes; it already contains the testing steps). Attach the
screen recording to the reply.

```text
Thank you for the review. The requested information follows; the attached screen recording was made on an iPhone 14 Pro Max running the latest iOS and starts with launching the app.

1. SCREEN RECORDING
Attached. It shows the whole app: launching it, the short guide, the connect card with the RTMP address DJI Fly publishes to, the test video standing in for the drone (the same path App Review can use without a drone, see 3), switching on a platform and entering its stream key, going live (the platform's own "you are live" notification appears), Picture in Picture keeping the broadcast running after leaving the app, and ending the broadcast. With a drone, the picture comes from DJI Fly on the remote controller instead of the test video; everything else is identical. The app has no account, no login, no user-generated content and no in-app purchases; it is a paid app with all features included.

2. PURPOSE AND AUDIENCE
Stream My Drone is for drone pilots and content creators who fly DJI drones with the DJI Fly app. DJI Fly can publish its live picture to one custom RTMP address only. This app receives that RTMP feed on the iPhone over the local network, shows a live preview, and forwards the same stream, without re-encoding, to one or more live streaming platforms at the same time. It replaces a computer or a paid cloud restreaming service in the field.

3. SETUP AND MAIN FEATURES
No account or login is needed.
- With a drone: put the iPhone and the DJI remote controller on the same Wi-Fi network (or use the iPhone's Personal Hotspot). Copy the address shown on the app's connect card (rtmp://<iPhone IP>:1935/drone) into DJI Fly → GO FLY → ... → Transmission → Live Streaming Platforms → RTMP and start the stream. The picture appears in the app.
- Without a drone: tap "Skip" on the guide, then "No drone? Try a test video" on the connect card and pick any video from the photo library. The app publishes it into its own receiver exactly as DJI Fly would, and the preview appears.
- Going live: switch on a platform tile, paste the stream key from that platform's live dashboard (the server address is prefilled where the platform publishes one; "Custom RTMP" accepts any RTMP/RTMPS server), then tap "Go live". "End broadcast" stops it. A stream key from the reviewer's own YouTube or other account works; the preview and test video need no key.
- While live, leaving the app moves the picture into Picture in Picture so the broadcast keeps running. This is why the "audio" background mode is declared; the app plays no sound. The local network permission is used only to accept the incoming RTMP connection from DJI Fly.

4. EXTERNAL SERVICES
The app has no server, account system, analytics, advertising, payment processor or AI service of its own, and collects no data. It connects only to:
- DJI Fly on the user's DJI remote controller, which sends the picture over the local network;
- the RTMP/RTMPS ingest servers of the platforms the user switches on (for example YouTube, Facebook, Instagram, TikTok, Twitch, Kick, or a custom server), using the user's own stream keys.
Stream keys are stored in the iOS Keychain on the device only. Apple frameworks used: VideoToolbox, AVFoundation/AVKit (Picture in Picture), Security (Keychain and certificate checks for RTMPS). The RTMP protocol is implemented by a bundled open-source Rust library with OpenSSL for RTMPS.

5. REGIONS
The app works the same in all regions. Whether a user may go live is decided by each streaming platform for the user's account.

6. REGULATION AND THIRD-PARTY MATERIAL
The app is not in a regulated industry and contains no protected third-party content. Platform names and icons are shown only so users can recognise which destination a saved stream key belongs to. The app is not affiliated with DJI or any streaming platform, as stated in the description.
```

Screen recording checklist (iPhone on the latest iOS, Control Center → Screen Recording,
microphone off, start the recording before opening the app):

1. Open the app from the Home Screen; show the guide and tap through or "Skip".
2. Show the connect card and the RTMP address, then start the stream in DJI Fly on the RC 2 (the
   iPhone does not record the RC 2's screen) and wait until the drone's picture appears (READY).
3. Switch on a platform, open its key screen, paste a stream key, save.
4. Tap "Go live", let it run a few seconds (LIVE timer and bitrate moving).
5. Go to the Home Screen: Picture in Picture keeps playing; come back to the app.
6. Open the technical details, then tap "End broadcast".
7. Optional: "No drone? Try a test video" to show the path App Review will use.

Keep it short (1–3 minutes). Stream keys are masked in the app, but do not show a platform's
dashboard with the key visible.

## Screenshots

Required sizes (App Store Connect scales them to the smaller displays):

- iPhone 6.9": `1290 x 2796` (or 6.5": `1284 x 2778` / `1242 x 2688`)
- iPad 13": `2064 x 2752` (or `2048 x 2732`) — required because the app runs on iPad

Take them from the iOS app, not from Android: App Review rejects screenshots that do not show
the app as it runs on the device (Guideline 2.3.3).

iPhone, upload-ready (`1284 x 2778` JPEG, no alpha; this account's slot is the 6.5" display), in upload order:

1. `store-assets/promo-screenshots/en-US/01-live-hero.jpg` - Take your drone live
2. `store-assets/promo-screenshots/en-US/02-connect-drone.jpg` - Connect in seconds
3. `store-assets/promo-screenshots/en-US/03-preview.jpg` - Check it before you go live
4. `store-assets/promo-screenshots/en-US/04-details.jpg` - Every detail at a glance

The same set with Turkish captions is in `promo-screenshots/tr-TR/` for a `tr` localization.
Generated from `store-assets/promo-screenshots/source/` (iPhone 17 simulator shots in `raw/`, the
Mardin sunset test video playing; captions for every language in `template.html`; re-render with `./render.sh`).

iPad 13", upload-ready (`2064 x 2752` JPEG, no alpha), in upload order:

1. `store-assets/promo-screenshots/en-US/ipad/01-live-hero.jpg` - Take your drone live
2. `store-assets/promo-screenshots/en-US/ipad/02-connect-drone.jpg` - Connect in seconds
3. `store-assets/promo-screenshots/en-US/ipad/03-reconnect.jpg` - Rides out network drops

From iPad (A16) simulator shots (`raw/ipad-*.jpg`); the same template and `./render.sh` make both sets.
