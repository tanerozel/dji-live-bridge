# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · **中文（繁體）** · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

**網站：** https://tanerozel.github.io/dji-live-bridge/zh-Hant/

一款 macOS 應用程式：在同一區域網路的 Mac 上接收 DJI Fly 的 RTMP 推流，透過 MediaMTX 提供本機預覽，並驅動不需要 OBS 的內建 FFmpeg 製作流程。同一個畫面可以同時推送到 Instagram、TikTok 與任何 RTMP 目的地，也能當成 TikTok LIVE Studio 的虛擬攝影機。

## 安裝

1. 到 [Releases](https://github.com/tanerozel/dji-live-bridge/releases) 下載最新的 `.dmg` 檔案。
2. 把 **DJI Live Bridge** 拖到 **應用程式** 資料夾。虛擬攝影機只有放在該位置才能運作。
3. 就這樣。FFmpeg 已隨應用程式附帶，不需要再安裝其他東西。

應用程式已由 Apple 簽署並完成公證，開啟時不會出現警告。

**系統需求：** Apple 晶片的 Mac、macOS 13 以上、`PATH` 中有 FFmpeg/ffprobe 8.1.2 以上。

## 推流到 Instagram / TikTok

`直播` 分頁是一個畫面、三個步驟：

1. **連接無人機** — 把畫面顯示的 `rtmp://…/drone` 位址貼到 DJI Fly（路徑：**GO FLY → 圖傳 → 直播平台 → RTMP**），或按 `用影片檔測試`。畫面送達後這個步驟會變綠色。
2. **要直播到哪裡？** — 選擇 Instagram、TikTok 或自訂 RTMP，貼上平台提供的伺服器位址與串流金鑰。Instagram：instagram.com → 建立 (+) → 直播影片。Instagram 每次直播都會給新的金鑰，可用目的地卡片上的 `更新金鑰` 更換。
3. **畫面與聲音** — `直向 9:16`（Instagram 與 TikTok 的預設值）、取景方式，以及選用的麥克風。設定會被記住。

當三項條件（本機伺服器、無人機畫面、已選目的地）都滿足時，`開始直播` 按鈕就會啟用。直播期間會顯示時間、各目的地狀態與已傳送的資料量；其中一個目的地失敗不會影響其他目的地。在 Instagram 仍需在該平台按下「開始直播」，直播才會公開。

這個應用程式不會登入任何平台帳號，也不會替你點選平台的確認畫面。

## TikTok LIVE Studio（虛擬攝影機）

這條路徑不需要伺服器位址或串流金鑰：

1. 在 `TikTok LIVE Studio` 分頁按一次 `啟用虛擬攝影機`。
2. 若 macOS 詢問，請到 **系統設定 → 一般 → 登入項目與延伸功能 → 攝影機延伸功能** 允許。
3. 當卡片顯示 `可以啟動` 時，按下 `啟動虛擬攝影機`。
4. 在 TikTok LIVE Studio 新增攝影機來源，選擇 **DJI Live Bridge Camera**。
5. 把該來源的音訊擷取設為 `無`，並在 LIVE Studio 的主音訊控制中只選一個麥克風。

這個攝影機刻意只傳送影像。如果更換了「應用程式」資料夾中的程式，macOS 會停用該延伸功能：再按一次 `啟動虛擬攝影機` 即可，按鈕會自動重新要求啟用。「開始直播」仍需在 TikTok LIVE Studio 手動按下。

## 語言與主題

應用程式提供英文、土耳其文、西班牙文、中文（簡體與繁體）、阿拉伯文、印地文、葡萄牙文、俄文、法文、德文、日文、韓文、印尼文與義大利文。首次啟動時會跟隨系統語言，若不支援則改用英文。選擇阿拉伯文時整個介面會轉為由右至左，而網址、連接埠、編解碼器等技術內容仍維持由左至右。裝置名稱、編解碼器、通訊協定與原始錯誤內容不會翻譯，以確保診斷準確。

共有五種主題：跟隨系統、淺色、深色、午夜藍與沙色。

## 畫質與安全

- 輸出固定 30 fps、6 Mbps CBR、H.264 High，關鍵影格間隔 2 秒，符合 Instagram/Facebook 的接收需求。優先使用 VideoToolbox 硬體編碼器。
- 使用 `完整畫面` 取景時，空白區域會用影片的模糊副本填滿，而不是黑邊。
- 畫面只編碼一次，再由 MediaMTX 分別轉送到每個啟用的目的地。
- 每個串流金鑰都存放在各自的 macOS 鑰匙圈項目中。金鑰不會進入設定檔、FFmpeg 參數或記錄檔。

| 連接埠 | 接聽 | 用途 |
| --- | --- | --- |
| 1935 | 區域網路 (`:1935`) | 接收 DJI Fly 的 RTMP |
| 8554 | `127.0.0.1` | RTSP 讀取與 `/production` 發佈 |
| 8889 | `127.0.0.1` | WHEP 預覽 |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX 控制 API |
| 9998 | `127.0.0.1` | MediaMTX 指標 |

設定：`~/Library/Application Support/DJI Live Bridge/` · 記錄：`~/Library/Logs/DJI Live Bridge/` · 機密：macOS 鑰匙圈

## 授權

MIT — 見 [LICENSE](LICENSE)。

隨附的 FFmpeg 為 LGPL 版本；授權文字位於應用程式內的 `Contents/Resources/licenses`。

## 開發

建置、簽署與公證的細節請見[英文 README](README.md#development)。

## 關於

**Taner Özel** — 開發者

- 電子郵件: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

本專案與 DJI、Instagram、TikTok 沒有隸屬關係。
