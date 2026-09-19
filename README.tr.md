# DJI Live Bridge

[English](README.md) · **Türkçe** · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

DJI Fly'ın RTMP yayınını aynı LAN'daki Mac'te karşılayan, MediaMTX üzerinden yerel preview'a ve OBS gerektirmeyen yerleşik FFmpeg production hattına dağıtan macOS uygulaması. Rust yalnız süreç, durum, güvenlik ve orchestration yapar; video frame'leri Rust içinden geçmez. OBS isteğe bağlı gelişmiş entegrasyondur.

## Çalışan vertical slice

1. Uygulama aktif default-route IPv4 arayüzünü bulur; loopback, link-local ve `utun` benzeri arayüzleri varsayılan seçimden çıkarır.
2. Paketlenmiş MediaMTX'i doğrular ve başlatır.
3. DJI RC 2 için `rtmp://<LAN_IPV4>:1935/drone` adresini ve QR kodunu gösterir.
4. `/drone` publisher durumunu MediaMTX Control API'den izler; reconnect için MediaMTX'i yeniden başlatmaz.
5. Direct WHEP preview'ı dener. Codec/ICE sorunu veya AAC preview sesi gerektiğinde, yalnız `drone-preview` için H.264 + Opus FFmpeg fallback'i açabilir.
6. Yerleşik production hattı FFmpeg capability probe sonucuna göre VideoToolbox/libx264 ile layout uygular; drone sesi ile isteğe bağlı AVFoundation mikrofonunu miksler ve `/production` yolunu üretir.
7. Direct RTMP yalnız `START LIVE` ile başlar. Tek production çıktısı, etkinleştirilen birden fazla Instagram/TikTok/özel RTMP hedefine aynı anda iletilir. Her stream key Keychain'den belleğe alınır, loopback Control API üzerinden geçici MediaMTX forward listesine verilir ve `STOP LIVE` ile silinir; FFmpeg argv'sine yazılmaz.
8. Yerleşik Core Media I/O Camera Extension, `/drone` videosunu `DJI Live Bridge Camera` adıyla 1080×1920/30 fps video-only kamera olarak sunar. TikTok LIVE Studio mikrofonu doğrudan kendi içinden seçer; OBS gerekmez.
9. İsteğe bağlı OBS WebSocket 5.x entegrasyonu `GetVersion.availableRequests` ile runtime feature detection yapar; scene, recording ve OBS Virtual Camera özellikleri OBS kuruluysa kullanılabilir.

DJI Fly yolu: **GO FLY → Transmission → Live Streaming Platforms → RTMP**. RC 2 / DJI Fly 1.16+ yayın başlatmak için RC 2'ye bağlı ayrı bir mikrofon isteyebilir. Bu mikrofon Mac/OBS commentary mikrofonu değildir.

RC 2 girdisi 720p olabilir ve UI gerçek ffprobe metadata'sını gösterir. 1080p veya portrait production layout seçimi 720p girdiyi upscale eder; uygulama bunu native 1080p input olarak sunmaz.

## Gereksinimler

- macOS 13 veya üzeri. Apple Silicon ve Intel için ayrı sürümler yayınlanır.
- Windows 10/11 (beta): yayın, önizleme, kayıt ve tanılama çalışır. Yerleşik sanal kamera şimdilik yalnız macOS'tadır; Windows'ta TikTok LIVE Studio için OBS sanal kamerası kullanılır. Kurulum henüz imzalanmadığından SmartScreen uyarı verir.
- Rust 1.98.1 (`rust-toolchain.toml`).
- Node.js 22 ve npm.
- OBS Studio 32.2.2 + obs-websocket 5.x yalnız isteğe bağlı advanced path içindir; varsayılan Direct RTMP hattında OBS gerekmez.
- FFmpeg/ffprobe uygulamanın içinde gelir (LGPL, `scripts/build-ffmpeg.sh` ile derlenir); kullanıcının ayrıca kurmasına gerek yoktur.
- Native Camera Extension derlemesi için macOS SDK içeren Xcode Command Line Tools gerekir. Kullanılabilir dağıtım için ana uygulama ve uzantı aynı Apple Developer Team ile imzalanmalı; yerel ad-hoc imza yalnız derleme doğrulamasıdır.

Sürüm kaydı [versions.lock.json](versions.lock.json) dosyasındadır. Değişken bağımlılık kaynakları ve checksum [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) içinde kayıtlıdır.

## Kurulum ve çalıştırma

```sh
npm ci
rustup toolchain install 1.98.1
rustup target add aarch64-apple-darwin x86_64-apple-darwin --toolchain 1.98.1
./scripts/download-mediamtx.sh arm64
npm run tauri -- dev
```

Intel için sidecar:

```sh
./scripts/download-mediamtx.sh x86_64
```

MediaMTX binary'si kaynak kontrolüne alınmaz; script resmi release archive'ını indirir, resmi SHA-256 listesiyle doğrular ve Tauri'nin zorunlu `mediamtx-<target-triple>` adına kurar.

## Test Drone

`Start Test Drone` ile yerel bir video seçildiğinde gerçek hat kullanılır:

```text
file → FFmpeg H.264/AAC 1280x720 → rtmp://127.0.0.1:1935/drone
     → MediaMTX → WHEP preview
     → FFmpeg layout/audio mix → rtsp://127.0.0.1:8554/production
     → MediaMTX runtime forwards → Instagram + TikTok + custom RTMP
```

Fake `connected` durumu üretilmez. FFmpeg/ffprobe bulunamazsa özellik açıkça unavailable olur.

## TikTok LIVE Studio — OBS'siz kullanım

Bu akışta TikTok server URL veya stream key gerekmez:

1. İmzalı `DJI Live Bridge.app` dosyasını `/Applications` klasörüne taşı ve oradan aç.
2. Mac ve DJI kumandayı aynı telefon hotspot'una bağla. DJI Fly yayın adresi olarak uygulamada gösterilen `rtmp://<hotspot-IP>:1935/drone` adresini kullan.
3. DJI Fly yayınını başlat ya da uygulamadaki `Start Test Drone` ile görüntüyü doğrula.
4. Görüntü önizlemesinin altındaki ana karttan bir kez `Sanal Kamerayı Etkinleştir` düğmesine bas.
5. macOS isterse **System Settings → Privacy & Security** altında sistem uzantısına izin ver. Kart `Başlatmaya hazır` durumuna gelince `Sanal Kamerayı Başlat` düğmesine bas.
6. Aynı karttaki `LIVE Studio'yu aç / indir` düğmesiyle TikTok LIVE Studio'yu aç.
7. TikTok LIVE Studio içinde yeni bir Camera kaynağı ekleyip **DJI Live Bridge Camera** seç.
8. Camera kaynağının `More settings → Audio capture` ayarını `None` yap.
9. TikTok LIVE Studio ana mikrofon denetiminden yalnızca bir fiziksel mikrofon seç. Bu kamera bilerek yalnız video taşır; DJI Live Bridge bu modda ses yakalamaz veya TikTok'a ses göndermez.
10. Yayın sesini dinleyeceksen hoparlör yerine kulaklık kullan; böylece mikrofonun hoparlör sesini yeniden alması engellenir.

Ana ekrandaki üç adımlı gösterge bağlantı, önizleme ve sanal kamera durumunu takip eder. RTMP hedefleri, OBS, kayıt, kodek metrikleri ve tanılama normal LIVE Studio akışını kalabalıklaştırmaması için `Gelişmiş ayarlar ve tanılama` altında bulunur.

Görüntü yolu şöyledir:

```text
DJI / Test Drone → MediaMTX /drone → FFmpeg NV12 1080×1920@30
                 → Localhost NV12 stream → Core Media I/O Camera Extension
                 → DJI Live Bridge Camera → TikTok LIVE Studio
```

Kamera etkinleştirme macOS güvenlik modelinin parçasıdır; uygulamanın `/Applications` altında bulunması ve ilk kullanımda yönetici onayı gerekir. `Go Live` işlemi TikTok LIVE Studio içinde manuel kalır.

## Instagram / TikTok'a Direct RTMP yayın

`Canlı yayın` sekmesi tek ekranlık üç adımlı akıştır:

1. **Drone'u bağla** — gösterilen `rtmp://…/drone` adresini DJI Fly'a yapıştır (veya `Bir video dosyasıyla dene`). Görüntü gelince adım yeşile döner.
2. **Nereye yayın yapacaksın?** — Instagram, TikTok veya Özel RTMP seç; platformun verdiği sunucu URL'si ile yayın anahtarını yapıştırıp kaydet. Instagram'da: instagram.com → Oluştur (+) → Canlı video. Instagram her yayında yeni anahtar verir; hedef kartındaki `Anahtarı güncelle` ile yenile. Birden fazla hedef açıksa aynı görüntü hepsine aynı anda gider.
3. **Görüntü ve ses** — Instagram/TikTok için `Dikey 9:16` (varsayılan), kadraj ve isteğe bağlı mikrofon. Seçimler hatırlanır.

Sağdaki `YAYINI BAŞLAT` düğmesi, üç ön koşul (yerel sunucu, drone görüntüsü, seçili hedef) sağlanınca açılır; production hattını hazırlayıp yayını tek tıkla başlatır. Yayın sırasında süre, her hedefin durumu (`Yayında` / `Bağlanıyor` / `Bağlantı başarısız`) ve gönderilen veri gösterilir; bir hedef hata verse bile diğeri devam eder. Instagram'da yayının herkese açılması için Instagram'daki `Canlı yayına geç` düğmesine ayrıca basılmalıdır. Bitirmek için `Yayını bitir`.

Uygulama platform hesabına giriş yapmaz ve platformdaki yayın başlatma/onay ekranlarını otomatik geçmez. TikTok LIVE Studio sanal kamerası ayrı sekmededir; kayıt, ölçümler, OBS ve tanılama `Gelişmiş` sekmesindedir.

## Dil ve tema desteği

Uygulama İngilizce, Türkçe, İspanyolca, Çince (Basitleştirilmiş ve Geleneksel), Arapça, Hintçe, Portekizce, Rusça, Fransızca, Almanca, Japonca, Korece, Endonezyaca ve İtalyanca dillerinde gelir. İlk açılışta sistem dilini izler, desteklenmiyorsa İngilizceye döner. Dil seçici üst menüdedir ve tercih yerel olarak saklanır. Arapça seçildiğinde arayüz tümüyle sağdan sola döner; URL, port ve kodek gibi teknik değerler soldan sağa kalır. Cihaz adları, kodekler, protokoller ve ham hata ayrıntıları teşhis doğruluğu için çevrilmez.

Beş tema vardır: Sistem (macOS'u izler), Açık, Koyu, Gece mavisi ve Kum.

## Kurulum (kullanıcılar)

1. Mac'ine uygun `.dmg` dosyasını [Releases](https://github.com/tanerozel/dji-live-bridge/releases) sayfasından indir: Apple Silicon için `aarch64`, Intel için `x86_64`.
2. **DJI Live Bridge**'i **Uygulamalar** klasörüne sürükleyin. Sanal kamera yalnızca oradan çalışır.
3. Hepsi bu. FFmpeg uygulamanın içinde gelir, başka bir şey kurman gerekmez.

Uygulama Apple tarafından imzalanıp notarize edilmiştir; uyarısız açılır.

## Yerleşik production ve yayın davranışı

- Varsayılan engine `NativeFfmpeg`'dir. Landscape `1920x1080`, TikTok Portrait `1080x1920`; framing varsayılan `Fit` ve crop yoktur.
- Mikrofon AVFoundation üzerinden yalnız kullanıcı seçerse capture edilir. Volume, non-negative sync delay, FFT noise reduction, compressor ve limiter capability probe ile uygulanır. macOS Microphone izni o zaman gerekir.
- Drone ses track'i yoksa ve mic seçilmediyse geçerli AAC stereo silence üretilir. Mic seçiliyse drone sesiyle `amix` üzerinden karıştırılır.
- Recording, yayın aktifse gerçek `/production` mix'ini; değilse `/drone` akışını stream-copy MKV olarak `~/Movies/DJI Live Bridge/` altına yazar.
- Her Instagram/TikTok/özel RTMP hedefinin stream key'i ayrı bir macOS Keychain kaydında kalır. Hedef listesi MediaMTX Control API ile runtime'da eklenir; anahtarlar config dosyasına ve child process argv'sine girmez.
- Görüntü kalitesi: çıkış sabit 30 fps, 6 Mbps CBR, H.264 High, 2 sn keyframe (Instagram/Facebook ingest gereksinimi). VideoToolbox donanım encoder'ı öncelikli (CPU'yu boş bırakır; RTSP okuyucusu geri kalmaz), yoksa libx264. `Tüm görüntü` kadrajı boş alanı siyah bant yerine videonun bulanık kopyasıyla doldurur; ölçeklemede lanczos kullanılır. MediaMTX okuyucu kuyruğu (`writeQueueSize`) kısa takılmalarda kare atmamak için 4096'dır.
- Test Drone, videonun yönünü korur (uzun kenar 1280, 30 fps); dikey test videosu artık yatay çerçeveye sıkıştırılmaz.
- Tek FFmpeg production encode'u MediaMTX tarafından etkin hedeflere ayrı ayrı forward edilir. Her hedefin `state`, `lastError` ve `outboundBytes` alanları gerçek Control API verisi olarak UI state'ine taşınır; tek hedef hatası diğer hedefi durdurmaz.
- Çoklu hedef için yerleşik `NativeFfmpeg` engine kullanılır. İsteğe bağlı OBS engine bu uygulama içinden tek RTMP hedefiyle sınırlıdır.

## İsteğe bağlı OBS davranışı

- Kaynak türü `ffmpeg_source`, OBS'nin `GetInputKindList` cevabında doğrulanır; varsayılan ayarlar `GetInputDefaultSettings` ile alınır.
- OBS Media Source URL'si `rtsp://127.0.0.1:8554/drone` olur.
- Virtual Camera yalnız video taşır ve FFmpeg beslemesi `-an` ile sesi açıkça kapatır. TikTok LIVE Studio Camera kaynağında `Audio capture = None` tutulur; kullanıcı tek mikrofonu LIVE Studio'nun ana ses denetiminden seçer. Uygulama sürücü veya virtual-audio device kurmaz.
- CoreAudio input cihazları capture başlatmadan izlenir ve connect/disconnect değişimleri Tauri state event'iyle UI'a taşınır.
- OBS engine açıkça hazırlanırsa Direct RTMP OBS video/audio mix'ini taşır; service snapshot/restore davranışı korunur.
- Yayın aktifken stream service ayarı değiştirilmez. `START LIVE` öncesinde snapshot alınır; `STOP LIVE`, kaynak disconnect'i ve app kapanışı geri yüklemeyi dener.
- OBS yoksa uygulama resmi download sayfasını açar. OBS testleri kurulumsuz ortamda başarılı gösterilmez.
- Recording, OBS'nin runtime'da ilan ettiği `GetRecordStatus` / `StartRecord` / `StopRecord` çağrılarıyla idempotent yönetilir. Çıktı yolu OBS aktif profilindendir ve `StopRecord` cevabındaki gerçek `outputPath` gösterilir; undocumented profile anahtarlarıyla zorla değiştirilmez.

TikTok LIVE Studio'nun güncel resmi indirme sayfası macOS 12+ build sunuyor. Uygulama `/Applications/TikTok LIVE Studio.app` keşfi yapar; yüklüyse açar, değilse resmi sayfaya yönlendirir. OBS'siz kamera bu repository'deki Core Media I/O Camera Extension ile sağlanır. İmzasız/ad-hoc build, Apple Developer Team yetkili dağıtım imzası yerine geçmez ve etkinleştirme başarısı olarak gösterilmez. TikTok login/private API/bypass/Go Live otomasyonu yoktur.

## Güvenlik ve yerel portlar

| Port | Bind | Amaç |
| --- | --- | --- |
| 1935 | LAN (`:1935`) | DJI Fly RTMP ingest |
| 8554 | `127.0.0.1` | FFmpeg/OBS RTSP reader ve `/production` publisher |
| 8889 | `127.0.0.1` | WHEP preview |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX Control API |
| 9998 | `127.0.0.1` | MediaMTX metrics |

Tauri CSP yalnız gereken localhost WHEP adresine izin verir. Capability yalnız core ve video file picker ile sınırlıdır. `NSLocalNetworkUsageDescription` ve isteğe bağlı yerleşik commentary capture için `NSMicrophoneUsageDescription` vardır.

Non-secret config: `~/Library/Application Support/DJI Live Bridge/`  
Log: `~/Library/Logs/DJI Live Bridge/`  
Secret: macOS Keychain

## Doğrulama ve üretim build

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo check --locked --manifest-path src-tauri/Cargo.toml
npm run typecheck
npm run lint
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run build:mac
```

> **Build için tek geçerli komut `npm run build:mac`'tir.** Çıplak `tauri build` engellenmiştir (`scripts/tauri.sh`), çünkü Tauri, uygulamanın kısıtlı entitlement'larını (`system-extension.install`, `application-groups`) `mediamtx` sidecar'ına da uygular. Provisioning profile olmayan sidecar'ı macOS açılışta `SIGKILL` ile öldürür (exit 137); uygulama da `error sending request for url (http://127.0.0.1:9997/v3/paths/list)` hatasını verir. `build:mac` sidecar'ı boş entitlement ile yeniden imzalar, `.app` ve `.dmg` içindeki bundle'ı `scripts/verify-bundle.sh` ile doğrular ve doğrulanmamış bir build'i başarılı saymaz. Kurulu bir uygulamayı denetlemek için: `npm run verify:bundle -- "/Applications/DJI Live Bridge.app"`.

RTMP config serileştirme, secret ayrımı, URL doğrulama ve bir hedef çalışırken diğerinin hata vermesi senaryoları Rust unit testleriyle doğrulanır. Gerçek Instagram/TikTok hesabına yayın ayrıca geçerli platform anahtarlarıyla kullanıcı tarafından doğrulanmalıdır.

## Packaging, signing ve notarization

Tauri `externalBin` target-triple kuralı nedeniyle her hedef için doğru sidecar dosyası build öncesi bulunmalıdır. MediaMTX executable izni (`0755`) korunmalıdır. `.app` ve `.dmg` üretimi sidecar'ı içeri alır; dağıtılacak build'de Apple Developer ID signing ve notarization hem ana executable'ı hem sidecar'ı kapsamalıdır. Unsigned local build yalnız geliştirme/test içindir. `npm run build:mac` `.app` ve `.dmg`'yi notarytool keychain profili `dji-live-bridge` ile notarize edip staple eder; notarize edilmemiş Developer ID system extension'ı macOS etkinleştirmez (`code=8 … code signature invalid`), bu yüzden sanal kamera için notarization zorunludur.

Sidecar imzası `scripts/sign-sidecars.sh` ile `src-tauri/entitlements-sidecar.plist` (boş) kullanılarak düzeltilir; bu adım `npm run build:mac` içinde otomatik çalışır ve elle çağrılmamalıdır. DMG, Tauri'nin kendi DMG adımıyla değil, düzeltilmiş `.app`'ten `build:mac` tarafından üretilir; aksi halde DMG bozuk sidecar'ı taşırdı. Uygulama kapanırken (`RunEvent::Exit` dahil) tüm alt süreçleri durdurur ve başlarken kendi config dosyasıyla başlatılmış artık bir MediaMTX'i temizler.

`beforeBundleCommand`, Swift kamera uzantısını derleyip `Contents/Library/SystemExtensions/DJILiveBridgeCamera.systemextension` altına yerleştirir. Yerel build varsayılan olarak `signingIdentity: "-"` ile ad-hoc imzalanır. Üretimde `APPLE_SIGNING_IDENTITY` ortam değişkeni Tauri ayarının üzerine yazar ve uzantı da aynı kimliği kullanır. Üretim paketi ana uygulamanın `entitlements.plist` dosyasındaki System Extension + App Group yetkilerini ve uzantının aynı App Group yetkisini korumalıdır. Notarization sonrasında `codesign --verify --deep --strict` ve gerçek bir macOS kullanıcı-onaylı aktivasyon testi ayrıca yapılmalıdır.

FFmpeg bundle edilmediği için uygulama FFmpeg'i yeniden dağıtmaz. Kullanıcının kurulu build'inin LGPL/GPL seçenekleri ve codec lisansları o build'in dağıtıcısına aittir. İleride bundle edilirse binary provenance, configure flags, kaynak teklifi ve ilgili LGPL/GPL yükümlülükleri release bazında ayrıca tutulmalıdır.

## Lisans

MIT — bkz. [LICENSE](LICENSE).

FFmpeg (LGPL, [scripts/build-ffmpeg.sh](scripts/build-ffmpeg.sh) ile derlenir) uygulamayla birlikte gelir; lisans metinleri uygulama içindeki `Contents/Resources/licenses` klasöründedir.

## Hakkında

**Taner Özel** — Geliştirici

- E-posta: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

Bu proje DJI, Instagram veya TikTok ile bağlantılı değildir.
