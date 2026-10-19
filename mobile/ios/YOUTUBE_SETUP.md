# YouTube account connection on iPhone and iPad

The app uses Google's native Google Sign-In SDK (Swift Package Manager, pinned to 10.0.0)
with direct HTTPS YouTube Data API v3 calls. There is no developer backend, API key,
web client secret, embedded browser, or manually copied YouTube key in this flow.

## Configured Google Cloud project

- Project: `streammydrone` (number `976687442774`). YouTube Data API v3 enabled.
- iOS OAuth client: `Stream My Drone iOS`, created October 15, 2026.
- Client ID: `976687442774-07f5kicfkavd31lvhr1jb0jbk72efngd.apps.googleusercontent.com`
- Bundle ID: `com.streammydrone.app`
- Apple team: `GXDXLCQ92M`; App Store ID: `6818233095`.
- `StreamMyDrone-Info.plist` contains `GIDClientID` and the reversed-client-ID URL scheme.
  `StreamMyDroneApp.onOpenURL` passes that callback to the SDK.
- OAuth consent remains **External / Testing**, with `tanerozel561@gmail.com` as a test user.
  The Cloud admin account and the test account are different. Add other test users in
  Google Auth Platform → Audience before trying them.
- Declared scopes: `youtube.force-ssl`, `userinfo.email`, `userinfo.profile`, `openid`.
  iOS requests the YouTube scope alongside the SDK's standard basic profile scopes.
  No phone-number scope is requested; the app stores only the email from Google's profile.

The Android OAuth client/JSON is not used on iOS. A native iOS OAuth client ID is public
application configuration, not a client secret. The package resolution file pins transitive
SDK dependencies as well.

## User flow

1. Add YouTube, then use Google's sign-in button and grant YouTube permission.
2. Choose a returned channel, a title, visibility (**Unlisted** initially), and explicitly
   choose whether the broadcast is made for kids. Save creates a reusable streaming input
   and stores its RTMPS key in Keychain. Saving does not create a broadcast.
3. Go live creates a new broadcast with those settings, enables automatic start/stop,
   saves its recovery ID before binding, binds the saved input, then attaches the native relay.
4. End live stops video forwarding and requests completion. An unstarted event is deleted;
   a completed/live video is never deleted. Other selected platforms continue independently.
5. Disconnect finishes outstanding cleanup, revokes the account's authorization, removes
   SDK credentials and deletes this local profile/key. Completed videos and reusable remote
   inputs remain in YouTube Studio. Connections sharing that Google account may need consent again.

Existing manual YouTube profiles keep the manual editor. A new YouTube profile can use
“Enter a stream key instead”. Instagram is prepared by the app in its own Live Producer page
(`StreamMyDrone/Instagram`, the same scripts as the Android app); the other platforms take a key.

The SDK has one current Google session. Connecting a different Google account replaces the
active SDK identity; saved connections for another account require reconnecting that account.
The app checks both account email and authorized channel before remote lifecycle operations.
YouTube connection editing is blocked during an active/preparing YouTube broadcast.

## Storage and interruption behavior

- Stream keys: separate iOS Keychain items, `WhenUnlockedThisDeviceOnly`, no synchronization.
- OAuth: SDK-owned authorization state/refresh credentials in Keychain; the app copies only an
  access token into a short-lived transport in memory. No tokens in profile/session files or logs.
- Public settings and recovery IDs: atomic Application Support files excluded from backup,
  protected until first unlock. Failed cleanup records remain for retry on the next launch
  or before the next broadcast for that profile.
- Network lifecycle work runs in a serialized async lane, independently of native relay work.
  Each attempt has an owner UUID: a late stop from an old attempt cannot finish a new event.
  Stop during creation waits for the response/ID persistence, then cleans up without attaching video.
- iOS background execution still requires Picture in Picture. Without it the existing grace
  period ends forwarding; a completion request is best effort at suspension. Automatic stop
  and the durable recovery record cover interrupted cleanup. Check YouTube Studio if reported.
- API calls use ephemeral HTTPS sessions with bearer headers, no redirects/cookies/cache,
  bounded response size and timeouts. Error messages expose no raw API responses or credentials.

## Validation and release requirements

Simulator unit tests cover payload settings, input validation, API error sanitization,
RTMPS-only input handling, bind failure compensation, idempotent completion, recovery,
wrong-channel refusal, stop during creation, old-owner isolation, profile migration and
key/token exclusion from metadata files. They use a fake YouTube transport, not a Google account.

Before distributing this build:

- Run on a signed physical iPhone/iPad with a listed test account. Complete Google's consent
  yourself; test key acquisition, real video, YouTube auto-start, stop and replay visibility.
- Test revoked permission, account switching, first-time live activation, channel restrictions,
  quota/network errors, interrupted preparation, relaunch recovery, Picture in Picture and lock.
- YouTube channel must have live streaming enabled and satisfy YouTube's current requirements.
  API project restrictions may make new broadcasts private until the relevant YouTube audit.
- Publish the updated privacy-policy source (`site/privacy-policy.html`, generated with
  `npm run build:site`) before distributing iOS account connection. The policy explains the
  SDK's persisted credentials and its bundled privacy disclosures.
- Reconcile App Store Connect privacy answers with the actual SDK manifests in the release
  archive. Do not retain a blanket “Data Not Collected” answer without reviewing Google SDK data.
  The app's own manifest describes its own APIs; dependency privacy manifests are bundled too.
- External production use requires the appropriate Google OAuth sensitive-scope verification;
  this setup does not publish the OAuth app, ship an App Store binary, or perform that review.

Official references:
[Google iOS setup](https://developers.google.com/identity/sign-in/ios/start-integrating),
[API access and token refresh](https://developers.google.com/identity/sign-in/ios/api-access),
[SDK App Store data disclosure](https://developers.google.com/identity/sign-in/ios/app-privacy),
[GoogleSignIn 10.0.0](https://github.com/google/GoogleSignIn-iOS/releases/tag/10.0.0).
