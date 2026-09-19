# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · **Deutsch** · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

macOS-App, die den RTMP-Stream von DJI Fly auf einem Mac im selben Netzwerk empfängt, über MediaMTX eine lokale Vorschau zeigt und eine integrierte FFmpeg-Produktionskette speist, die ohne OBS auskommt. Aus einem einzigen Bild sendest du gleichzeitig zu Instagram, TikTok und jedem RTMP-Ziel — oder nutzt es als virtuelle Kamera in TikTok LIVE Studio.

## Installation

1. Lade die neueste `.dmg`-Datei unter [Releases](https://github.com/tanerozel/dji-live-bridge/releases) herunter.
2. Ziehe **DJI Live Bridge** in den Ordner **Programme**. Die virtuelle Kamera funktioniert nur von dort.
3. Installiere FFmpeg, das nicht mitgeliefert wird: `brew install ffmpeg`.

Die App ist von Apple signiert und notarisiert und öffnet sich ohne Warnungen.

**Voraussetzungen:** Mac mit Apple Silicon, macOS 13 oder neuer, FFmpeg/ffprobe 8.1.2 oder neuer im `PATH`.

## Zu Instagram / TikTok streamen

Der Tab `Live gehen` ist ein einziger Bildschirm mit drei Schritten:

1. **Drohne verbinden** — füge die angezeigte Adresse `rtmp://…/drone` in DJI Fly ein (Pfad: **GO FLY → Übertragung → Livestream-Plattformen → RTMP**) oder nutze `Mit einer Videodatei testen`. Der Schritt wird grün, sobald das Bild ankommt.
2. **Wohin möchtest du streamen?** — wähle Instagram, TikTok oder eigenes RTMP und füge Serveradresse und Stream-Schlüssel der Plattform ein. Bei Instagram: instagram.com → Erstellen (+) → Live-Video. Instagram vergibt für jeden Stream einen neuen Schlüssel; aktualisiere ihn über `Schlüssel aktualisieren` auf der Zielkarte.
3. **Bild und Ton** — `Hochformat 9:16` (Standard für Instagram und TikTok), Bildausschnitt und optionales Mikrofon. Die Einstellungen werden gemerkt.

Die Taste `LIVE STARTEN` wird aktiv, sobald die drei Bedingungen erfüllt sind (lokaler Server, Drohnenbild, ausgewähltes Ziel). Während des Streams siehst du die Laufzeit, den Zustand jedes Ziels und die gesendete Datenmenge; fällt ein Ziel aus, laufen die anderen weiter. Bei Instagram musst du dort zusätzlich auf „Live gehen“ drücken, damit der Stream öffentlich wird.

Die App meldet sich bei keinem Konto an und klickt keine Bestätigungsdialoge der Plattform für dich weg.

## TikTok LIVE Studio (virtuelle Kamera)

Dieser Weg braucht weder Serveradresse noch Stream-Schlüssel:

1. Drücke im Tab `TikTok LIVE Studio` einmal auf `Virtuelle Kamera aktivieren`.
2. Wenn macOS fragt, erlaube die Erweiterung unter **Systemeinstellungen → Allgemein → Anmeldeobjekte & Erweiterungen → Kameraerweiterungen**.
3. Sobald die Karte `Startbereit` anzeigt, drücke auf `Virtuelle Kamera starten`.
4. Füge in TikTok LIVE Studio eine Kameraquelle hinzu und wähle **DJI Live Bridge Camera**.
5. Stelle die Audioaufnahme dieser Quelle auf `Keine` und wähle im Haupt-Audioregler von LIVE Studio genau ein Mikrofon.

Diese Kamera überträgt bewusst nur Video. Wenn du die App in /Programme ersetzt, deaktiviert macOS die Erweiterung: Drücke danach erneut auf `Virtuelle Kamera starten` — die Taste fordert die Aktivierung selbst wieder an. „Go Live“ bleibt in TikTok LIVE Studio manuell.

## Sprachen und Designs

Die App gibt es auf Englisch, Türkisch, Spanisch, Chinesisch, Arabisch, Hindi, Portugiesisch, Russisch, Französisch, Deutsch und Japanisch. Beim ersten Start folgt sie der Systemsprache und fällt sonst auf Englisch zurück. Bei Arabisch wechselt die gesamte Oberfläche nach rechts-nach-links, während technische Werte (URLs, Ports, Codecs) von links nach rechts bleiben. Gerätenamen, Codecs, Protokolle und rohe Fehlerdetails werden nicht übersetzt, damit die Diagnose genau bleibt.

Es gibt fünf Designs: System, Hell, Dunkel, Mitternacht und Sand.

## Qualität und Sicherheit

- Konstante Ausgabe mit 30 fps, 6 Mbit/s CBR, H.264 High und Keyframes alle 2 Sekunden, wie es die Instagram-/Facebook-Annahme verlangt. Der Hardware-Encoder VideoToolbox wird bevorzugt.
- Beim Bildausschnitt `Ganzes Bild` wird der leere Bereich mit einer unscharfen Kopie des Videos gefüllt statt mit schwarzen Balken.
- Das Bild wird einmal codiert; MediaMTX leitet es danach einzeln an jedes aktive Ziel weiter.
- Jeder Stream-Schlüssel liegt in einem eigenen Eintrag im macOS-Schlüsselbund. Schlüssel gelangen nie in die Konfigurationsdatei, in FFmpeg-Argumente oder in Protokolle.

| Port | Bindung | Zweck |
| --- | --- | --- |
| 1935 | Lokales Netz (`:1935`) | RTMP-Eingang von DJI Fly |
| 8554 | `127.0.0.1` | RTSP-Leser und Veröffentlichung von `/production` |
| 8889 | `127.0.0.1` | WHEP-Vorschau |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | MediaMTX Control-API |
| 9998 | `127.0.0.1` | MediaMTX-Metriken |

Konfiguration: `~/Library/Application Support/DJI Live Bridge/` · Protokolle: `~/Library/Logs/DJI Live Bridge/` · Geheimnisse: macOS-Schlüsselbund

## Entwicklung

Einzelheiten zu Build, Signierung und Notarisierung stehen im [englischen README](README.md#development).
