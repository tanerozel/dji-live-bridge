# Android YouTube bağlantısını kurma

Bu kurulum geliştirici tarafından bir kez yapılır. Son kullanıcı Google hesabını seçer, izin
verir, yayın başlığını, görünürlüğünü ve çocuklara özel içerik seçimini yapar. Bilgisayardan
yayın anahtarı kopyalaması gerekmez.

## Google Cloud

1. [Google Cloud Console](https://console.cloud.google.com/) üzerinde Stream My Drone için bir proje oluştur.
2. API Library bölümünde **YouTube Data API v3** hizmetini etkinleştir. Live Streaming API bu
   hizmetin parçasıdır; ayrı bir canlı yayın API anahtarı yoktur.
3. Google Auth Platform → Branding bölümünde uygulama adı, destek e-postası, geliştirici iletişim
   adresi, uygulama sitesi, gizlilik politikası ve kullanım koşullarını doldur. Google'ın istediği
   alan adı doğrulamasını tamamla.
4. Audience bölümünde **External** seç. İlk denemede **Testing** durumunda kal; kullanacağın
   Google hesabını **Test users** listesine ekle.
5. Data Access bölümüne şu kapsamları ekle:
   - `https://www.googleapis.com/auth/youtube.force-ssl`: video girdisi ve canlı yayın oluşturma,
     bağlama ve bitirme. Google izin ekranı bu kapsamın desteklediği geniş YouTube yetkilerini
     gösterir; uygulama bunları bu yayın akışı için kullanır.
   - `https://www.googleapis.com/auth/userinfo.email`: servis yeniden yetkilendirilirken aynı
     Google hesabını seçmek için hesap e-postası. Servis ayrıca kanal kimliğini doğrular;
     başka kanalda yayın açmaz.

Bu entegrasyon Firebase, bir API anahtarı, google-services.json, Web OAuth istemcisi veya
uygulamaya gömülecek client secret kullanmaz. Android istemcisi paket adı ve imza ile tanınır.
Google Play services kısa ömürlü erişim tokenlarını yönetir; uygulama refresh token istemez.

## Android OAuth istemcisi ve imza

Google Auth Platform → Clients → Create client → **Android**:

- Paket adı: **`com.streammydrone.app`**
- SHA-1: telefona yüklediğin APK'yı imzalayan sertifikanın parmak izi.

Yerel debug imzası:

```sh
cd mobile/android
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" ./gradlew signingReport
```

`Variant: debug` altındaki SHA-1'i kopyala ve bu imza için bir Android OAuth istemcisi oluştur.
Debug keystore her geliştiricide farklı olabilir; başka bilgisayarın SHA-1'ini kullanma.

15 Ekim 2026 tarihinde bu geliştirme makinesinde signingReport ile doğrulanan debug SHA-1:

```text
4B:72:F3:F6:1F:B3:78:84:3F:A8:1F:C5:3B:70:54:78:13:89:69:6D
```

Bu yalnızca bu makinenin debug APK'sı içindir; Play dağıtım imzası değildir.

Play dağıtımında **Play Console → App integrity → App signing → App signing key certificate →
SHA-1** kullanılmalı. Bu, upload key ile aynı olmak zorunda değildir. Debug, yerel release ve
Play imzaları farklıysa aynı Cloud projesinde her paket/imza çifti için ayrı Android istemcisi
oluştur. Özel anahtar, keystore parolası veya client secret repoya eklenmez.

30 Ekim 2026'da Play App Signing anahtarı için ayrı bir Android istemcisi oluşturuldu
(**Stream My Drone (Google Play)**):

```text
66:FE:33:BA:B3:E3:53:13:4D:CF:A7:FD:F1:38:C1:15:4E:B7:74:D2
```

Upload key ile imzalanıp Play dışında kurulan release APK'larında YouTube bağlantısı bu yüzden
çalışmaz; o imza için de istemci gerekirse ayrıca eklenmelidir.

## Kanal ve fiziksel cihaz testi

- Test hesabının YouTube kanalı olmalı; doğrulaması ve canlı yayın yetkisi açık olmalı.
  Hesaptaki kısıtlar API üzerinden aşılamaz. İlk etkinleştirme hemen kullanılabilir olmayabilir.
- Google Play services bulunan bir Android telefon kullan.
- İlk denemede **Liste dışı** veya **Gizli** seç. Kitle seçimi kullanıcı cevaplamadan kaydedilmez.
- Bir Google hesabındaki farklı/Marka Hesabı kanallarında, bağlantı ekranındaki kanalı kontrol et.
  Birden fazla kanal dönerse form seçim sunar; listelenmeyen kanala erişim varsaymaz. Marka
  Hesabı seçimi fiziksel cihazda ayrıca doğrulanmalıdır.

1. Uygulamada YouTube platformunu aç → **YouTube hesabını bağla**.
2. Test kullanıcısı olarak eklenen hesabı seç ve izin ver.
3. Kanalı kontrol et; başlık, görünürlük ve kitleyi seç → Kaydet.
4. DJI görüntüsünü veya uygulamanın test videosunu aç → **Yayına geç**.
5. Servis yeni bir YouTube yayını oluşturur ve kayıtlı video girdisine bağlar. Otomatik başlama
   açık olduğu için YouTube görüntüyü aldığında yayına geçer. Uygulamadaki RTMP bağlantı durumu,
   YouTube'un izleyiciye açık yayın durumunun bağımsız kanıtı değildir; izleme sayfasını da açıp
   görüntü ve sesi kontrol et.
6. **Yayını bitir** → hem görüntü aktarımının durduğunu hem YouTube yayınının bittiğini doğrula.
7. Uygulama arka plandayken bildirimdeki bitirme düğmesini de dene.
8. Google hesabı iznini kaldır → sonraki denemede yeniden bağlanma mesajı görülmeli.
   **YouTube bağlantısını kaldır**, Google yetkisini iptal eder ve yerel profili siler.
9. Yayın hazırlanırken bitirme, internet kesilmesi, yayın yetkisi olmayan kanal ve uygulamanın
   zorla kapatılması durumlarını dene. Yarım kalan yayın kimlikleri özel depoda tutulur;
   sonraki servis açılışında veya aynı profil yeniden başlatılırken temizlenir.

Başarısız kapatmada uygulama YouTube Studio'dan kontrol etmeni ister. Otomatik durdurma da
etkindir; bağlantı kaybından sonra YouTube'un kapatması anlık olmayabilir. Gizli/liste dışı test
yayınları kanalda kayıt bırakabilir; test sonunda Studio'dan kontrol et.

## Herkese açık dağıtım

Testing ve test kullanıcıları geliştirme içindir. Dağıtımdan önce Google Auth Platform üzerinden
istenen OAuth doğrulamasını tamamla, marka/gizlilik sayfalarının erişilebilir olduğunu kontrol et
ve Audience ayarını üretime geçir. API uyumluluk/kota incelemesi ile OAuth doğrulaması farklı
süreçlerdir. Kotayı Google Cloud Console'dan izle.

| Hata | Kontrol |
| --- | --- |
| Bağlantı yapılandırılmamış | APK'nın paket adı/SHA-1'i, Android OAuth istemcisi, etkin API |
| Yeniden bağlanma isteniyor | İptal edilen izin, eksik kapsam veya kaldırılan Google erişimi |
| Kanal bulunamıyor/uyuşmuyor | Yetkilendirilen hesap ve dönen kanal kimliği |
| Canlı yayın etkin değil | Kanal doğrulaması, yayın yetkisi ve kısıtları |
| İstek sınırı | Google Cloud YouTube API kotası |
| Kapanma doğrulanamıyor | YouTube Studio; kayıtlı oturum sonraki bağlantıda yeniden denenir |

Token, yayın anahtarı ve API yanıt gövdelerini hata raporuna veya logcat'e ekleme.

Resmi kaynaklar: [Android yetkilendirme ve Cloud kurulumu](https://developer.android.com/identity/authorization),
[video girdileri ve RTMPS](https://developers.google.com/youtube/v3/live/docs/liveStreams),
[yayın oluşturma](https://developers.google.com/youtube/v3/live/docs/liveBroadcasts/insert),
[yayın durumunu değiştirme](https://developers.google.com/youtube/v3/live/docs/liveBroadcasts/transition),
[canlı yayın gereksinimleri](https://support.google.com/youtube/answer/2474026).
