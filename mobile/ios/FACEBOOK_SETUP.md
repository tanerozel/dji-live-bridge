# Facebook account connection on iPhone and iPad

The Meta app, permissions, App Review and the 60-day / 100-follower live rule are the same as on
Android: see [the Android guide](../android/FACEBOOK_SETUP.md) (Turkish). This file covers what
differs on iOS.

## No Facebook SDK

Without App Tracking Transparency consent the Facebook iOS SDK only performs Limited Login, which
returns no Graph API access token, so it cannot create live videos. The app declares no tracking,
so it does not use the SDK. `FacebookAuthorization` opens Facebook's own OAuth dialog in
`ASWebAuthenticationSession` (shares Safari's Facebook login), with
`redirect_uri=fb<App ID>://authorize/`, `response_type=token,granted_scopes` and a random `state`.
The token is checked against `GET /me` and kept in one Keychain item
(`AfterFirstUnlockThisDeviceOnly`, not synchronized), so a broadcast can still be ended while the
phone is locked in Picture in Picture. No URL type needs registering: the session catches the
`fb<App ID>` scheme itself.

**To verify on the first real login** (not testable in unit tests): that Facebook accepts the
`fb<App ID>://authorize/` redirect for this app's iOS platform, and how long the returned token
lasts (`expires_in`). If the token is short-lived, the app asks to reconnect once it expires.

## Meta app settings for iOS

- **Add platform → iOS**: Bundle ID `com.streammydrone.app`.
- **Facebook Login → Settings**: *Client OAuth login* on. If Facebook rejects the redirect, add
  `fb<App ID>://authorize/` to *Valid OAuth Redirect URIs*.
- Put the App ID in `StreamMyDrone-Info.plist` under `FacebookAppID` (public, like `GIDClientID`).
  Empty: the editor reports that Facebook is not configured.

## Flow and storage

- Connect → Facebook login → profile or Page, title, and for the profile an explicit audience
  (Only me, Friends, Public) → Save. The profile stores only public settings; no stream key.
- Go live: `FacebookBroadcastManager` (one ordered lane, owner UUID per attempt, like YouTube)
  creates a `LIVE_NOW` live video, saves its ID in `facebook-sessions-v1.json` (Application
  Support, excluded from backup) and hands Facebook's RTMPS URL to the relay in memory.
- End live / failure / stop ends the live video (deletes it if it never started). Failed
  cleanup stays recorded and is retried at the next launch.
- The status row polls every 30 seconds while the app is on screen and says live only when
  Facebook reports `LIVE`. Only facebook.com permalinks are opened.
- Disconnect ends any recorded live video, calls `DELETE /me/permissions`, deletes the Keychain
  item and the profile.
