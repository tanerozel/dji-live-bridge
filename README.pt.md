# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · **Português** · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

Aplicativo para macOS que recebe a transmissão RTMP do DJI Fly em um Mac na mesma rede, mostra uma prévia local através do MediaMTX e alimenta uma cadeia de produção FFmpeg integrada que dispensa o OBS. A partir de uma única imagem você transmite ao mesmo tempo para Instagram, TikTok e qualquer destino RTMP, ou a usa como câmera virtual no TikTok LIVE Studio.

## Instalação

1. Baixe o `.dmg` mais recente em [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Arraste o **DJI Live Bridge** para **Aplicativos**. A câmera virtual só funciona a partir daí.
3. Instale o FFmpeg, que não é incluído: `brew install ffmpeg`.

O aplicativo é assinado e notarizado pela Apple, então abre sem avisos.

**Requisitos:** Mac com Apple Silicon, macOS 13 ou mais recente, FFmpeg/ffprobe 8.1.2 ou mais recente no `PATH`.

## Transmitir para Instagram / TikTok

A aba `Transmitir` é uma única tela com três passos:

1. **Conecte seu drone** — cole o endereço `rtmp://…/drone` exibido no DJI Fly (caminho: **GO FLY → Transmissão → Plataformas de transmissão → RTMP**) ou use `Testar com um arquivo de vídeo`. O passo fica verde quando a imagem chega.
2. **Para onde você quer transmitir?** — escolha Instagram, TikTok ou RTMP personalizado e cole o endereço do servidor e a chave de transmissão da plataforma. No Instagram: instagram.com → Criar (+) → Vídeo ao vivo. O Instagram gera uma chave nova a cada transmissão; atualize-a com `Atualizar chave` no cartão do destino.
3. **Imagem e som** — `Vertical 9:16` (padrão para Instagram e TikTok), enquadramento e microfone opcional. As escolhas são lembradas.

O botão `INICIAR TRANSMISSÃO` é liberado quando as três condições são atendidas (servidor local, imagem do drone e um destino selecionado). Durante a transmissão você vê o tempo decorrido, o estado de cada destino e os dados enviados; se um destino falhar, os outros continuam no ar. No Instagram ainda é preciso pressionar «Iniciar live» lá para que a transmissão fique pública.

O aplicativo não faz login em nenhuma plataforma e não passa pelas telas de confirmação por você.

## TikTok LIVE Studio (câmera virtual)

Este caminho não precisa de endereço de servidor nem de chave de transmissão:

1. Na aba `TikTok LIVE Studio`, pressione uma vez `Ativar câmera virtual`.
2. Se o macOS pedir, permita a extensão em **Ajustes do Sistema → Geral → Itens de Início e Extensões → Extensões de câmera**.
3. Quando o cartão mostrar `Pronta para iniciar`, pressione `Iniciar câmera virtual`.
4. No TikTok LIVE Studio, adicione uma fonte de câmera e escolha **DJI Live Bridge Camera**.
5. Defina a captura de áudio dessa fonte como `Nenhum` e selecione apenas um microfone no controle de áudio principal do LIVE Studio.

Essa câmera transmite apenas vídeo, de propósito. Se você substituir o aplicativo em /Aplicativos, o macOS desativa a extensão: pressione `Iniciar câmera virtual` novamente e o botão solicita a ativação sozinho. O «Go Live» continua manual dentro do TikTok LIVE Studio.

## Idiomas e temas

O aplicativo está disponível em inglês, turco, espanhol, chinês, árabe, híndi, português, russo, francês, alemão e japonês. Na primeira execução ele segue o idioma do sistema e volta ao inglês se não houver suporte. Com o árabe toda a interface passa para a direita-para-esquerda, enquanto valores técnicos (URLs, portas, codecs) permanecem da esquerda para a direita. Nomes de dispositivos, codecs, protocolos e detalhes de erro não são traduzidos, para que o diagnóstico continue exato.

São cinco temas: Sistema, Claro, Escuro, Meia-noite e Areia.

## Qualidade e segurança

- Saída constante de 30 fps, 6 Mbps CBR, H.264 High e quadros-chave a cada 2 segundos, como a entrada do Instagram/Facebook exige. O codificador por hardware VideoToolbox tem preferência.
- Com o enquadramento `Imagem inteira`, o espaço vazio é preenchido por uma cópia desfocada do vídeo em vez de barras pretas.
- A imagem é codificada uma única vez e o MediaMTX a encaminha separadamente para cada destino ativo.
- Cada chave de transmissão fica em uma entrada própria nas Chaves do macOS. As chaves nunca chegam ao arquivo de configuração, aos argumentos do FFmpeg ou aos registros.

| Porta | Escuta | Uso |
| --- | --- | --- |
| 1935 | Rede local (`:1935`) | Entrada RTMP do DJI Fly |
| 8554 | `127.0.0.1` | Leitor RTSP e publicação de `/production` |
| 8889 | `127.0.0.1` | Prévia WHEP |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | API de controle do MediaMTX |
| 9998 | `127.0.0.1` | Métricas do MediaMTX |

Configuração: `~/Library/Application Support/DJI Live Bridge/` · Registros: `~/Library/Logs/DJI Live Bridge/` · Segredos: Chaves do macOS

## Desenvolvimento

Os detalhes de compilação, assinatura e notarização estão no [README em inglês](README.md#development).
