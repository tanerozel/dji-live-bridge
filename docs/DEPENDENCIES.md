# Pinned external dependencies

Doğrulama tarihi: 2026-09-18.

| Bileşen | Pin / politika | Resmi kaynak |
| --- | --- | --- |
| Tauri Rust | 2.11.0 | https://crates.io/crates/tauri/2.11.0 |
| Tauri JS API | 2.11.1 | https://www.npmjs.com/package/@tauri-apps/api/v/2.11.1 |
| Tauri CLI | 2.11.4 | https://www.npmjs.com/package/@tauri-apps/cli/v/2.11.4 |
| Tauri sidecar kuralı | `externalBin` + target triple | https://v2.tauri.app/develop/sidecar/ |
| Rust stable | 1.98.1 | https://blog.rust-lang.org/ |
| MediaMTX | 1.21.0 | https://github.com/bluenviron/mediamtx/releases/tag/v1.21.0 |
| MediaMTX Control API | v3 paths API | https://mediamtx.org/docs/usage/control-api |
| MediaMTX forwarding | runtime path `forward` + status API | https://mediamtx.org/docs/usage/forward |
| MediaMTX metrics | path inbound byte counters | https://mediamtx.org/docs/usage/metrics |
| MediaMTX WHEP | `/<path>/whep` | https://mediamtx.org/docs/usage/read-a-stream#webrtc |
| OBS Studio | 32.2.2 | https://github.com/obsproject/obs-studio/releases/tag/32.2.2 |
| obs-websocket | 5.x, RPC 1; runtime `availableRequests` | https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md |
| OBS Virtual Camera | video camera behavior | https://obsproject.com/kb/virtual-camera-guide |
| FFmpeg | external; minimum 8.1.2, current pin record 9.0.1 | https://ffmpeg.org/download.html |
| FFmpeg AVFoundation | external device input capability | https://ffmpeg.org/ffmpeg-devices.html#avfoundation |
| macOS Camera Extension | Core Media I/O system extension contract | https://developer.apple.com/documentation/coremediaio/creating-a-camera-extension-with-core-media-i-o |
| System Extensions | `OSSystemExtensionRequest` activation and user approval | https://developer.apple.com/documentation/systemextensions |
| Swift compiler / macOS SDK | Xcode Command Line Tools; local extension build | https://developer.apple.com/xcode/resources/ |
| Bonjour `dns-sd` | macOS system binary; `live.local` proxy registration | `man dns-sd` |
| TikTok LIVE Studio | official page verified with macOS 12+ download on 2026-09-17 | https://www.tiktok.com/studio/download |

## MediaMTX provenance

Download script: `scripts/download-mediamtx.sh`  
Archive: `mediamtx_v1.21.0_darwin_arm64.tar.gz`  
SHA-256: `159b8e8164022189e654b61ee8bb7edf2d3ff9354ff06cf4c315bb59f5cc9eca`

Checksum, release'in resmi `checksums.sha256` dosyasından alınır ve her indirmede script tarafından karşılaştırılır. Intel archive'ının checksum'u build anında aynı resmi liste üzerinden çözülür; doğrulanmadan binary kurulmaz.

## FFmpeg lisans kararı

FFmpeg/ffprobe uygulamayla bundle edilmez ve process argv üzerinden dış dependency olarak kullanılır. Bu repository FFmpeg binary'si dağıtmadığı için belirli bir üçüncü taraf build'in GPL/nonfree configure seçeneklerini sahiplenmez. Uygulama başlangıç/diagnostics aşamasında binary ve encoder capability'lerini gerçek kurulumdan probe eder; VideoToolbox yoksa libx264 ancak kurulu FFmpeg bunu gerçekten ilan ediyorsa seçilir.
