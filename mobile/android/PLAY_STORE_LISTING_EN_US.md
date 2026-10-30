# Google Play store listing - English (United States)

Last updated: 2026-10-30

## App name

```text
Stream My Drone
```

## Short description

66 characters out of the 80-character limit.

```text
Stream DJI Fly video live to multiple RTMP and RTMPS destinations.
```

## Full description

```text
Stream your drone footage live—directly from DJI Fly to the platforms you choose.

Stream My Drone turns your Android phone into a local RTMP bridge. It receives the Custom RTMP feed from a compatible DJI controller, displays a live preview, and forwards the same H.264/AAC stream to one or more RTMP or RTMPS destinations.

HOW IT WORKS
1. Connect your phone and DJI controller to the same trusted Wi-Fi network, or use your phone's hotspot.
2. Enter the RTMP address shown by the app in DJI Fly.
3. Connect your YouTube or Instagram account, or add a server address and stream key for any other platform.
4. Preview the feed, choose your platforms, and tap Go live.

KEY FEATURES
• Connect your YouTube account once: each broadcast is created with your title and visibility, with no stream key to copy
• Connect Instagram inside the app: the stream key is prepared for every broadcast
• Talk over your broadcast with your phone's microphone; your voice replaces the drone's sound
• Virtual Bluetooth microphone for DJI RC 2, so DJI Fly recognises a microphone
• Send one live feed to multiple destinations at the same time
• Profiles for YouTube, Instagram, TikTok, Facebook, Twitch, Kick, and custom RTMP servers
• Secure RTMPS support with certificate and hostname validation
• Live preview with resolution, bitrate, codec, packet, and connection details
• Automatic destination reconnection after temporary network interruptions
• Test-video mode to check the complete streaming path before flying
• Continue broadcasting while using another app or locking the screen, subject to Android system limits
• Interface available in 15 languages
• Light, dark, midnight blue, sand, and system themes

PRIVACY AND SECURITY
Stream keys are encrypted on the device using Android Keystore. They are not written to logs, cloud backups, or our servers. Stream My Drone does not run a developer-operated relay server; video is sent from your phone directly to the destinations you configure. RTMP is unencrypted, so use RTMPS whenever your platform supports it. Account connections are optional; the app talks directly to Google, YouTube and Instagram, and no account data reaches a developer server.

BEFORE YOU BUY
• Live drone use requires a compatible DJI controller or app with Custom RTMP publishing
• Instagram connection needs an account with access to Instagram Live Producer
• Each platform may require account eligibility and may issue a new stream key for each broadcast
• Upload bandwidth is used separately for every active destination
• Use only a trusted local network for the DJI-to-phone connection
• Stream My Drone is a streaming bridge, not a drone flight-control app

One-time purchase. No subscription.

Stream My Drone is an independent product and is not affiliated with, endorsed by, or sponsored by DJI or any listed streaming platform. DJI and the platform names are trademarks of their respective owners.
```

## Graphic assets

- App icon: `branding/streammydrone-play-store-512.png`
- Feature graphic: prepared at `1024 x 500 px` (24-bit PNG, no alpha), matching the promo
  screenshots and rendered by `store-assets/promo-screenshots/source/render.sh`:
  `mobile/android/store-assets/feature-graphic/stream-my-drone-feature-graphic-en-US-1024x500.png`
  (Turkish listing: `...-tr-TR-1024x500.png`). The earlier illustration is kept as
  `stream-my-drone-feature-graphic-1024x500.png`.
- Phone screenshots: prepared at `1080 x 2160 px` under
  `mobile/android/store-assets/phone-screenshots/`. The Android status and navigation
  bars were removed with a centered, pixel-preserving crop.

Recommended phone screenshot order:

1. `Screenshot_20260928_185359_Stream My Drone.jpg` - connect the drone and copy the local RTMP address
2. `Screenshot_20260928_185437_Stream My Drone.jpg` - clean preview in the Ready state
3. `Screenshot_20260928_185549_Stream My Drone.jpg` - clean preview while Live
4. `Screenshot_20260928_185542_Stream My Drone.jpg` - live Instagram guidance

Upload-ready files, in order:

1. `phone-screenshots/01-connect-drone.jpg`
2. `phone-screenshots/02-ready-preview.jpg`
3. `phone-screenshots/03-live-broadcast.jpg`
4. `phone-screenshots/04-platform-guidance.jpg`

Additional prepared alternatives:

- `phone-screenshots/05-reconnecting.jpg`
- `phone-screenshots/06-ready-hotspot.jpg`

Promotional screenshots (uploaded 2026-10-30): `1080 x 1920 px` JPEG, no alpha, with a
headline above a device frame. Generated from `store-assets/promo-screenshots/source/`
(`template.html` holds the captions for every language; re-render with `./render.sh`).
The raw shots in `source/raw/` are 1080x2400 emulator captures of a debug build (FLAG_SECURE
removed only for that build) cropped to 1080x2280, with the drone clip published over RTMP
and the destinations pointed at a local MediaMTX.

1. `promo-screenshots/en-US/01-live-hero.jpg` - Take your drone live
2. `promo-screenshots/en-US/02-connect-drone.jpg` - Connect in seconds
3. `promo-screenshots/en-US/03-youtube.jpg` - YouTube in one tap
4. `promo-screenshots/en-US/04-instagram.jpg` - Instagram, no key to copy
5. `promo-screenshots/en-US/05-phone-mic.jpg` - Talk over your broadcast
6. `promo-screenshots/en-US/06-multistream.jpg` - One feed, many platforms
7. `promo-screenshots/en-US/07-preview.jpg` - Check it before you go live
8. `promo-screenshots/en-US/08-reconnect.jpg` - Rides out network drops

The same set with Turkish captions is in `promo-screenshots/tr-TR/` for the `tr-TR` listing.
Play's upload panel finishes several files in random order and the last tile cannot be
dragged, so upload the files one at a time, in order.
