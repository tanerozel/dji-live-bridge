# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · **Español** · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

Aplicación para macOS que recibe la transmisión RTMP de DJI Fly en un Mac de la misma red, muestra una vista previa local mediante MediaMTX y alimenta una cadena de producción FFmpeg integrada que no necesita OBS. Desde una sola imagen puedes emitir a la vez a Instagram, TikTok y cualquier destino RTMP, o usarla como cámara virtual en TikTok LIVE Studio.

## Instalación

1. Descarga el último archivo `.dmg` desde [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Arrastra **DJI Live Bridge** a **Aplicaciones**. La cámara virtual solo funciona desde ahí.
3. Instala FFmpeg, que no viene incluido: `brew install ffmpeg`.

La aplicación está firmada y notarizada por Apple, así que se abre sin advertencias.

**Requisitos:** Mac con Apple Silicon, macOS 13 o posterior, FFmpeg/ffprobe 8.1.2 o posterior en el `PATH`.

## Emitir a Instagram / TikTok

La pestaña `Transmitir` es una sola pantalla con tres pasos:

1. **Conecta tu dron** — pega la dirección `rtmp://…/drone` que se muestra en DJI Fly (ruta: **GO FLY → Transmisión → Plataformas de streaming → RTMP**), o pulsa `Probar con un archivo de vídeo`. El paso se vuelve verde cuando llega la imagen.
2. **¿Dónde quieres transmitir?** — elige Instagram, TikTok o RTMP personalizado y pega la URL del servidor y la clave de transmisión de la plataforma. En Instagram: instagram.com → Crear (+) → Vídeo en directo. Instagram genera una clave nueva en cada directo; actualízala con `Actualizar clave` en la tarjeta del destino.
3. **Imagen y sonido** — `Vertical 9:16` (opción predeterminada para Instagram y TikTok), encuadre y micrófono opcional. Tus elecciones se recuerdan.

El botón `INICIAR TRANSMISIÓN` se activa cuando se cumplen las tres condiciones (servidor local, imagen del dron y un destino seleccionado). Durante el directo verás el tiempo transcurrido, el estado de cada destino y los datos enviados; si un destino falla, los demás siguen emitiendo. En Instagram todavía tienes que pulsar «Iniciar directo» allí para que la transmisión sea pública.

La aplicación no inicia sesión en ninguna plataforma ni pulsa por ti las pantallas de confirmación.

## TikTok LIVE Studio (cámara virtual)

Esta ruta no necesita URL de servidor ni clave de transmisión:

1. En la pestaña `TikTok LIVE Studio`, pulsa una vez `Activar cámara virtual`.
2. Si macOS lo pide, permite la extensión en **Ajustes del Sistema → General → Ítems de inicio y extensiones → Extensiones de cámara**.
3. Cuando la tarjeta indique `Lista para iniciar`, pulsa `Iniciar cámara virtual`.
4. En TikTok LIVE Studio añade una fuente de cámara y elige **DJI Live Bridge Camera**.
5. Pon la captura de audio de esa fuente en `Ninguno` y selecciona un solo micrófono en el control de audio principal de LIVE Studio.

Esta cámara transmite solo vídeo a propósito. Si sustituyes la aplicación en /Aplicaciones, macOS desactiva la extensión: vuelve a pulsar `Iniciar cámara virtual` y el botón solicitará la activación de nuevo. El botón «Go Live» sigue siendo manual dentro de TikTok LIVE Studio.

## Idiomas y temas

La aplicación está disponible en inglés, turco, español, chino, árabe, hindi, portugués, ruso, francés, alemán y japonés. En el primer inicio sigue el idioma del sistema y, si no está disponible, usa el inglés. Con el árabe toda la interfaz pasa a derecha-a-izquierda, mientras que los valores técnicos (URL, puertos, códecs) se mantienen de izquierda a derecha. Los nombres de dispositivos, códecs, protocolos y detalles de error no se traducen para que el diagnóstico siga siendo exacto.

Hay cinco temas: Sistema, Claro, Oscuro, Medianoche y Arena.

## Calidad y seguridad

- Salida constante de 30 fps, 6 Mbps CBR, H.264 High y fotogramas clave cada 2 segundos, tal como exige la entrada de Instagram/Facebook. Se prefiere el codificador por hardware VideoToolbox.
- Con el encuadre `Imagen completa`, el espacio vacío se rellena con una copia desenfocada del vídeo en lugar de barras negras.
- La imagen se codifica una sola vez y MediaMTX la reenvía a cada destino activo por separado.
- Cada clave de transmisión se guarda en una entrada propia del Llavero de macOS. Las claves nunca llegan al archivo de configuración, a los argumentos de FFmpeg ni a los registros.

| Puerto | Escucha | Uso |
| --- | --- | --- |
| 1935 | Red local (`:1935`) | Entrada RTMP de DJI Fly |
| 8554 | `127.0.0.1` | Lector RTSP y publicación de `/production` |
| 8889 | `127.0.0.1` | Vista previa WHEP |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | API de control de MediaMTX |
| 9998 | `127.0.0.1` | Métricas de MediaMTX |

Configuración: `~/Library/Application Support/DJI Live Bridge/` · Registros: `~/Library/Logs/DJI Live Bridge/` · Secretos: Llavero de macOS

## Desarrollo

Los detalles de compilación, firma y notarización están en el [README en inglés](README.md#development).
