# Stream My Drone - Google Play release checklist

Last updated: 2026-09-28

This file records the Android release decisions and progress. Never add passwords,
private keys, recovery codes, or other credentials to this file.

## Application identity

- Play Store application name: `Stream My Drone`
- Package name: `com.streammydrone.app`
- Default store-listing language: English (United States), `en-US`
- Product type: Application
- Pricing: Paid application, one-time upfront Play Store purchase
- Subscription: None
- In-app one-time product: None; purchase is the app itself

## Android developer verification

- Friendly package name: `Stream My Drone`
- Package registration status: Verified
- Signing-key alias: `streammydrone-release`
- Keystore location (local only):
  `~/Library/Application Support/Stream My Drone/keys/streammydrone-release.jks`
- Certificate SHA-256 fingerprint:
  `85:19:CD:31:DB:05:B2:ED:97:52:89:A5:DD:25:B5:79:B4:C5:3E:EC:A3:C1:1C:C1:B8:CE:A7:72:6B:D0:96:6F`
- Certificate algorithm: 2048-bit RSA
- Certificate validity: 2026-09-25 through 2054-02-10
- Keystore backup: Pending confirmation

## Play Console application creation

- [x] Enter application name: `Stream My Drone`
- [x] Enter package name: `com.streammydrone.app`
- [x] Select default language: English (United States), `en-US`
- [x] Select product type: Application
- [x] Select pricing: Paid
- [x] Accept the Developer Program Policies declaration
- [x] Accept the US export laws declaration
- [x] Accept the Play App Signing terms
- [x] Create the Play Console application

## Initial Play Console setup

- Progress reported in Play Console: 12 of 13 tasks complete
- Remaining initial setup task: Default store listing (`en-US`)
- Store copy source: `mobile/android/PLAY_STORE_LISTING_EN_US.md`
- Translation import source: `mobile/android/PLAY_STORE_TRANSLATIONS.txt`

## Remaining release work

- [ ] Configure local release signing without committing credentials
- [x] Produce and verify a signed Android App Bundle (`.aab`): `0.1.0` / versionCode `1`, built
  2026-09-28 with `scripts/build-release.sh`; certificate SHA-256 matches the registered key;
  AAB SHA-256 `a03d5ee97a649f677ff8f3eceff34e5355b1ff8336f0eac2c6318320fcd6a0b2`
- [x] Create or verify the Google Play payments profile
- [ ] Verify payout bank account, public merchant details, and required tax information
- [ ] Set the paid app's base price and review country-specific prices
- [ ] Complete paid-app tax and compliance settings
- [x] Add a public privacy-policy URL
- [x] Complete the initial Data safety declaration
- [ ] Complete the release-specific foreground-service declaration
- [x] Complete content rating, target audience, ads, and app-access forms
- [x] Save the `en-US` app name, short description, and full description
- [x] Upload the `512 x 512 px` Play Store app icon
- [x] Prepare six Play-compatible phone screenshots at `1080 x 2160 px`
- [ ] Upload the six prepared phone screenshots in the documented order
- [x] Prepare the `1024 x 500 px` feature graphic at
  `mobile/android/store-assets/feature-graphic/stream-my-drone-feature-graphic-1024x500.png`
- [ ] Upload the prepared feature graphic
- [ ] Upload to an internal or closed testing track
- [ ] Keep Google Play automatic installer protection enabled
- [ ] Test the Play-protected build from a test track on a physical device
- [ ] Review the Play pre-launch report
- [ ] Complete a long RC 2 to phone to real RTMPS destination field test
- [ ] Promote to production only after the release checks pass
