# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · **Bahasa Indonesia** · [Italiano](README.it.md)

Aplikasi macOS yang menerima siaran RTMP dari DJI Fly di Mac pada jaringan yang sama, menampilkan pratinjau lokal lewat MediaMTX, dan memberi umpan ke rangkaian produksi FFmpeg bawaan yang tidak memerlukan OBS. Dari satu gambar Anda bisa siaran sekaligus ke Instagram, TikTok, dan tujuan RTMP mana pun, atau memakainya sebagai kamera virtual di TikTok LIVE Studio.

## Pemasangan

1. Unduh berkas `.dmg` terbaru dari [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Seret **DJI Live Bridge** ke folder **Applications**. Kamera virtual hanya bekerja dari sana.
3. Pasang FFmpeg yang tidak disertakan: `brew install ffmpeg`.

Aplikasi sudah ditandatangani dan dinotarisasi oleh Apple, jadi terbuka tanpa peringatan.

**Kebutuhan:** Mac dengan Apple Silicon, macOS 13 atau lebih baru, FFmpeg/ffprobe 8.1.2 atau lebih baru di `PATH`.

## Siaran ke Instagram / TikTok

Tab `Siaran` adalah satu layar dengan tiga langkah:

1. **Hubungkan drone Anda** — tempel alamat `rtmp://…/drone` yang ditampilkan ke DJI Fly (jalur: **GO FLY → Transmission → Live Streaming Platforms → RTMP**), atau pakai `Coba dengan berkas video`. Langkah ini berubah hijau saat gambar masuk.
2. **Mau siaran ke mana?** — pilih Instagram, TikTok, atau RTMP khusus, lalu tempel alamat server dan stream key dari platform. Di Instagram: instagram.com → Buat (+) → Video live. Instagram memberi kunci baru setiap siaran; perbarui lewat `Perbarui kunci` pada kartu tujuan.
3. **Gambar dan suara** — `Vertikal 9:16` (bawaan untuk Instagram dan TikTok), pembingkaian, dan mikrofon opsional. Pilihan Anda diingat.

Tombol `MULAI SIARAN` aktif begitu tiga syarat terpenuhi: server lokal, video drone, dan tujuan yang dipilih. Selama siaran Anda melihat durasi, status tiap tujuan, dan data yang terkirim; bila satu tujuan gagal, yang lain tetap berjalan. Di Instagram Anda masih harus menekan “Mulai live” di sana agar siaran menjadi publik.

Aplikasi tidak masuk ke akun platform mana pun dan tidak menekan layar konfirmasi untuk Anda.

## TikTok LIVE Studio (kamera virtual)

Jalur ini tidak butuh alamat server atau stream key:

1. Di tab `TikTok LIVE Studio`, tekan sekali `Aktifkan kamera virtual`.
2. Bila macOS meminta, izinkan ekstensi di **System Settings → General → Login Items & Extensions → Camera Extensions**.
3. Saat kartu menampilkan `Siap dimulai`, tekan `Mulai kamera virtual`.
4. Di TikTok LIVE Studio tambahkan sumber kamera dan pilih **DJI Live Bridge Camera**.
5. Atur audio capture sumber itu ke `None`, lalu pilih satu mikrofon saja di kontrol audio utama LIVE Studio.

Kamera ini sengaja hanya membawa video. Jika aplikasi di folder Applications diganti, macOS menonaktifkan ekstensinya: tekan `Mulai kamera virtual` sekali lagi dan tombol itu akan meminta aktivasi sendiri. “Go Live” tetap ditekan manual di TikTok LIVE Studio.

## Bahasa dan tema

Aplikasi tersedia dalam bahasa Inggris, Turki, Spanyol, Mandarin (aksara sederhana dan tradisional), Arab, Hindi, Portugis, Rusia, Prancis, Jerman, Jepang, Korea, Indonesia, dan Italia. Saat pertama dijalankan, aplikasi mengikuti bahasa sistem dan kembali ke bahasa Inggris bila tidak didukung. Dengan bahasa Arab seluruh antarmuka berubah menjadi kanan-ke-kiri, sementara nilai teknis (alamat, port, codec) tetap kiri-ke-kanan. Nama perangkat, codec, protokol, dan rincian galat asli tidak diterjemahkan agar diagnosis tetap akurat.

Ada lima tema: Sistem, Terang, Gelap, Tengah malam, dan Pasir.

## Kualitas dan keamanan

- Keluaran tetap 30 fps, 6 Mbps CBR, H.264 High, dan keyframe tiap 2 detik sesuai syarat penerimaan Instagram/Facebook. Encoder perangkat keras VideoToolbox diutamakan.
- Pada pembingkaian `Gambar utuh`, ruang kosong diisi salinan video yang diburamkan, bukan bilah hitam.
- Gambar hanya dienkode sekali, lalu MediaMTX meneruskannya ke setiap tujuan aktif secara terpisah.
- Setiap stream key disimpan di entri Keychain macOS tersendiri. Kunci tidak pernah masuk ke berkas konfigurasi, argumen FFmpeg, atau log.

| Port | Mendengarkan | Kegunaan |
| --- | --- | --- |
| 1935 | Jaringan lokal (`:1935`) | Masukan RTMP dari DJI Fly |
| 8554 | `127.0.0.1` | Pembaca RTSP dan penerbitan `/production` |
| 8889 | `127.0.0.1` | Pratinjau WHEP |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | Control API MediaMTX |
| 9998 | `127.0.0.1` | Metrik MediaMTX |

Konfigurasi: `~/Library/Application Support/DJI Live Bridge/` · Log: `~/Library/Logs/DJI Live Bridge/` · Rahasia: Keychain macOS

## Pengembangan

Rincian build, penandatanganan, dan notarisasi ada di [README bahasa Inggris](README.md#development).
