# Android Facebook bağlantısını kurma

Bu kurulum geliştirici tarafından bir kez yapılır. Son kullanıcı Facebook'un kendi giriş ekranında
izin verir, profilini veya yönettiği bir Sayfayı, başlığı ve (profil için) kimlerin izleyeceğini seçer.
Yayın anahtarı kopyalamak gerekmez: uygulama her **Yayına geç**'te Facebook'ta yeni bir canlı video
oluşturur ve Facebook'un o videoya verdiği RTMPS adresini doğrudan relay'e verir. Adres hiçbir
yere kaydedilmez.

## Meta uygulaması

1. [developers.facebook.com/apps](https://developers.facebook.com/apps) → **Create app**. Kullanım
   durumu olarak Facebook Login'i ve Sayfa/video yayınlamayı içeren seçeneği al (Meta'nın listesi
   değişebiliyor; gerekirse **Other** → **Business**).
2. **App settings → Basic**: App ID'yi not et. Gizlilik politikası:
   `https://tanerozel.github.io/dji-live-bridge/privacy-policy.html`, veri silme talimatları:
   `https://tanerozel.github.io/dji-live-bridge/privacy-policy.html#deleting-data`, kategori, ikon ve iletişim e-postası.
3. **App settings → Advanced → Security → Client token**: client token'ı not et.
   **Require App Secret** kapalı kalmalı (mobil uygulama app secret taşıyamaz).
4. **Facebook Login → Settings**: *Client OAuth login* açık, *Embedded browser OAuth login* kapalı.
5. **Permissions** (Use cases / App Review → Permissions and features) bölümünde şunları ekle:
   `public_profile`, `publish_video`, `pages_show_list`, `pages_read_engagement`,
   `pages_manage_posts`. Live Video API özelliği de listede varsa ekle.
6. **Add platform → Android**:
   - Package name: **`com.streammydrone.app`**
   - Class name: **`com.streammydrone.app.MainActivity`**
   - Key hash: yüklediğin APK'yı imzalayan sertifikanın hash'i.

20 Ekim 2026'da bu makinenin debug keystore'undan hesaplanan key hash (YOUTUBE_SETUP.md'deki debug
SHA-1 ile aynı sertifika):

```text
S3Lz9h+zeIQ/qB/FO3BUeBOJaW0=
```

Başka bir imza için:

```sh
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
"$JAVA_HOME/bin/keytool" -exportcert -alias androiddebugkey -keystore ~/.android/debug.keystore \
  -storepass android | openssl sha1 -binary | openssl base64
```

Play dağıtımında **Play Console → App integrity → App signing key certificate → SHA-1** değerini
key hash'e çevirip ayrıca ekle:
`echo <SHA-1 iki noktalı> | tr -d : | xxd -r -p | openssl base64`.

## Uygulamaya App ID ve client token vermek

İkisi de uygulamanın içine gömülen, gizli olmayan değerlerdir; yine de repoya yazılmaz.
`~/.gradle/gradle.properties` dosyasına ekle:

```properties
facebookAppId=1234567890
facebookClientToken=0123456789abcdef0123456789abcdef
```

veya ortam değişkeni olarak `FACEBOOK_APP_ID` / `FACEBOOK_CLIENT_TOKEN`. Değerler yoksa uygulama
yine derlenir; Facebook profil ekranı "yapılandırılmamış" hatası gösterir ve SDK hiç başlatılmaz.

## SDK ve gizlilik

`com.facebook.android:facebook-login` yalnızca giriş için kullanılır. Manifest'te otomatik
başlatma, otomatik uygulama olayları, reklam kimliği toplama ve izleme kapalıdır. Erişim token'ını
SDK kendi özel deposunda tutar; uygulama kendi dosyalarına veya loglara token yazmaz. Sayfa
token'ı ve yayın adresi yalnızca bellekte tutulur. Kurtarma için yalnızca kullanıcı/hedef/canlı
video kimlikleri `noBackupFilesDir` içinde saklanır.

## Geliştirme modunda test

Uygulama **Development** modundayken, uygulamada rolü olan hesaplar (admin, developer, tester) tüm
izinleri App Review olmadan kullanabilir. Kendi Facebook hesabın uygulamanın sahibi olduğu için
hemen test edebilirsin. Geliştirme modunda oluşturulan içerik başkalarına görünmeyebilir; ilk
denemede **Sadece ben**'i seç.

Facebook'un canlı yayın şartları API ile aşılamaz: hesap en az 60 günlük olmalı, profil
(profesyonel mod) veya Sayfa en az 100 takipçiye sahip olmalı. Karşılanmazsa uygulama bunu
söyleyen bir hata gösterir (Graph API alt kodu 1363120 / 1363144).

1. Uygulamada Facebook platformunu aç → **Facebook hesabını bağla**.
2. Facebook uygulamasında veya açılan Facebook sayfasında giriş yap, tüm izinleri ver.
3. Profilini veya bir Sayfayı seç; başlık ve **Sadece ben** → Kaydet.
4. DJI görüntüsünü veya test videosunu aç → **Yayına geç**.
5. Servis Facebook'ta yeni bir canlı video oluşturur ve görüntüyü onun adresine gönderir. Durum
   panelinde "Facebook onayladı: canlı" görülmeli; satıra dokunup videoyu Facebook'ta aç.
6. **Yayını bitir** → hem aktarımın durduğunu hem Facebook'taki canlı videonun bittiğini doğrula.
7. Bildirimdeki bitirme düğmesini, uygulama arka plandayken de dene.
8. Facebook'ta Ayarlar → Uygulamalar ve web siteleri'nden uygulamayı kaldır → sonraki denemede
   yeniden bağlanma mesajı görülmeli. **Facebook bağlantısını kaldır** izinleri Facebook tarafında
   da siler ve yerel profili kaldırır.
9. Yayın hazırlanırken bitirme, internet kesilmesi ve uygulamanın zorla kapatılması durumlarını
   dene. Yarım kalan canlı video kimlikleri sonraki servis açılışında temizlenir.

## Herkese açık dağıtım

Uygulamada rolü olmayan kullanıcılar için şunlar gerekir:

- `publish_video`, `pages_show_list`, `pages_read_engagement`, `pages_manage_posts` (ve listede
  varsa Live Video API) için **App Review**: her izin için ne yaptığını gösteren ekran kaydı ve
  açıklama. Ekran kaydında giriş → hedef seçimi → Yayına geç → Facebook'ta canlı video → bitirme
  akışı görünmeli.
- **Business Verification** (Meta çoğu gelişmiş izin için istiyor).
- Uygulamayı **Live** moda almak.

| Hata | Kontrol |
| --- | --- |
| Bağlantı yapılandırılmamış | `facebookAppId` / `facebookClientToken`, Android platformu, key hash |
| Facebook izni gerekli | Reddedilen izin, kaldırılan uygulama erişimi, süresi dolan token |
| Hesabı yeniden bağla | Facebook'ta başka bir kullanıcıyla giriş yapılmış |
| Sayfa kullanılamıyor | Sayfadaki rol veya `pages_*` izinleri |
| Canlı yayına izin yok | 60 gün / 100 takipçi şartı |
| İstek sınırı | Graph API hız sınırları; durum sorgusu 30 sn'de bir yapılır |
| Bitirme doğrulanamıyor | Facebook'ta canlı videoyu elle bitir; kayıt sonraki açılışta yeniden denenir |

Token, yayın adresi ve API yanıt gövdelerini hata raporuna veya logcat'e ekleme.

Resmî kaynaklar: [Live Video API](https://developers.facebook.com/docs/live-video-api/),
[başlangıç](https://developers.facebook.com/documentation/live-video-api/getting-started),
[Android için Facebook Login](https://developers.facebook.com/docs/facebook-login/android),
[Graph API hata kodları](https://developers.facebook.com/docs/graph-api/guides/error-handling).
