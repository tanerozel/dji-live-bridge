# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · **한국어** · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

같은 네트워크의 Mac에서 DJI Fly의 RTMP 송출을 받아 MediaMTX로 로컬 미리보기를 보여주고, OBS 없이도 동작하는 내장 FFmpeg 방송 파이프라인에 전달하는 macOS 앱입니다. 하나의 화면으로 Instagram, TikTok, 임의의 RTMP 대상에 동시에 방송하거나, TikTok LIVE Studio의 가상 카메라로 사용할 수 있습니다.

## 설치

1. [Releases](https://github.com/tanerozel/dji-live-bridge/releases)에서 최신 `.dmg`를 내려받으세요.
2. **DJI Live Bridge**를 **응용 프로그램** 폴더로 끌어다 놓으세요. 가상 카메라는 이 위치에서만 동작합니다.
3. FFmpeg는 앱에 포함되어 있지 않습니다. 처음 실행하면 앱이 이를 감지해 Homebrew로 한 번에 설치해 주고, 진행 로그도 함께 보여 줍니다. 직접 설치하려면 `brew install ffmpeg`.

앱은 Apple의 서명과 공증을 받았으므로 경고 없이 열립니다.

**필요 사항:** Apple Silicon Mac, macOS 13 이상, `PATH`에 FFmpeg/ffprobe 8.1.2 이상.

## Instagram / TikTok으로 방송하기

`방송` 탭은 한 화면에 세 단계로 구성됩니다.

1. **드론 연결** — 화면에 표시된 `rtmp://…/drone` 주소를 DJI Fly에 붙여넣으세요(경로: **GO FLY → 전송 → 라이브 방송 플랫폼 → RTMP**). 또는 `동영상 파일로 시험`을 사용하세요. 영상이 들어오면 이 단계가 초록색으로 바뀝니다.
2. **어디로 방송할까요?** — Instagram, TikTok, 사용자 지정 RTMP 중에서 고르고 플랫폼이 준 서버 주소와 스트림 키를 붙여넣으세요. Instagram은 instagram.com → 만들기(+) → 라이브 방송입니다. Instagram은 방송할 때마다 새 키를 주므로 대상 카드의 `키 업데이트`로 바꾸세요.
3. **화면과 소리** — `세로 9:16`(Instagram과 TikTok의 기본값), 화면 구성, 선택 사항인 마이크. 설정은 저장됩니다.

세 가지 조건(로컬 서버, 드론 영상, 선택된 대상)이 충족되면 `방송 시작` 버튼이 활성화됩니다. 방송 중에는 경과 시간, 각 대상의 상태, 전송된 데이터가 표시되며, 한 대상이 실패해도 나머지는 계속 방송합니다. Instagram에서는 방송을 공개하려면 그쪽에서 '라이브 시작'을 눌러야 합니다.

이 앱은 어떤 플랫폼에도 로그인하지 않으며, 플랫폼의 확인 화면을 대신 눌러주지 않습니다.

## TikTok LIVE Studio (가상 카메라)

이 방법에는 서버 주소나 스트림 키가 필요 없습니다.

1. `TikTok LIVE Studio` 탭에서 `가상 카메라 활성화`를 한 번 누르세요.
2. macOS가 물으면 **시스템 설정 → 일반 → 로그인 항목 및 확장 프로그램 → 카메라 확장 프로그램**에서 허용하세요.
3. 카드에 `시작할 수 있음`이 표시되면 `가상 카메라 시작`을 누르세요.
4. TikTok LIVE Studio에서 카메라 소스를 추가하고 **DJI Live Bridge Camera**를 선택하세요.
5. 해당 소스의 오디오 캡처를 `없음`으로 설정하고, LIVE Studio의 기본 오디오 설정에서 마이크를 하나만 고르세요.

이 카메라는 의도적으로 영상만 전달합니다. 응용 프로그램 폴더의 앱을 교체하면 macOS가 확장 프로그램을 비활성화하므로, 이후 `가상 카메라 시작`을 다시 누르면 버튼이 직접 활성화를 요청합니다. 'Go Live'는 TikTok LIVE Studio에서 직접 눌러야 합니다.

## 언어와 테마

영어, 터키어, 스페인어, 중국어(간체·번체), 아랍어, 힌디어, 포르투갈어, 러시아어, 프랑스어, 독일어, 일본어, 한국어, 인도네시아어, 이탈리아어를 지원합니다. 처음 실행하면 시스템 언어를 따르고, 지원하지 않으면 영어로 돌아갑니다. 아랍어를 고르면 화면 전체가 오른쪽에서 왼쪽으로 바뀌지만 주소, 포트, 코덱 같은 기술 값은 왼쪽에서 오른쪽으로 유지됩니다. 기기 이름, 코덱, 프로토콜, 원본 오류 내용은 진단 정확도를 위해 번역하지 않습니다.

테마는 다섯 가지입니다: 시스템, 밝게, 어둡게, 미드나이트, 샌드.

## 품질과 보안

- 출력은 30 fps 고정, 6 Mbps CBR, H.264 High, 2초 간격 키프레임으로 Instagram/Facebook 수신 요구에 맞춥니다. 하드웨어 인코더 VideoToolbox를 우선 사용합니다.
- `전체 화면 표시` 구성에서는 빈 공간을 검은 띠 대신 영상의 흐린 복사본으로 채웁니다.
- 화면은 한 번만 인코딩되며, MediaMTX가 각 대상에 따로 전달합니다.
- 모든 스트림 키는 macOS 키체인의 개별 항목에 저장됩니다. 키는 설정 파일, FFmpeg 인자, 로그에 절대 남지 않습니다.

| 포트 | 바인딩 | 용도 |
| --- | --- | --- |
| 1935 | 로컬 네트워크 (`:1935`) | DJI Fly RTMP 수신 |
| 8554 | `127.0.0.1` | RTSP 읽기 및 `/production` 송출 |
| 8889 | `127.0.0.1` | WHEP 미리보기 |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX 제어 API |
| 9998 | `127.0.0.1` | MediaMTX 지표 |

설정: `~/Library/Application Support/DJI Live Bridge/` · 로그: `~/Library/Logs/DJI Live Bridge/` · 비밀 정보: macOS 키체인

## 라이선스

MIT — [LICENSE](LICENSE) 참고.

## 개발

빌드, 서명, 공증에 대한 자세한 내용은 [영문 README](README.md#development)에 있습니다.

## 프로젝트 정보

**Taner Özel** — 개발자

- 이메일: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

이 프로젝트는 DJI, Instagram, TikTok과 제휴 관계가 없습니다.
