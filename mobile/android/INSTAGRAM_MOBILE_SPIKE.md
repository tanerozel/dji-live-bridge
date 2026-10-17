# Instagram Live Producer: bilgisayarsız akış denemesi

Durum: **Fiziksel telefonda, uygun bir Instagram hesabıyla henüz doğrulanmadı.** Bu belge deneme
planıdır; Instagram bağlantısının çalıştığına veya herkese açık yayın API'sine kanıt değildir.

Amaç: Android telefonda anahtar almayı, drone görüntüsünü göndermeyi ve Instagram'da yayına
geçmeyi bilgisayarsız tamamlamak. Önce sistem tarayıcısıyla denenmeli; sonuç alınmadan uygulamaya
WebView veya sayfadan otomatik anahtar okuma eklenmemeli.

1. Live Producer erişimi bulunan bir hesap ve Chrome bulunan Android telefon hazırla.
2. Chrome'da instagram.com'u aç. Link uygulamaya yönleniyorsa tarayıcıda kalacak şekilde adresi
   doğrudan aç; Instagram uygulamasını kaldırmayı kullanıcı akışının şartı yapma.
3. **Masaüstü sitesi** seçeneğini aç. Instagram'ın kendi sayfasında giriş ve 2FA'yı tamamla.
4. Oluştur → Canlı video menüsünün açıldığını doğrula. Hesapta menü yoksa masaüstü görünümünün
   bunu sağlayacağını varsayma; hesabın Live Producer erişimini ayrı incele.
5. Sayfa sunuyorsa **Practice/Alıştırma** kitlesini seç. URL ve anahtarı Stream My Drone'a elle
   aktar. Anahtarı ekran görüntüsü veya kayıt dosyasıyla toplama.
6. Uygulamanın test videosundan aktarımı başlat; Instagram sayfasına dön. Önizleme, görüntü ve
   sesi doğrula; deneme yayınını başlat. Herkese açık yayın ayrı bir kullanıcı kararıdır.
7. Tarayıcı → uygulama → tarayıcı geçişlerini, ekran kilidini, en az 10 dakika aktarımı ve kısa
   internet kesintisini dene. Live Producer sekmesinin arka planda kalmasının etkisini kaydet.
8. Instagram'da yayını bitir, uygulamada aktarımı durdur. Yeni oturumda yeni anahtar gerekip
   gerekmediğini hesabın gerçek davranışından doğrula.

Başarı ölçütü: hiçbir adımda bilgisayar kullanılmaması, önizleme/ses gelmesi, uygulamalar arası
geçişte aktarımın devam etmesi ve yeniden yayın açılabilmesi. Telefon modeli, Android/Chrome
sürümü, menü erişimi ve sonuç kaydedilmeli; şifre, cookie, token veya anahtar kaydedilmemeli.

Başarılıysa sonraki deneme aynı resmi sayfayı uygulama içindeki masaüstü görünümünde açmaktır.
Giriş/2FA, önizleme ve oturum ömrü yeniden doğrulanır. Görünen alanlardan adres/anahtar aktarma,
sayfa yapısına bağlı ayrı bir prototiptir; resmi API entegrasyonu olarak sunulmaz. iOS'ta tarayıcıya
geçişin arka plan ömrü/Picture in Picture etkisi ayrıca fiziksel cihazda doğrulanmalıdır.

Kaynaklar: [Instagram Live Producer](https://about.instagram.com/blog/tips-and-tricks/instagram-live-producer),
[PRISM'in kendi Instagram RTMP akışı ve sekme gereksinimi](https://guide.prismlive.com/desktop/guides/streaming/rtmp-streaming/connecting-instagram-via-rtmp-presets).
