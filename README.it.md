# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · **Italiano**

App per macOS che riceve la trasmissione RTMP di DJI Fly su un Mac della stessa rete, mostra un'anteprima locale tramite MediaMTX e alimenta una catena di produzione FFmpeg integrata che non richiede OBS. Da una sola immagine puoi trasmettere contemporaneamente su Instagram, TikTok e qualsiasi destinazione RTMP, oppure usarla come fotocamera virtuale in TikTok LIVE Studio.

## Installazione

1. Scarica l'ultimo file `.dmg` dalla pagina [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Trascina **DJI Live Bridge** in **Applicazioni**. La fotocamera virtuale funziona solo da lì.
3. Installa FFmpeg, che non è incluso: `brew install ffmpeg`.

L'app è firmata e autenticata (notarized) da Apple, quindi si apre senza avvisi.

**Requisiti:** Mac con Apple Silicon, macOS 13 o successivo, FFmpeg/ffprobe 8.1.2 o successivo nel `PATH`.

## Trasmettere su Instagram / TikTok

La scheda `Diretta` è una sola schermata in tre passaggi:

1. **Collega il drone** — incolla in DJI Fly l'indirizzo `rtmp://…/drone` mostrato (percorso: **GO FLY → Trasmissione → Piattaforme di streaming → RTMP**), oppure usa `Prova con un file video`. Il passaggio diventa verde quando arriva l'immagine.
2. **Dove vuoi trasmettere?** — scegli Instagram, TikTok o RTMP personalizzato e incolla l'indirizzo del server e la chiave di streaming della piattaforma. Su Instagram: instagram.com → Crea (+) → Video in diretta. Instagram genera una chiave nuova a ogni diretta; aggiornala con `Aggiorna chiave` sulla scheda della destinazione.
3. **Immagine e audio** — `Verticale 9:16` (impostazione predefinita per Instagram e TikTok), inquadratura e microfono facoltativo. Le scelte vengono ricordate.

Il pulsante `AVVIA DIRETTA` si sblocca quando le tre condizioni sono soddisfatte: server locale, immagine del drone e una destinazione selezionata. Durante la diretta vedi la durata, lo stato di ogni destinazione e i dati inviati; se una destinazione fallisce, le altre continuano. Su Instagram devi comunque premere «Vai in diretta» lì per rendere pubblica la trasmissione.

L'app non accede a nessun account e non conferma al posto tuo le schermate della piattaforma.

## TikTok LIVE Studio (fotocamera virtuale)

Questo percorso non richiede indirizzo del server né chiave di streaming:

1. Nella scheda `TikTok LIVE Studio` premi una volta `Attiva la fotocamera virtuale`.
2. Se macOS lo chiede, autorizza l'estensione in **Impostazioni di Sistema → Generali → Elementi login ed estensioni → Estensioni fotocamera**.
3. Quando la scheda mostra `Pronta all'avvio`, premi `Avvia fotocamera virtuale`.
4. In TikTok LIVE Studio aggiungi una sorgente fotocamera e scegli **DJI Live Bridge Camera**.
5. Imposta l'acquisizione audio di quella sorgente su `Nessuna` e seleziona un solo microfono nel controllo audio principale di LIVE Studio.

Questa fotocamera trasporta volutamente solo il video. Se sostituisci l'app in /Applicazioni, macOS disattiva l'estensione: premi di nuovo `Avvia fotocamera virtuale` e il pulsante richiederà da solo l'attivazione. «Go Live» resta manuale dentro TikTok LIVE Studio.

## Lingue e temi

L'app è disponibile in inglese, turco, spagnolo, cinese (semplificato e tradizionale), arabo, hindi, portoghese, russo, francese, tedesco, giapponese, coreano, indonesiano e italiano. Al primo avvio segue la lingua di sistema e, se non è supportata, torna all'inglese. Con l'arabo l'intera interfaccia passa da destra a sinistra, mentre i valori tecnici (indirizzi, porte, codec) restano da sinistra a destra. Nomi dei dispositivi, codec, protocolli e dettagli grezzi degli errori non vengono tradotti, così la diagnostica resta precisa.

Ci sono cinque temi: Sistema, Chiaro, Scuro, Mezzanotte e Sabbia.

## Qualità e sicurezza

- Uscita costante a 30 fps, 6 Mbps CBR, H.264 High e fotogrammi chiave ogni 2 secondi, come richiede l'ingestione di Instagram/Facebook. Si preferisce l'encoder hardware VideoToolbox.
- Con l'inquadratura `Immagine intera` lo spazio vuoto viene riempito con una copia sfocata del video invece che con bande nere.
- L'immagine viene codificata una sola volta e MediaMTX la inoltra separatamente a ogni destinazione attiva.
- Ogni chiave di streaming resta in una voce dedicata del portachiavi di macOS. Le chiavi non finiscono mai nel file di configurazione, negli argomenti di FFmpeg o nei log.

| Porta | Ascolto | Uso |
| --- | --- | --- |
| 1935 | Rete locale (`:1935`) | Ingresso RTMP da DJI Fly |
| 8554 | `127.0.0.1` | Lettura RTSP e pubblicazione di `/production` |
| 8889 | `127.0.0.1` | Anteprima WHEP |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | API di controllo MediaMTX |
| 9998 | `127.0.0.1` | Metriche MediaMTX |

Configurazione: `~/Library/Application Support/DJI Live Bridge/` · Log: `~/Library/Logs/DJI Live Bridge/` · Segreti: portachiavi macOS

## Sviluppo

I dettagli su build, firma e notarizzazione sono nel [README in inglese](README.md#development).

## Informazioni

**Taner Özel** — Sviluppatore

- E-mail: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

Questo progetto non è affiliato a DJI, Instagram o TikTok.
