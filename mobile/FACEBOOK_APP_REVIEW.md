# Meta App Review — Stream My Drone (App ID 1140011221710660)

Answers used in the App Review submission (2026-11-12). Business: InnSol Bilgi Teknolojileri A.Ş.,
Business Verification approved 2026-11-11; the app was marked as a Tech Provider the same day.
Requested: `publish_video`, `pages_show_list`, `pages_read_engagement`, `pages_manage_posts`,
`public_profile`, Live Video API. `business_management` was removed from the submission (never called).

## Screencast (one recording, reused for every permission)

Record on a phone with English UI, 1–3 minutes, no cuts:

1. Open Stream My Drone → enable **Facebook** → **Connect Facebook account**.
2. Facebook Login for Business opens: log in, the permission screen shows all four permissions → continue.
3. Back in the app: the destination list shows **your profile** and your **Pages** (pages_show_list).
4. Pick the profile, audience **Only me**, enter a title → Save.
5. Tap **No drone? Try a test video** (or connect DJI Fly) → **Go live**.
6. The status panel shows "Facebook confirmed: live"; tap it → the live video opens on Facebook.
7. **End broadcast** → the live video on Facebook shows as ended.
8. Repeat 4–7 with a **Page** as the destination (pages_read_engagement + pages_manage_posts).
9. Optional: **Disconnect Facebook** to show the permissions being removed.

## publish_video

```text
Stream My Drone forwards the live video of a DJI drone (received from the DJI Fly app over the local network) to streaming platforms. When the user chooses Facebook and taps "Go live", the app uses publish_video to create a live video on the user's own timeline (POST /{user-id}/live_videos with status=LIVE_NOW, the title the user typed and the audience the user picked) and streams the drone video to the secure_stream_url that Facebook returns. While live, the app reads the live video's status (GET /{live-video-id}?fields=status,permalink_url) to show "Facebook confirmed: live" and a link to the video. When the user taps "End broadcast" the app ends the live video (POST /{live-video-id} end_live_video=true). The app never posts anything else and never publishes without the user tapping "Go live". The stream URL is kept in memory only and never stored or sent to our servers; we have no servers.
```

## Live Video API

```text
The Live Video API is the core of the Facebook destination in Stream My Drone. For every broadcast the user starts, the app creates one live video on the destination the user chose (their profile, or a Page they manage) with POST /{target-id}/live_videos (status=LIVE_NOW), sends the drone's H.264/AAC stream to the returned secure_stream_url over RTMPS, polls GET /{live-video-id}?fields=status,permalink_url every 30 seconds to show the broadcast state in the app, and ends the live video when the user taps "End broadcast". Nothing is created without an explicit "Go live" tap, and only one live video exists per broadcast.
```

## pages_show_list

```text
After a user connects Facebook, Stream My Drone lists the Pages the user manages (GET /me/accounts?fields=id,name,tasks) so the user can choose whether the drone broadcast goes to their own profile or to one of their Pages. Only the Page name and id are shown in the destination picker; the list is not stored or sent anywhere other than Facebook.
```

## pages_read_engagement

```text
When the user picks one of their Pages as the destination, Stream My Drone needs the Page access token to create the live video on that Page (GET /{page-id}?fields=access_token). The same token is used to read the status and permalink of the live video the app just created (GET /{live-video-id}?fields=status,permalink_url) so the app can show "Facebook confirmed: live" and open the broadcast on Facebook. The app does not read the Page's posts, comments, followers or insights.
```

## pages_manage_posts

```text
When the user chooses one of their Pages as the destination and taps "Go live", Stream My Drone creates a live video post on that Page (POST /{page-id}/live_videos with status=LIVE_NOW and the title the user typed) using the Page access token, streams the drone video to it, and ends that live video when the user taps "End broadcast". The app only creates and ends the live video for the broadcast the user started; it never edits or deletes other Page posts.
```

## public_profile

```text
Used to show which Facebook account is connected (name) and to make sure the same account is still signed in before each broadcast (GET /me?fields=id,name), so a live video is never created on someone else's account.
```

## Reviewer instructions

```text
Stream My Drone is a mobile app (iOS and Android) that receives the live RTMP video from the DJI Fly app on a DJI drone remote controller and forwards it to streaming platforms, including Facebook Live.

Important: until this review is approved, the Facebook connection is switched off in the public App Store / Google Play builds (they only accept a manually pasted stream key), because the permissions only work for app roles. The attached screencast was recorded with our test build, which is identical except for that switch. We can send an Android APK of the test build on request; it will become the store build once the permissions are approved.

You do not need a drone. To test:
1. Install the app, open it and tap "Skip" on the short guide.
2. Tap the Facebook tile, then "Connect Facebook account" and log in with your test Facebook user. Grant the requested permissions.
3. Choose your profile (audience "Only me") or a Page you manage, enter a title and save.
4. On the connect card, tap "No drone? Try a test video" and pick any video from the photo library; the app feeds it into its own receiver exactly as DJI Fly would.
5. Tap "Go live". The app creates a live video on Facebook and streams the test video to it; the status panel shows "Facebook confirmed: live". Tap it to open the video on Facebook.
6. Tap "End broadcast" to end the live video.

Facebook only allows going live from accounts older than 60 days, and from Pages/professional profiles that meet Facebook's follower requirement; otherwise Facebook returns an error that the app shows.
The app has no account and no server; tokens are stored only in the device's secure storage (iOS Keychain / Android Keystore).
```

## Data handling

- Data processors / third parties with access to Platform Data: **none** (no server, no analytics).
- Responsible entity: InnSol Bilgi Teknolojileri A.Ş.
- Requests from public authorities in the last 12 months: **no**.
- Policies for such requests: we hold no Platform Data off-device, so there is nothing to disclose.
