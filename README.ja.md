# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · **日本語** · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

**ウェブサイト：** https://tanerozel.github.io/dji-live-bridge/ja/

同じネットワーク上の Mac で DJI Fly の RTMP 配信を受け取り、MediaMTX 経由でローカルプレビューを表示し、OBS 不要の内蔵 FFmpeg 配信パイプラインへ渡す macOS アプリです。1 つの映像から Instagram、TikTok、任意の RTMP 配信先へ同時に配信でき、TikTok LIVE Studio の仮想カメラとしても使えます。

## インストール

1. [Releases](https://github.com/tanerozel/dji-live-bridge/releases) から最新の `.dmg` をダウンロードします。
2. **DJI Live Bridge** を **アプリケーション** フォルダへドラッグします。仮想カメラはこの場所からのみ動作します。
3. これだけです。FFmpeg はアプリに同梱されているので、ほかに入れるものはありません。

アプリは Apple の署名と公証を受けているため、警告なしで開きます。

**必要環境：** Apple シリコンの Mac、macOS 13 以降、`PATH` に FFmpeg/ffprobe 8.1.2 以降。

## Instagram / TikTok へ配信する

`配信` タブは 1 画面・3 ステップです。

1. **ドローンを接続** — 表示された `rtmp://…/drone` のアドレスを DJI Fly に貼り付けます（経路：**GO FLY → 伝送 → ライブ配信プラットフォーム → RTMP**）。`動画ファイルで試す` でも確認できます。映像が届くとステップが緑になります。
2. **どこへ配信しますか？** — Instagram、TikTok、カスタム RTMP から選び、配信先のサーバーアドレスとストリームキーを貼り付けます。Instagram の場合：instagram.com →「作成（＋）」→「ライブ動画」。Instagram は配信ごとに新しいキーを発行するので、配信先カードの `キーを更新` で更新してください。
3. **映像と音声** — `縦 9:16`（Instagram と TikTok の既定値）、画角、任意のマイク。設定は記憶されます。

3 つの条件（ローカルサーバー、ドローン映像、選択済みの配信先）がそろうと `配信を開始` ボタンが有効になります。配信中は経過時間、各配信先の状態、送信済みデータ量が表示され、1 つの配信先が失敗しても他は継続します。Instagram では公開するために、Instagram 側で「ライブを開始」を押す必要があります。

このアプリが各プラットフォームにログインしたり、確認画面を代わりに操作したりすることはありません。

## TikTok LIVE Studio（仮想カメラ）

この方法ではサーバーアドレスもストリームキーも不要です。

1. `TikTok LIVE Studio` タブで `仮想カメラを有効にする` を一度押します。
2. macOS が確認したら、**システム設定 → 一般 → ログイン項目と機能拡張 → カメラ機能拡張** で許可します。
3. カードが `開始できます` になったら `仮想カメラを開始` を押します。
4. TikTok LIVE Studio でカメラソースを追加し、**DJI Live Bridge Camera** を選びます。
5. そのソースの音声取り込みを `なし` にし、LIVE Studio のメイン音声設定でマイクを 1 つだけ選びます。

このカメラは意図的に映像のみを送ります。「アプリケーション」内のアプリを入れ替えると macOS が機能拡張を無効にするため、その後もう一度 `仮想カメラを開始` を押してください。ボタンが自動で有効化を要求します。「Go Live」は TikTok LIVE Studio 側で手動のままです。

## 言語とテーマ

英語、トルコ語、スペイン語、中国語、アラビア語、ヒンディー語、ポルトガル語、ロシア語、フランス語、ドイツ語、日本語に対応しています。初回起動時はシステムの言語に従い、未対応の場合は英語になります。アラビア語では画面全体が右から左になり、URL・ポート・コーデックなどの技術的な値は左から右のままです。機器名、コーデック、プロトコル、エラーの生データは診断の正確さのため翻訳しません。

テーマは 5 種類：システムに合わせる、ライト、ダーク、ミッドナイト、サンド。

## 画質とセキュリティ

- 出力は常時 30 fps、6 Mbps CBR、H.264 High、キーフレーム 2 秒間隔で、Instagram／Facebook の受け入れ要件に合わせています。ハードウェアエンコーダー VideoToolbox を優先します。
- `全体を表示` の画角では、余白を黒帯ではなく映像のぼかしコピーで埋めます。
- 映像のエンコードは 1 回だけ行い、MediaMTX が有効な配信先ごとに転送します。
- ストリームキーはそれぞれ macOS キーチェーンの個別項目に保存されます。キーが設定ファイル、FFmpeg の引数、ログに出ることはありません。

| ポート | 待ち受け | 用途 |
| --- | --- | --- |
| 1935 | ローカルネットワーク (`:1935`) | DJI Fly からの RTMP 受信 |
| 8554 | `127.0.0.1` | RTSP 読み出しと `/production` の配信 |
| 8889 | `127.0.0.1` | WHEP プレビュー |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX コントロール API |
| 9998 | `127.0.0.1` | MediaMTX のメトリクス |

設定：`~/Library/Application Support/DJI Live Bridge/` · ログ：`~/Library/Logs/DJI Live Bridge/` · 機密情報：macOS キーチェーン

## ライセンス

MIT — [LICENSE](LICENSE) を参照。

同梱の FFmpeg は LGPL ビルドです。ライセンス文書はアプリ内の `Contents/Resources/licenses` にあります。

## 開発

ビルド、署名、公証の詳細は[英語版 README](README.md#development) にあります。

## このプロジェクトについて

**Taner Özel** — 開発者

- メール: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

本プロジェクトは DJI、Instagram、TikTok とは無関係です。
