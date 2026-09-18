# DJI Live Bridge

DJI Fly'ın RTMP yayınını aynı LAN'daki Mac'te karşılayan, MediaMTX üzerinden yerel preview'a ve OBS gerektirmeyen yerleşik FFmpeg production hattına dağıtan macOS uygulaması. Rust yalnız süreç, durum, güvenlik ve orchestration yapar; video frame'leri Rust içinden geçmez. OBS isteğe bağlı gelişmiş entegrasyondur.

## Çalışan vertical slice

1. Uygulama aktif default-route IPv4 arayüzünü bulur; loopback, link-local ve `utun` benzeri arayüzleri varsayılan seçimden çıkarır.
2. Paketlenmiş MediaMTX'i doğrular ve başlatır.
3. DJI RC 2 için `rtmp://<LAN_IPV4>:1935/drone` adresini ve QR kodunu gösterir.
4. `/drone` publisher durumunu MediaMTX Control API'den izler; reconnect için MediaMTX'i yeniden başlatmaz.
5. Direct WHEP preview'ı dener. Codec/ICE sorunu veya AAC preview sesi gerektiğinde, yalnız `drone-preview` için H.264 + Opus FFmpeg fallback'i açabilir.
6. Yerleşik production hattı FFmpeg capability probe sonucuna göre VideoToolbox/libx264 ile layout uygular; drone sesi ile isteğe bağlı AVFoundation mikrofonunu miksler ve `/production` yolunu üretir.
7. Direct RTMP yalnız `START LIVE` ile başlar. Stream key Keychain'den belleğe alınır, loopback Control API üzerinden geçici MediaMTX forward'a verilir ve `STOP LIVE` ile silinir; FFmpeg argv'sine yazılmaz.
8. Yerleşik Core Media I/O Camera Extension, `/drone` videosunu `DJI Live Bridge Camera` adıyla 1080×1920/30 fps video-only kamera olarak sunar. TikTok LIVE Studio mikrofonu doğrudan kendi içinden seçer; OBS gerekmez.
9. İsteğe bağlı OBS WebSocket 5.x entegrasyonu `GetVersion.availableRequests` ile runtime feature detection yapar; scene, recording ve OBS Virtual Camera özellikleri OBS kuruluysa kullanılabilir.

DJI Fly yolu: **GO FLY → Transmission → Live Streaming Platforms → RTMP**. RC 2 / DJI Fly 1.16+ yayın başlatmak için RC 2'ye bağlı ayrı bir mikrofon isteyebilir. Bu mikrofon Mac/OBS commentary mikrofonu değildir.

RC 2 girdisi 720p olabilir ve UI gerçek ffprobe metadata'sını gösterir. 1080p veya portrait production layout seçimi 720p girdiyi upscale eder; uygulama bunu native 1080p input olarak sunmaz.

## Gereksinimler

- macOS 13 veya üzeri, arm64 öncelikli; Intel build yolu korunur.
- Rust 1.98.1 (`rust-toolchain.toml`).
- Node.js 22 ve npm.
- OBS Studio 32.2.2 + obs-websocket 5.x yalnız isteğe bağlı advanced path içindir; varsayılan Direct RTMP hattında OBS gerekmez.
- FFmpeg/ffprobe dış bağımlılık olarak PATH üzerinde. Minimum desteklenen sürüm 8.1.2; geliştirme doğrulaması 8.1 ile yapıldı. Binary bundle edilmez.
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
     → MediaMTX runtime forward → TikTok/custom RTMP
```

Fake `connected` durumu üretilmez. FFmpeg/ffprobe bulunamazsa özellik açıkça unavailable olur.

## TikTok LIVE Studio — OBS'siz kullanım

Bu akışta TikTok server URL veya stream key gerekmez:

1. İmzalı `DJI Live Bridge.app` dosyasını `/Applications` klasörüne taşı ve oradan aç.
2. Mac ve DJI kumandayı aynı telefon hotspot'una bağla. DJI Fly yayın adresi olarak önce `rtmp://live.local/drone` kullan; kumanda bu adı çözemezse uygulamadaki IP fallback adresini kullan.
3. DJI Fly yayınını başlat ya da uygulamadaki `Start Test Drone` ile görüntüyü doğrula.
4. Destination bölümünde `LIVE Studio` seç ve bir kez `Enable virtual camera` düğmesine bas.
5. macOS isterse **System Settings → Privacy & Security** altında sistem uzantısına izin ver. Durum `Ready` olunca `Start Virtual Camera` düğmesine bas.
6. TikTok LIVE Studio içinde yeni bir Camera kaynağı ekleyip **DJI Live Bridge Camera** seç.
7. Camera kaynağının `More settings → Audio capture` ayarını `None` yap.
8. TikTok LIVE Studio ana mikrofon denetiminden yalnızca bir fiziksel mikrofon seç. Bu kamera bilerek yalnız video taşır; DJI Live Bridge bu modda ses yakalamaz veya TikTok'a ses göndermez.
9. Yayın sesini dinleyeceksen hoparlör yerine kulaklık kullan; böylece mikrofonun hoparlör sesini yeniden alması engellenir.

Uygulama macOS `dns-sd` ile `_rtmp._tcp.local` servisini ve `live.local` IPv4 kaydını yayınlar. Seçili LAN/hotspot IP'si değiştiğinde Bonjour kaydı otomatik yenilenir. IP tabanlı `rtmp://<LAN-IP>:1935/drone` adresi her zaman yedek olarak gösterilir.

Görüntü yolu şöyledir:

```text
DJI / Test Drone → MediaMTX /drone → FFmpeg NV12 1080×1920@30
                 → Localhost NV12 stream → Core Media I/O Camera Extension
                 → DJI Live Bridge Camera → TikTok LIVE Studio
```

Kamera etkinleştirme macOS güvenlik modelinin parçasıdır; uygulamanın `/Applications` altında bulunması ve ilk kullanımda yönetici onayı gerekir. `Go Live` işlemi TikTok LIVE Studio içinde manuel kalır.

## Yerleşik production ve yayın davranışı

- Varsayılan engine `NativeFfmpeg`'dir. Landscape `1920x1080`, TikTok Portrait `1080x1920`; framing varsayılan `Fit` ve crop yoktur.
- Mikrofon AVFoundation üzerinden yalnız kullanıcı seçerse capture edilir. Volume, non-negative sync delay, FFT noise reduction, compressor ve limiter capability probe ile uygulanır. macOS Microphone izni o zaman gerekir.
- Drone ses track'i yoksa ve mic seçilmediyse geçerli AAC stereo silence üretilir. Mic seçiliyse drone sesiyle `amix` üzerinden karıştırılır.
- Recording, yayın aktifse gerçek `/production` mix'ini; değilse `/drone` akışını stream-copy MKV olarak `~/Movies/DJI Live Bridge/` altına yazar.
- TikTok/custom stream key yalnız macOS Keychain'de kalır. Uzak destination MediaMTX Control API ile runtime'da eklenir; config dosyasına ve child process argv'sine girmez.
- MediaMTX forward `state`, `lastError` ve `outboundBytes` alanları gerçek Control API verisi olarak UI state'ine taşınır.

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
npm run tauri -- build --target aarch64-apple-darwin
```

Kullanıcının isteği doğrultusunda bu aşamada yeni unit/integration test dosyası eklenmedi; gerçek Test Drone hattı canlı çalıştırılarak doğrulandı.

## Packaging, signing ve notarization

Tauri `externalBin` target-triple kuralı nedeniyle her hedef için doğru sidecar dosyası build öncesi bulunmalıdır. MediaMTX executable izni (`0755`) korunmalıdır. `.app` ve `.dmg` üretimi sidecar'ı içeri alır; dağıtılacak build'de Apple Developer ID signing ve notarization hem ana executable'ı hem sidecar'ı kapsamalıdır. Unsigned local build yalnız geliştirme/test içindir.

`beforeBundleCommand`, Swift kamera uzantısını derleyip `Contents/Library/SystemExtensions/DJILiveBridgeCamera.systemextension` altına yerleştirir. Yerel build varsayılan olarak `signingIdentity: "-"` ile ad-hoc imzalanır. Üretimde `APPLE_SIGNING_IDENTITY` ortam değişkeni Tauri ayarının üzerine yazar ve uzantı da aynı kimliği kullanır. Üretim paketi ana uygulamanın `entitlements.plist` dosyasındaki System Extension + App Group yetkilerini ve uzantının aynı App Group yetkisini korumalıdır. Notarization sonrasında `codesign --verify --deep --strict` ve gerçek bir macOS kullanıcı-onaylı aktivasyon testi ayrıca yapılmalıdır.

FFmpeg bundle edilmediği için uygulama FFmpeg'i yeniden dağıtmaz. Kullanıcının kurulu build'inin LGPL/GPL seçenekleri ve codec lisansları o build'in dağıtıcısına aittir. İleride bundle edilirse binary provenance, configure flags, kaynak teklifi ve ilgili LGPL/GPL yükümlülükleri release bazında ayrıca tutulmalıdır.
