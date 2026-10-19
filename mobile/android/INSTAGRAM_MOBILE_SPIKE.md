# Instagram: Android uygulama içi Live Producer

## Uygulanan ana akış

Instagram profilinde **Instagram’ı bağla** düğmesi uygulama içinde, ekranı kaplayan güvenli bir
modal açar. Instagram'ın kendi web sayfası Android WebView'da masaüstü kullanıcı aracısıyla
çalışır; tarayıcıya geçmek veya bir bilgisayardan anahtar kopyalamak ana akışın parçası değildir.

1. Stream My Drone → Instagram'a dokun. Kayıtlıysa basılı tutup düzenle.
2. **Instagram’ı bağla** düğmesine bas. Şifre ve 2FA'yı doğrudan Instagram'ın sayfasında tamamla.
   Başlıkta mevcut alan adı gösterilir; Instagram/Facebook giriş sayfaları dışında gezinti engellenir.
3. Uygulama görünen **Oluştur → Canlı video** menüsünü otomatik açmayı dener. Boş yayın başlığı
   **Stream My Drone** ile doldurulur; mevcut başlığın ve sonraki düzenlemelerin korunur.
   Kitle seçimi Instagram sayfasında kullanıcıda kalır; ilk testte varsa **Alıştırma/Practice** seç.
   Menü yoksa/belirsizse işlem durur ve yeniden deneme/elle ilerleme bilgisi gösterilir.
4. Live Producer adres ve anahtar alanlarını gösterdiğinde uygulama bunları otomatik okuyup
   doğrular ve profili mevcut Keystore şifrelemesiyle kaydeder. Pencere kapanır; ayrı kopyalama,
   yapıştırma veya Kaydet adımı gerekmez. **Bilgileri al** düğmesi aynı yakalamayı elle tetikler.
5. Drone'dan ya da test videosundan aktarımı başlat. **Instagram penceresini aç** düğmesi aynı
   bellekteki WebView sayfasını yeniden gösterir. Anahtar alındığında bu sayfa yok edilmez.
6. Önizlemeyi kontrol edip **Instagram'ın kendi Canlı yayına geç düğmesine** bas. Yayını
   Instagram'da da bitir, ardından uygulamada aktarımı durdur.

Bu entegrasyon Instagram'ın web arayüzüne bağlıdır; resmî Instagram yayın API'si veya OAuth
entegrasyonu değildir. Hesapta Live Producer erişimi yoksa bunu sağlayamaz. Web arayüzü/etiketler
ve gömülü giriş politikası değişirse otomatik yakalama veya giriş için uyarlama gerekebilir.
Herkese açık yayın uygulama tarafından kendiliğinden başlatılmaz.

Kayıtlı anahtar bulunması, bellekteki Producer sayfasının hâlâ var olduğu anlamına gelmez.
Aktarım isteğinde o profil için korunmuş Producer bağlantısı yoksa önce hazırlık açılır; güncel
bağlantı kaydedilince yalnız bu açık aktarım isteği sürdürülür. Hazırlık editörünü kapatmak isteği
iptal eder. Elle kaydetme yedeği aynı uygulama oturumunda harici tarayıcı akışına izin verir.

## Otomatik yakalama ve güvenlik

- Yalnız `https://www.instagram.com` üst çerçevesinde çalışır. Login, challenge, checkpoint,
  parola ve tek kullanımlık kod sayfaları kapsam dışıdır; başka origin ve alt çerçeve reddedilir.
- Script görünen RTMPS adresi ve yayın anahtarı alanlarını arar. Sunucu Instagram/fbcdn alan
  adında, `/rtmp` yolunda ve anahtarsız olmalıdır. Anahtar için alan etiketi/okunur alan kontrolleri
  uygulanır. Maske, adres, belirsiz birden fazla anahtar veya hatalı giriş aktarılmaz.
- Native callback tekrar origin, sayfa URL'si ve editör isteğinin güncelliğini kontrol eder;
  her iki alanı tekrar doğrular. Eski sayfa yanıtı başka bir profilin anahtarını değiştiremez.
- `addJavascriptInterface`, cookie okuma, özel Instagram API'si, native parola/token toplama,
  ağ yanıtı yakalama ve ham bağlantı bilgisi loglama yoktur. Gösterilen sayfa üçüncü taraf
  Instagram sayfasıdır; script yalnız bağlantı bilgilerini döndürür.
- WebView Javascript/DOM depolaması Instagram için açıktır; dosya/content erişimi, karma içerik,
  üçüncü taraf cookie'leri, kamera/mikrofon izni ve web hata ayıklaması kapalıdır. TLS hatası
  atlanmaz. HTTP önbelleği kullanılmaz. Modal ve anahtar editörü FLAG_SECURE ile korunur.
- Instagram'ın web oturumu WebView'ın uygulama içi verilerinde kalabilir; native profil dosyasına
  kopyalanmaz. **Oturumu temizle** yerel web cookie/depolamasını temizler. Aktif Instagram aktarımı
  sırasında bu işlem ve bağlantıyı değiştirme kapalıdır. Temizleme bir Instagram API yetkisi iptali
  değildir; uygulama bu tür bir yetki almamaktadır.
- Pencere gizlenirken WebView tutulur. Activity/process kapanması, renderer ölümü ve arka plan
  kısıtlamaları için uzun gerçek yayın testi ayrıca gereklidir; mevcut yerel relay servisi tek başına
  Instagram web oturumunun sonsuza kadar korunacağı garantisi değildir.

## Elle aktarım yedeği

**Elle yapıştır** bölümü eski adres/anahtar alanlarını ve harici tarayıcı yardımını açar. Android'in
Instagram linkini uygulamaya yönlendirmesini önlemek için tarayıcı paketi açıkça hedeflenir.
Tarayıcı keşfi eksikse Chrome/Brave/Samsung Internet/Firefox/Edge/Opera paketleri denenir.
Tarayıcıda Masaüstü sitesi açılır; Live Producer sekmesi kapatılmadan Son uygulamalar üzerinden
uygulamaya dönülür. Adres ve anahtar için iki açık Yapıştır düğmesi vardır.

Pano yalnız bu düğmelere basılınca tek düz metin olarak okunur; URI/intent içerikleri açılmaz,
arka plan dinleme veya dönüşte otomatik pano okuma yoktur. Anahtar maskeli kalır; başarılı
anahtar aktarımında hâlâ aynı içeriği taşıyan pano temizlenir. Bu yedek yol ana akış değildir.

## Doğrulama kaydı — 17 Ekim 2026

- Parser/güvenlik testleri: bölgesel RTMPS adresleri, benzer alan adları, anahtar eklenmiş tam URL,
  maskeli anahtarlar, query içeren anahtar ve tarayıcı fallback sırası.
- Native yakalama testleri: Javascript sonucu, boyut sınırı, yanlış origin, login/checkpoint ve
  uygulama/dosya protokollerinin reddi.
- `node --test mobile/android/scripts/test-instagram-capture.mjs`: yerel DOM fixture testleri;
  Türkçe/İngilizce etiket, şifreli anahtar alanı, giriş alanlarının hiç okunmaması, alt çerçeve,
  gizli/yanlış sunucu, maske ve belirsizlik senaryoları. Bunlar gerçek Instagram DOM kanıtı değildir.
- Galaxy S23 / SM-S911B (`RFCX413L56N`) üzerinde debug yükleme, uygulama açılışı ve PID doğrulandı.
- İlk harici tarayıcı denemesinde keşif filtresi hata verdi. Android sorgusu varsayılan Brave ve
  Instagram link yönlendiricisini gösterdi; filtre/fallback düzeltildi. Kullanıcı tarayıcının
  açıldığını doğruladı, fakat uygulama içi otomatik pencere istedi. Ana akış bu talebe göre değişti.
- Modal sürümü Galaxy S23'e yüklendi. Kullanıcı gerçek hesap denemesinde “Pencere açıldı ve
  bağlantı otomatik kaydedildi” sonucunu bildirdi. Bu, pencerenin açılması ve otomatik profil
  kaydı için cihaz kanıtıdır.
- Devam denemesinde kullanıcı “Aynı oturum korundu; Practice seçili ve önizlemede görüntü var”
  sonucunu bildirdi. Pencerenin aynı Producer oturumuna dönmesi ve görüntünün Instagram Practice
  önizlemesine ulaşması cihazda doğrulandı. Kaynağın drone/test videosu ayrımı, ses ve Practice
  yayınını başlatıp bitirme henüz ayrıca doğrulanmadı.
- Sonraki kontrolde kullanıcı uygulamanın aktarım gösterdiğini, ancak Instagram/YouTube'da
  görünür yayın bulamadığını ve Instagram'ın Canlı yayına geç ekranına erişemediğini bildirdi.
  Önizleme kanıtı herkese açık yayın kanıtı değildir. Bu sorun için aktarım başlatılırken aynı
  Producer penceresi otomatik açılır; görünür yayın düğmesine kaydırma yardımcısı eklendi.
  Yardımcı düğmeye basmaz ve kitleyi değiştirmez. Bu düzeltmenin cihaz testi henüz yapılmadı.
- Düzeltme kontrolü: 100 Android JVM testi ve 10 yerel Javascript fixture testi geçti;
  debug/release APK derlemeleri ve Android lint tamamlandı (0 hata, mevcut 6 uyarı).
- Kullanıcı aktarımı durdurduktan sonra Galaxy S23 tekrar bağlandı. Düzeltme
  `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile derlenip yüklendi:
  Gradle başarılı, `adb install -r` sonucu `Success`, MainActivity açılışı `Status: ok`,
  çalışan uygulama PID'si `9219`. Bu, yükleme/açılış kanıtıdır; yeni yayın penceresi
  geçişi, düğmeye kaydırma ve gerçek yayın sonucu henüz cihazda doğrulanmadı.
- Bu yüklemenin ardından kullanıcı ayrıntıyı netleştirdi: Instagram masaüstü sayfası açılıyor,
  fakat Instagram'ın kendi Canlı video modalı açılmıyor. Önceki düzeltme yalnız dış pencereyi
  açıyordu. Bu nedenle görünen Oluştur/Canlı video kontrolleri için sınırlı menü geçişi ve
  kaybolmuş Producer oturumunu hazırlamadan aktarımı başlatmama akışı eklendi. Menü geçişi
  giriş/2FA alanlarına dokunmaz; başlık, kitle, İleri ve Canlı yayına geç düğmelerine basmaz.
- Menü geçişi sürümü: 101 Android JVM testi ve 14 yerel Javascript fixture testi geçti;
  debug/release derlemeleri, lint (0 hata, mevcut 6 uyarı), site, TypeScript/ESLint ve zorunlu
  Rust kontrolleri tamamlandı (79 geçti, 6 gerçek-model testi kapsam dışı). Galaxy S23'e
  `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile yükleme başarılı, MainActivity
  açılışı `Status: ok`, PID `19966`. Canlı video hazırlık modalının otomatik açılması için
  yeni cihaz denemesi bekleniyor; fixture sonucu gerçek Instagram arayüzü kanıtı değildir.
- Kullanıcı bu sürümde de ana sayfada kaldığını bildirdi. PID `19966` için tanılama yalnız
  `Instagram setup: waiting` durumunu gösterdi: genel görünür diyalog koruması menü geçişini
  engelliyordu. Bu genel koruma kaldırılıp hedef kontrolün üstünde başka bir pencere olup
  olmadığı `elementFromPoint` ile kontrol ediliyor; arka plandaki bir kontrole programatik
  tıklama yapılmıyor. Bu ek düzeltmenin gerçek hesap sonucu henüz doğrulanmadı.
- Ek düzeltme 101 Android JVM ve 15 yerel Javascript fixture testiyle doğrulandı; debug
  derleme/lint başarılı. Aynı yükleme komutuyla Galaxy S23'e kuruldu (`Success`), MainActivity
  açıldı (`Status: ok`), PID `22776`. Kullanıcıdan yeni hazırlık ekranı sonucu bekleniyor.
- PID `22776` denemesinde kullanıcı yine ana sayfa/boş içerik bildirdi. Yardımcı kaydı
  `menu → unavailable`: Oluştur kontrolüne tıklama girişimi kaydedildi, Canlı video kontrolü
  algılanmadı; bu kayıt menünün gerçekten açıldığını kanıtlamaz. Kullanıcı Canlı video'ya elle
  dokununca da modalda içerik olmadığını bildirdi. WebView sağlayıcısı
  `com.google.android.webview 155.0.8059.43`, etkin/geçerli; mevcut Chromium/AndroidRuntime
  hata düzeyi kayıtlarında satır yoktu. Sebep henüz doğrulanmadı. Aynı hesap için Chrome
  masaüstü görünümünde karşılaştırma testi istendi; gerçek yayın hâlâ doğrulanmış değildir.
- Chrome karşılaştırması: kullanıcı aynı telefonda Canlı video ekranının açıldığını doğruladı.
  Geçici yerel tanılama APK'sında form değerleri/çerez/anahtar okunmadan yerleşim incelendi:
  Canlı video alt menüsü düz iç içe `span` öğeleri kullanıyordu; etikete doğrudan tıklanınca
  diyalog oluşuyor, fakat yüksekliği `40px`, içerik üst sınırı `0px` kalıyordu. Java hata
  ayıklayıcıyla WebView parametreleri genişlik/yükseklik `-2/-2` (`WRAP_CONTENT`) olarak ölçüldü.
  Aynı açık WebView'a ana thread üzerinde `MATCH_PARENT` parametreleri uygulanınca, geçici CSS
  denemeleri kaldırılmışken `100vh ≈ 1454px`, diyalog `529px` oldu. Kullanıcı “Başlık/kitle ekranı
  artık görünüyor” sonucunu doğruladı. Kalıcı düzeltme WebView oluşturulurken MATCH_PARENT
  kullanıyor; menü yardımcısı en içteki tek, görünür Canlı video span etiketini de destekliyor.
  Normal kaynakta web hata ayıklaması kapalıdır.
- Kalıcı ölçü/menü düzeltmesi: 101 Android JVM ve 16 Javascript fixture testi geçti;
  debug/release derlemeleri, lint (0 hata, mevcut 6 uyarı), TypeScript/ESLint ve çeviri kontrolü
  başarılı. Normal APK `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile Galaxy S23'e
  yüklendi (`Success`), MainActivity açıldı (`Status: ok`), PID `12222`. Geçici DevTools
  bağlantısı, tanılama APK'sı ve yerel yardımcı dosyalar kaldırıldı. Normal APK'daki otomatik
  hazırlık geçişinin kullanıcı doğrulaması bekleniyor; görünür gerçek yayın henüz doğrulanmadı.
- Normal APK kontrolünde kullanıcı canlı video başlığı istendiğini bildirdi; boş pencere yerine
  hazırlık formuna erişilebildi. Kullanıcının isteğiyle boş başlığı otomatik doldurma eklendi:
  yalnız tek Live video diyaloğundaki başlık alanını doldurur, dolu başlığı/sonraki silmeyi
  değiştirmez; başlık değerini native tarafa/loga döndürmez. Giriş/2FA, farklı origin ve belirsiz
  alanlar kapsam dışıdır. Kitle, İleri ve yayın başlatma kullanıcıda kalır. Cihaz doğrulaması bekleniyor.
- Başlık sürümü: 101 Android JVM ve 20 Javascript fixture testi geçti; debug/release derlemeleri,
  lint (0 hata, mevcut 6 uyarı), çeviri, site, TypeScript/ESLint kontrolleri başarılı.
  `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile Galaxy S23'e yüklendi (`Success`),
  uygulama açıldı (`Status: ok`), PID `27110`. Otomatik başlığın dolduğu kullanıcı tarafından gerçek cihazda doğrulandı.

## Gerçek hesap testi

- [x] Uygulama içi Instagram penceresi açıldı (kullanıcı doğrulaması).
- [x] Boş Canlı video diyaloğu sorunu aynı açık cihaz oturumunda ölçü düzeltmesiyle giderildi;
  başlık/kitle içeriğinin görünmesi kullanıcı tarafından doğrulandı.
- [ ] Yeni giriş ve 2FA deneyimi ayrıca doğrulandı.
- [x] Practice seçeneği bulundu ve seçildi (kullanıcı doğrulaması).
- [x] Bağlantı profili kendiliğinden kaydedildi (kullanıcı doğrulaması).
- [x] Pencere yeniden açıldığında aynı Producer oturumu korundu (kullanıcı doğrulaması).
- [x] Aktarılan görüntü Instagram Practice önizlemesine ulaştı (kullanıcı doğrulaması).
- [ ] Ses ayrıca doğrulandı (ses içeren bir kaynakla).
- [ ] Practice Instagram'da başlatılıp bitirildi; uygulamada aktarım durduruldu.
- [ ] Uzun yayın, ekran kilidi, uygulama geçişi, ağ kesintisi ve yeni yayın oturumu test edildi.

Sonucu telefon/Android/WebView sürümü ile kaydet; şifre, cookie, token ve gerçek anahtar kaydetme.
Practice yoksa kendiliğinden herkese açık yayın açma.

Kaynaklar: [Instagram Live Producer](https://about.instagram.com/blog/tips-and-tricks/instagram-live-producer),
[Android WebView](https://developer.android.com/develop/ui/views/layout/webapps/webview).
Meta kaynakları bu araştırmada erişim sınırı verdi; yeni bir resmî yayın API'si olduğu ileri sürülmedi.

## Tek Başlat akışı (2026-10-19)

- Kullanıcı boş başlığın otomatik dolduğunu doğruladı.
- Bağlantı ekranındaki tekrarlı anahtar açıklamaları kaldırıldı. Instagram penceresinde kısa
  kitle/İleri yönlendirmesi var; tarayıcı ve oturum temizleme taşma menüsünde. Bağlantı kaydolunca
  pencere kapanır. Kayıtlı oturum ölçülü WebView olarak ana ekranın arkasında korunur.
- Yalnız uygulamadaki açık Başlat isteği, ilgili profilde gerçek veri aktarımı ve hazır Instagram
  Go live düğmesini bekleyen tek kullanımlık bir işlem oluşturur. Kitle değiştirilmez. Tıklama
  callback kaybında tekrarlanmaz; End live düğmesiyle başlama ayrıca doğrulanır. Durdurma, belge
  değişimi, renderer kaybı ve 90 saniyelik süre aşımı işlemi iptal eder; kalıcı kuyruk/replay yoktur.
- Ana ekranda görüntü bekleniyor / başlatılıyor / başlatıldı / doğrulanamadı durumları vardır.
  Manuel bağlantıda yayın başlatma manuel kalır. Yayını Instagram tarafında bitirme bu kapsamda
  otomatikleştirilmedi; aynı pencereye dönülür.
- Yeni Go live otomasyonunun gerçek hesap sonucu ve ekran kilidi/uygulama geçişi testi bekleniyor.
  Sonraki cihaz denemesi yalnız Alıştırma/Practice olmalı; herkese açık yayın doğrulanmış sayılmaz.

- Doğrulama: 104 Android JVM ve 25 JavaScript fixture testi geçti. Debug ve küçültülmüş release
  derlemeleri başarılı. TypeScript/ESLint, iOS çeviri kontrolü (45 dosya / 15 dil), site kontrolü
  (15 dil / 40 yazı) ve Rust fmt/clippy geçti. Rust testindeki sandbox soket kısıtı nedeniyle aynı
  komut yetkili ortamda tekrarlandı: 79 geçti, 6 gerçek-model testi kapsam dışı bırakıldı.
- Yükleme yolu: `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh`; yükleme `Success`,
  MainActivity `Status: ok`. Yayın başlangıcı gerçek hesapta henüz doğrulanmadı. Güvenli pencere
  ekran görüntüsünü siyah verdiği için görsel cihaz doğrulaması yapılmış sayılmadı.
- Son kaynakla tekrar derleme/yükleme tamamlandı: Android lint 0 hata / mevcut 6 uyarı;
  `Success`, MainActivity `Status: ok`, PID `22116`. WebView hata ayıklaması kapalı. Cihazın
  gerçek Practice başlangıç sonucu kullanıcıdan bekleniyor.
- Son sürümün cihazdaki sabit tanılama kayıtlarında `setup: menu`, `setup: opened` ve
  `title: default filled` görüldü; PID `22116` çalışmaya devam etti. Bu kayıtlar son Go live
  tıklamasını veya gerçek yayını kanıtlamaz; Practice sonucu henüz alınmadı.

## Bağlantı ekranında kalma düzeltmesi (2026-10-19)

- Kullanıcı İleri sonrası adres/anahtar ekranında kalındığını bildirdi. Açık gerçek WebView
  sayfası, değer/HTML/çerez dışarı çıkarılmadan yapısal boolean kayıtlarıyla incelendi. Anahtar
  inputu readonly text, etiket ise onu saran LABEL'ın bir üst DIV'indeydi. Eski okuyucu yalnız
  doğrudan etiket/tek üst elemanı taradığından anahtarı tanıyamıyordu.
- Okuyucu en fazla üç üst elemanda, yalnız tek aday alan içeren kapsayıcıların etiketini arar;
  ortak sunucu/anahtar kapsayıcısına çıkmaz. Gerçek açık sayfada yeni okuyucu yalnız
  `{ready: true}` sonucuyla doğrulandı; adres/anahtar native dışına aktarılmadı. Native profil
  kaydı ve pencere kapanması yeni APK'da ayrıca kullanıcı kontrolü bekler.
- Aynı sayfada eski diyalog katmanlarında iki devre dışı Go live düğmesi saptandı. Başlatıcı
  benzersizliği yalnız etkin ve hit-test ile erişilebilir düğmeler arasında kontrol eder;
  iki gerçekten erişilebilir aday varsa yine tıklamaz. Kitle ve Next otomatik değiştirilmez.
- Regresyon: üst kapsayıcıdaki etiket, ortak kapsayıcıdan yanlış alan seçmeme ve eski düğme
  katmanları testleri eklendi. 104 Android JVM / 28 JavaScript testi geçti. Debug/release,
  lint (0 hata / mevcut 6 uyarı), Rust fmt/clippy/test (79 geçti, 6 kapsam dışı), TypeScript,
  ESLint ve iOS çeviri kontrolü geçti. Geçici USB DevTools yönlendirmesi kaldırıldı.
- Yükleme denemesinde USB cihazı kayboldu: `adb devices -l` boş, `start.sh` cihaz bağlı ve
  yetkili değil hatasıyla durdu. Bu düzeltmenin APK'sı henüz telefona kurulmadı; kullanıcıdan
  yeniden bağlama istendi. `adb forward --list` boş; geçici tanılama yönlendirmesi kalmadı.
- Telefon yeniden bağlandıktan sonra `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh`
  başarıyla tamamlandı: güncel debug derlemesi, APK `Success`, MainActivity `Status: ok`,
  PID `15400`. Native otomatik kayıt/pencere kapanması kontrolü kullanıcıdan bekleniyor.

## Instagram oturumunu bitirme (2026-10-19)

- Kullanıcı önceki akışın çalıştığını, uygulamadaki Bitir sonrası Instagram yayınının açık
  kaldığını doğruladı. Eski Bitir yalnız RTMP aktarımını kesiyordu.
- Uygulama ve bildirim aynı servis yolundan korunan Producer oturumundaki End/onay adımlarını
  en fazla sekiz saniye dener. Instagram sonuçlanana veya süre dolana kadar yalnız ilgili ingest
  korunur; diğer platformlar bağımsız durur. Süre sonunda aktarım her durumda kesilir.
- Birbirinden bağımsız End/onay tek-sefer iddiaları, renderer son kullanma süresi ve belge/origin
  kontrolleri var. Go live, paylaşma, tekrar silme veya genel Kapat tıklanmaz. Başarı yalnız
  açık sonuç başlığı veya etkin olmayan yayın hazırlığının Go live kontrolüyle doğrulanır.
- Gerçek korunan sayfada salt okunur yeni okuyucu `end` durumunu döndürdü. Gerçek End tıklaması
  otomatik onay incelemesince reddedildi; mevcut yayını hemen bitirme testi için açık kullanıcı
  onayı gerektiği belirtildi. Tıklama yapılmadı; gerçek kapanış henüz doğrulanmadı.
- Son doğrulama: 108 Android JVM / 36 JavaScript fixture testi geçti; debug/release başarılı,
  Android lint 0 hata / mevcut 6 uyarı. Rust fmt/clippy ve 79 test (6 kapsam dışı), TypeScript,
  ESLint, 15 dil/40 yazı site kontrolü ve 45 dosya/15 dil iOS çeviri kontrolü geçti.
- Hazırlanan bitirme APK'sı henüz yüklenmedi; mevcut oturumun End/onay/sonuç ekranı testi için
  açık kullanıcı onayı soruldu. Onay gelene kadar gerçek End tıklaması yapılmayacak.
- Kullanıcı gerçek yayına dokunulmamasını, yalnız yeni sürümün yüklenmesini seçti. Bu sınıra
  uygun olarak Instagram'da End/onay testi yapılmadı. `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh`
  ile güncel debug derlendi, APK `Success`, MainActivity `Status: ok`, PID `25707` doğrulandı.
  Otomatik bitirmenin gerçek oturum sonucu henüz doğrulanmış değildir.

## Ortak yayın başlığı (2026-10-19)

- Başlat öncesinde ortak başlık diyaloğu açılır. Son onaylanan başlık yerel tercihlerden gelir;
  geçişte seçili YouTube profil başlığı, o da yoksa Stream My Drone kullanılır. Temizle yalnız
  taslağı temizler; İptal kayıt veya yayın başlatmaz. Geçerli onay başlığı hedeflerle birlikte
  immutable istek olarak izin/Instagram hazırlığı boyunca korunur; yeniden sorulmaz.
- Yeni yayında seçili bütün hedeflere video akışı gider. Başlık bağlı YouTube/Instagram için
  otomatik uygulanır; diğer manuel anahtarlı hedeflerin başlıkları platformda düzenlenir.
  Bu sınır ilgili hedef seçiliyken diyalogda belirtilir.
- YouTube servis isteği ortak başlığı yeniden doğrular ve yeni broadcast isteğine uygular;
  profilin kayıtlı varsayılanı, hesabı, görünürlüğü ve çocuk içeriği seçimi değiştirilmez.
- Instagram başlık seçimi revizyonla sınırlandırılır. Bağlantı yakalanırken yalnız eşleşme
  boolean bilgisi döner; başlık metni loglanmaz. Farklı/eski taslak yeni hazırlık gerektirir;
  alan sınırı aşılırsa sessiz kesme yapılmaz. Kitle/İleri kullanıcıda kalır.
- Kullanıcının önceki sınırı geçerlidir: mevcut gerçek Instagram yayınına test için dokunulmaz.
  Bu özellik için gerçek yayın başlatma/bitirme yapılmayacak; build/fixture kanıtı ayrı tutulur.
- Son doğrulama: 114 Android JVM / 41 JavaScript testi geçti; debug/release derlemeleri ve
  Android lint başarılı (0 hata / mevcut 6 uyarı). Rust fmt/clippy, 79 test (6 kapsam dışı),
  TypeScript/ESLint, 15 dil/40 yazı site ve 45 dosya/15 dil iOS çeviri kontrolleri geçti.
- `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile Galaxy S23'e güncel debug APK
  yüklendi: build başarılı, APK `Success`, MainActivity `Status: ok`, PID `30192`.
  Gerçek platformda yeni başlıkla yayın açma veya mevcut yayını bitirme testi yapılmadı.

## Instagram hazırlığını arka planda tamamlama (2026-10-19)

- Önceki sürümde başlık otomatik olsa da kitle/İleri Instagram penceresinde bırakılıyordu.
  Başlat artık tek native başlık/kitle onayı alır. Bağla son değerlerle yalnız taslak hazırlar,
  başlık/kitleyi yeniden sormaz ve yayın açmaz. İlk kullanım Alıştırma; son onaylanan seçim
  yerel tercihlerde hatırlanır ve Başlat onay ekranında görünür. Devam eden yayına
  Instagram eklenirken de kitle onayı gösterilir.
- Giriş açıksa ölçülmüş, dokunulamayan WebView Create → Live video, başlık, onaylanan kitle
  ve İleri adımlarını uygulamanın arkasında yürütür. Native ilerleme göstergesi iptal edilebilir.
  Login/2FA veya hazırlık hatası sayfayı öne getirir; giriş bitince otomatik devam eder.
  Hazırlık döngüsü görünür pencere kompozisyonundan bağımsızdır.
- Ayrı setup helper yalnız aynı revizyonun başlık kanıtıyla çalışır. Seçilen audience
  radio/option/combobox/native-select durumuyla doğrulanır; benzersiz ve örtülmeyen İleri
  bir defa tıklanır. Audience/revizyon kanıtı bağlantıyla yalnız boolean olarak döner.
  Yanlış/eski başlık veya kitle kanıtı otomatik Başlat isteğini sürdüremez. Login alanı,
  Go live veya End hazırlıkta tıklanmaz. Sınır 60 saniye; belirsiz hedefler tahmin edilmez.
- İptal bekleyen Başlat isteğini siler. Normal Bağla yalnız bağlantıyı kaydeder; yayın
  başlatmaz. Açık native Başlat onayı sonrasında mevcut video/Go live doğrulama akışı kullanılır.
  Retry yeni bir hazırlık revizyonu açar; önceki İleri/kitle işlemleri kendiliğinden tekrarlanmaz.
- Gerçek Instagram yayınına dokunmama sınırı korundu. Bu değişiklik için cihazda uçtan uca
  yeni başlık/kitle/Practice yayını henüz doğrulanmadı; aşağıdaki testler fixture/build kanıtıdır.

- Kontroller: 115 Android JVM / 51 JavaScript testi geçti. Android lint 0 hata / mevcut
  6 uyarı; debug/release derlemeleri başarılı. Rust fmt/clippy, 79 test (6 kapsam dışı),
  TypeScript/ESLint, 15 dil/40 yazı site ve 45 dosya/15 dil iOS çeviri kontrolü geçti.
  Rust overlay testi yerel TCP listener açamadığı kısıtlı ortamda başarısız oldu; yerel
  soket erişimiyle tam test çalıştırması 79/79 geçti. İlgisiz Vision kodu değiştirilmedi.

- Son sürüm `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile Galaxy S23'e
  yüklendi: debug build başarılı, install `Success`, MainActivity `Status: ok`, PID `18226`.
  APK içindeki setup/capture helper'ları son kaynakla birebir eşleşiyor. Gerçek Instagram
  yayınında başlatma veya bitirme testi yapılmadı; mevcut yayına dokunulmadı.

## Hazırlık uyarısını ayırma (2026-10-19)

- Kullanıcı otomatik Canlı video hazırlığı uyarısını bildirdi. PID 18226 sabit durum
  kayıtları Create/menu → Live video/opened → title/filled gösterdi; menü/başlık tamamlanmış,
  kitle/İleri aşamasının nedeni ilk sürümde loglanmıyordu. WebView sağlayıcısı geçerli
  `com.google.android.webview 155.0.8059.43`.
- Çalışan oturuma debugger ile DevTools açma denemesi otomatik onay incelemesince güvenlik
  nedeniyle reddedildi ve çalıştırılmadı. Bu yol veya bir eşdeğeri denenmedi. Bunun yerine
  normal WebView debugging kapalı bırakıldı; sadece sabit rol/tag/kontrol kodları döndüren,
  native beyaz listeyle ikinci kez süzülen, alan değeri/hesap/çerez okumayan dar probe eklendi.
- Probe sürümü: 115 Android JVM / 53 JavaScript testi ve lint geçti; Galaxy S23'e
  `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile yüklendi (`Success`, `Status: ok`,
  PID 23361). Canlı yayın açma/bitirme yapılmadı; yalnız Bağla hazırlık denemesi istendi.

- Gerçek probe sonucu: `public:span:none:div:none:enabled`; kitle metni SPAN, üst DIV'de
  semantik button/combobox rolü yok. Önceki selector bu gerçek kontrolü dışladığı için
  `preparation: unavailable` ve süre sonunda genel hata oluşuyordu. Create ve başlık sorunsuzdu.
- Düzeltme tam Live video diyaloğundaki en içteki tek kitle SPAN etiketini ve açılan menüdeki
  aynı tür seçenekleri destekler. Seçim sonrası menünün kapanması ve tek görünen etiketin
  istenen kitleyle eşleşmesi gerekir. Tıklama tek başına kanıt sayılmaz. Üst düğmenin disabled
  durumu ve örtülme kontrolü SPAN için de geçerlidir. Go live/End bu yardımcıda yoktur.
  Draft Next/Continue/İleri aynı dar scope içinde desteklenir. Uyarı 15 dilde hazırlığın
  tamamlanamadığını anlatır; açılmış ekranı açılmamış gibi adlandırmaz.

- İlk SPAN düzeltmesi cihazda kitle tetikleyicisini açtı (`audience-opened`) fakat seçim
  bekledi. USB yeniden bağlandıktan sonra aynı deneme `public:button:none:div:none:enabled`
  ve `practice:button:none:div:none:enabled` döndürdü: seçenekler ayrı portalda normal BUTTON,
  role=option değil. Helper artık yalnız kendi açtığı menüden sonra, küçük ortak kapsayıcıda
  tam Public/Practice düğme çiftini bulup istenen seçeneğe tek kez dokunur. Sonraki kapalı
  seçici doğrulaması yine zorunludur. Gerçek yapı regresyonu 59 JavaScript testinden geçti.

- Son düzeltme: 115 Android JVM / 59 JavaScript testi geçti. Debug/release ve Android lint
  başarılı (0 hata / mevcut 6 uyarı); Rust fmt/clippy ve 79 test (6 kapsam dışı), TypeScript,
  ESLint ve 45 dosya/15 dil iOS çeviri kontrolü geçti.
- `env DJI_ANDROID_DEVICE_SERIAL=RFCX413L56N ./start.sh` ile son kaynak S23'e yüklendi:
  install `Success`, MainActivity `Status: ok`, PID 29864. Paket içindeki setup/diagnose
  helper'ları kaynakla birebir aynı. WebView debugging kapalı; ADB DevTools forward yok.
- Gerçek Bağla denemesi (PID 29864): menu → opened → title/filled → preparation/next →
  title/verified. Otomatik capture guard, başlık VE audience aynı revizyonla eşleşmeden
  verified sonucuna izin vermediğinden bu kayıt yeni hazırlığın iki doğrulamasını da kanıtlar.
  capture sırasında pencere gizlendi ve profile-save callback'ine geçildi. Yeni gerçek yayın
  başlatma/bitirme yapılmadı; Go live veya mevcut yayının kapanışı bu testin kanıtı değildir.
