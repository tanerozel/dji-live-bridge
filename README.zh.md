# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · **中文（简体）** · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

一款 macOS 应用：在同一局域网的 Mac 上接收 DJI Fly 的 RTMP 推流，通过 MediaMTX 提供本地预览，并驱动无需 OBS 的内置 FFmpeg 制作流程。同一路画面可以同时推送到 Instagram、TikTok 和任意 RTMP 目标，也可以作为 TikTok LIVE Studio 的虚拟摄像头。

## 安装

1. 在 [Releases](https://github.com/tanerozel/dji-live-bridge/releases) 页面下载最新的 `.dmg` 文件。
2. 把 **DJI Live Bridge** 拖到 **应用程序** 文件夹。虚拟摄像头只有在该位置才能工作。
3. FFmpeg 未随应用附带。首次启动时，应用会发现缺少它，并通过 Homebrew 一键为你安装，同时显示安装日志。想自己装也可以：`brew install ffmpeg`。

应用已由 Apple 签名并公证，打开时不会出现警告。

**系统要求：** Apple 芯片的 Mac、macOS 13 或更高版本、`PATH` 中有 FFmpeg/ffprobe 8.1.2 或更高版本。

## 推流到 Instagram / TikTok

`直播` 标签页是一个界面、三个步骤：

1. **连接无人机** —— 把界面显示的 `rtmp://…/drone` 地址粘贴到 DJI Fly（路径：**GO FLY → 图传 → 直播平台 → RTMP**），或点击 `用视频文件测试`。画面到达后该步骤变绿。
2. **要推流到哪里？** —— 选择 Instagram、TikTok 或自定义 RTMP，粘贴平台给出的服务器地址和推流密钥。Instagram：instagram.com → 创建 (+) → 直播视频。Instagram 每次直播都会生成新密钥，可用目标卡片上的 `更新密钥` 刷新。
3. **画面与声音** —— `竖屏 9:16`（Instagram 和 TikTok 的默认值）、取景方式以及可选麦克风。选择会被记住。

当三项条件（本地服务器、无人机画面、已选目标）都满足时，`开始直播` 按钮会变为可用。直播中会显示时长、每个目标的状态以及已发送的数据量；某个目标失败不会影响其他目标。在 Instagram 上仍需在其页面点击“开始直播”，直播才会公开。

本应用不会登录任何平台账号，也不会替你点击平台的确认界面。

## TikTok LIVE Studio（虚拟摄像头）

这条路径不需要服务器地址或推流密钥：

1. 在 `TikTok LIVE Studio` 标签页点击一次 `启用虚拟摄像头`。
2. 如果 macOS 询问，请在 **系统设置 → 通用 → 登录项与扩展 → 摄像头扩展** 中允许该扩展。
3. 当卡片显示 `可以启动` 时，点击 `启动虚拟摄像头`。
4. 在 TikTok LIVE Studio 中添加摄像头源，选择 **DJI Live Bridge Camera**。
5. 把该来源的音频采集设为 `无`，并在 LIVE Studio 的主音频控件中只选择一个麦克风。

这个摄像头刻意只传输视频。如果替换了“应用程序”中的程序，macOS 会停用该扩展：再次点击 `启动虚拟摄像头` 即可，按钮会自动重新申请启用。“开始直播”仍需在 TikTok LIVE Studio 中手动点击。

## 语言与主题

应用提供英语、土耳其语、西班牙语、中文、阿拉伯语、印地语、葡萄牙语、俄语、法语、德语和日语。首次启动时跟随系统语言，不支持时回退到英语。选择阿拉伯语时整个界面切换为从右到左，而网址、端口、编解码器等技术内容仍保持从左到右。设备名称、编解码器、协议和原始错误信息不作翻译，以保证诊断准确。

共有五种主题：跟随系统、浅色、深色、午夜蓝和沙色。

## 画质与安全

- 输出固定 30 fps、6 Mbps CBR、H.264 High，关键帧间隔 2 秒，符合 Instagram/Facebook 的接收要求。优先使用 VideoToolbox 硬件编码器。
- 使用 `完整画面` 取景时，空白区域由视频的模糊副本填充，而不是黑边。
- 画面只编码一次，再由 MediaMTX 分别转发给每个启用的目标。
- 每个推流密钥都保存在各自的 macOS 钥匙串条目中。密钥不会进入配置文件、FFmpeg 参数或日志。

| 端口 | 监听 | 用途 |
| --- | --- | --- |
| 1935 | 局域网 (`:1935`) | 接收 DJI Fly 的 RTMP |
| 8554 | `127.0.0.1` | RTSP 读取与 `/production` 发布 |
| 8889 | `127.0.0.1` | WHEP 预览 |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX 控制 API |
| 9998 | `127.0.0.1` | MediaMTX 指标 |

配置：`~/Library/Application Support/DJI Live Bridge/` · 日志：`~/Library/Logs/DJI Live Bridge/` · 密钥：macOS 钥匙串

## 许可证

MIT — 见 [LICENSE](LICENSE)。

## 开发

构建、签名和公证的细节请见[英文 README](README.md#development)。

## 关于

**Taner Özel** — 开发者

- 邮箱: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

本项目与 DJI、Instagram、TikTok 没有隶属关系。
