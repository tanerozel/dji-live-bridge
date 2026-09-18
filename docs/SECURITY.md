# Security model

- MediaMTX Control API, metrics, RTSP, WHEP ve ICE listener'ları loopback ile sınırlıdır. Yalnız RTMP ingest LAN'a açılır.
- Shell interpolation kullanılmaz; process'ler executable + argv ile başlatılır.
- RTMP destination URL'leri parse edilir, yalnız `rtmp` / `rtmps` şemaları ve geçerli host kabul edilir.
- Stream key ve OBS WebSocket password macOS Keychain'de tutulur. Stream key yalnız loopback MediaMTX Control API'ye runtime forward olarak gönderilir ve stop/shutdown sırasında silinir; FFmpeg argv'sine yazılmaz.
- Loglarda query-style secret'lara ek olarak RTMP URL fragment (`#stream-key`) redaction uygulanır; process arg snapshot'ı maskelenir.
- Runtime MediaMTX config yalnız uygulama support dizinine yazılır ve process başlamadan `--validate-conf` ile doğrulanır.
- Test Drone yalnız file picker'ın döndürdüğü mevcut regular file'ı kabul eder; kullanıcı girdisi path segmenti olarak birleştirilmez.
- Process shutdown önce SIGTERM uygular, grace timeout sonrası force kill kullanır; app'in başlattığı child süreçler supervisor tarafından sahiplenilir.
- Uygulama yalnız kullanıcı bir commentary cihazı seçerse FFmpeg AVFoundation üzerinden mikrofon capture eder; bu nedenle macOS permission metni bulunur. Varsayılan drone-audio-only path capture başlatmaz.
- Yerleşik sanal kamera yalnız video taşır. FFmpeg ham NV12 kareleri yalnızca `127.0.0.1:49213` üzerinde dinleyen yerel TCP akışına yazar; Camera Extension bu loopback akışına istemci olarak bağlanır. Akış harici ağ arayüzlerine açılmaz.
- Camera Extension yalnız imzalı ana uygulama paketinden `OSSystemExtensionRequest` ile etkinleştirilir. Uygulama `/Applications` dışında çalışıyorsa istek gönderilmez; macOS kullanıcı/yönetici onayı atlanmaz.
- TikTok private API, login automation, credential scraping, bypass ve UI click automation yoktur.
