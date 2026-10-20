# Google OAuth doğrulaması ve Meta App Review başvuruları

Hazırlandı: 2026-10-20. Amaç: YouTube ve Facebook "hesabı bağla" özelliklerini herkese açmak.
Mağaza sürümlerinde (1.1) ikisi de kapalı: Android `BuildConfig.YOUTUBE_ACCOUNT_CONNECTION` /
`FACEBOOK_ACCOUNT_CONNECTION`, iOS `AccountConnection`. Onay gelince release değerleri `true` yapılır.

Ekran kayıtları **Debug** derlemeyle çekilir (bağlantılar orada açık): Xcode'dan simülatöre veya
iPhone'a, Android Studio'dan telefona. Uygulama dili **English** olmalı (incelemeciler İngilizce
ister); dil düğmesinden veya cihaz dilinden değiştir.

Ortak bilgiler:

- App name: `Stream My Drone`
- Home page: `https://tanerozel.github.io/dji-live-bridge/`
- Privacy policy: `https://tanerozel.github.io/dji-live-bridge/privacy-policy.html`
  (Google bölümü `#google-youtube`, Facebook bölümü `#facebook`)
- Support email: `tanerozel47@gmail.com`
- iOS bundle ID `com.streammydrone.app`, App Store ID `6818233095`; Android package `com.streammydrone.app`

---

## 1. Google OAuth doğrulaması (YouTube)

### Önce kontrol et

1. **Google Auth Platform → Branding:** ad, logo, destek e-postası, home page, privacy policy
   dolu olmalı. **Authorized domains** listesinde `tanerozel.github.io` olmalı ve Google Search
   Console'da bu alan adının sahipliği doğrulanmış olmalı. Sayfa uyarı gösteriyorsa önce bunu
   çöz. `github.io` alt alan adı doğrulamada sorun çıkarırsa kendi alan adına geçmek gerekebilir.
2. **Data Access:** kapsamlar `youtube.force-ssl` (hassas), `userinfo.email`,
   `userinfo.profile`, `openid`. Kullanılmayan kapsam ekleme; Google bunu reddeder.
3. **Audience → Publish app** ("In production"). Hassas kapsam olduğu için **Verification Center**
   doğrulama başvurusu ister. Aşağıdaki metinleri ve videoyu orada ver.

### Scope justification (`youtube.force-ssl`)

```
Stream My Drone relays the live video from a DJI drone (sent by the DJI Fly app over the local
network) to the streaming platforms the user picks. When the user adds YouTube and taps "Sign in
with Google" (iOS) or "Connect YouTube account" (Android), the app uses the YouTube Live Streaming API on the user's own channel to:
- read the user's channel (channels.list, mine=true) to show which channel is connected;
- create a live broadcast with the title, visibility and made-for-kids setting the user chose
  (liveBroadcasts.insert) and a stream with its ingestion address (liveStreams.insert);
- bind them (liveBroadcasts.bind) and send the drone video to that address;
- check the broadcast status while it runs (liveBroadcasts.list, liveStreams.list) and end it
  when the user taps End broadcast (liveBroadcasts.transition to complete);
- delete the broadcast and stream it created if a start is cancelled before going live
  (liveBroadcasts.delete, liveStreams.delete), so no unused broadcasts are left behind.
These calls need write access to live broadcasts, which youtube.force-ssl provides; the
narrower read-only scopes cannot create or end broadcasts. The app does not upload videos, read
comments or analytics, or modify anything other than the broadcasts the user starts.
Tokens stay on the device (Google Play services on Android, Keychain on iOS); there is no
developer server and no data is shared, sold or used for advertising or AI training.
```

### Demo videosu (YouTube'a "Liste dışı" yükle, linki forma koy)

Tek kesintisiz kayıt, İngilizce arayüz, 3-5 dakika. iPhone simülatöründe Debug derleme yeterli.

1. Uygulamayı aç; adı ve simgesi görünsün.
2. YouTube platformunu ekle → **Sign in with Google** (iOS'ta Google'ın düğmesi; Android'de
   "Connect YouTube account").
3. Google hesap seçici ve **onay ekranı**: "Stream My Drone" adı ve YouTube izni okunabilir
   olsun; ekranı kaydırıp tüm izin metnini göster. Mümkünse adres çubuğuna dokunup `client_id`
   parametresini göster.
4. Uygulamaya dön: bağlanan kanal adı, **Visibility**, **Made for kids** seçimi → kaydet.
5. **Test video** ile bir video aç → **Go live** → başlığı onayla.
6. YouTube Studio'ya geç: aynı başlıkla oluşan canlı yayını göster.
7. Uygulamada **End broadcast** → YouTube Studio'da yayının bittiğini göster.
8. Platformu aç → **Disconnect YouTube account** → ardından
   `myaccount.google.com/permissions` sayfasında uygulamanın erişiminin kalktığını göster.

### Ayrıca (isteğe bağlı ama önerilir): YouTube API Services denetimi

Tüm kullanıcılar Cloud projesinin günlük kotasını (varsayılan 10.000 birim) paylaşır ve
denetlenmemiş projelerde yayınlar private kalabilir. Doğrulama başvurusundan sonra
`https://support.google.com/youtube/contact/yt_api_form` üzerinden denetim/kota formunu doldur.
Aynı demo videosu kullanılabilir.

---

## 2. Meta App Review (Facebook)

### Önce kontrol et

1. **Business Verification** (Meta Business Suite → Security Center). Gelişmiş erişim için şart;
   genellikle birkaç gün sürer, ilk bunu başlat.
2. **App settings → Basic:** privacy policy URL, app icon (1024×1024), category, **User data
   deletion** = "Data deletion instructions URL" →
   `https://tanerozel.github.io/dji-live-bridge/privacy-policy.html` (bölüm 6).
   Platformlar: iOS (bundle ID) ve Android (package).
3. **Data Use Checkup** bekliyorsa tamamla.
4. **App Review → Permissions and features:** şu izinler için **Advanced Access** iste:
   `publish_video`, `pages_show_list`, `pages_read_engagement`, `pages_manage_posts`.
   Live Video API listede ayrı görünüyorsa onu da ekle.

### İncelemeciye verilecek derleme

Mağaza sürümünde Facebook kapalı olduğu için **iOS simülatör derlemesi** ver (bağlantılar açık):
`StreamMyDrone-1.1-simulator.zip` (Debug, `xcodebuild -sdk iphonesimulator`, `.app` zip'lenmiş).
Android'de Facebook uçtan uca henüz denenmedi; başvuruyu iOS ile yap.

### Testing instructions (forma yapıştır)

```
Stream My Drone relays a DJI drone's live video to streaming platforms. Reviewers do not need a
drone: the app can stream a test video instead.

1. Install the attached simulator build: unzip it and run
   xcrun simctl install booted StreamMyDrone.app
   (any iPhone simulator on iOS 17 or later), then open Stream My Drone.
2. Drag any short MP4 file onto the simulator window to add it to Photos.
3. Tap the Facebook platform tile, then "Connect Facebook account" and log in with your test
   user. Grant all requested permissions.
4. Under "Broadcast to" choose "Your profile" (audience "Only me" is fine) or a Page you
   manage, enter a title and save.
5. Tap "Test video", pick the video from step 2, then tap "Go live" and confirm the title.
   The app creates a live video on Facebook and sends the test video to it. The status panel
   shows "Facebook confirmed: live"; tapping it opens the live video on Facebook.
6. Tap "End broadcast": the live video ends on Facebook.
7. Open the Facebook platform again and tap "Disconnect Facebook account" to remove the app's
   access.
```

### İzin başına açıklamalar

**publish_video**

```
When the user connects their Facebook account and taps Go live, the app creates a live video on
the user's own profile (POST /me/live_videos, status LIVE_NOW) with the title and audience the
user chose, sends the drone's video to the returned secure_stream_url, reads the live video's
status to show whether Facebook reports it as live, and ends it when the user taps End broadcast
(a live video that never started is deleted). Disconnecting removes the app's access
(DELETE /me/permissions). The app publishes nothing other than live videos the user starts. The stream address is kept
in memory only.
```

**pages_show_list**

```
After login the user picks where to broadcast: their profile or one of the Pages they manage.
pages_show_list is used to list those Pages (GET /me/accounts) so the user can choose one.
```

**pages_read_engagement**

```
Required together with pages_show_list to read the selected Page's name, ID and Page access
token, which the app needs to create a live video as that Page. The app does not read posts,
comments, followers or insights. The Page token is fetched when needed and never stored.
```

**pages_manage_posts**

```
Used to create, check and end a live video on the Page the user selected (POST
/{page-id}/live_videos with the Page token, then reading and ending that live video). Page live
videos are published only when the user taps Go live, and end when they tap End broadcast.
```

### Ekran kaydı (izin başına bir video veya hepsini kapsayan tek video)

1. Uygulamayı aç (İngilizce) → Facebook → **Connect Facebook account**.
2. Facebook giriş ve izin ekranı: istenen izinlerin listesi görünsün.
3. **Broadcast to** → önce **Your profile** göster, sonra **Page** listesini aç (pages_show_list)
   ve bir Sayfa seç; başlık gir → kaydet.
4. **Test video** → **Go live** → durum panelinde "Facebook confirmed: live".
5. Facebook'ta Sayfada (veya profilde) canlı videoyu göster (publish_video / pages_manage_posts).
6. **End broadcast** → Facebook'ta yayının bittiğini göster.
7. **Disconnect Facebook account**.

Not: Facebook canlı yayın için hesabın 60 günlük ve profil/Sayfanın 100 takipçili olmasını
ister; kaydı bu şartı karşılayan bir hesapla/Sayfayla çek.
