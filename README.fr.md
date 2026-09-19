# DJI Live Bridge

[English](README.md) · [Türkçe](README.tr.md) · [Español](README.es.md) · [中文（简体）](README.zh.md) · [中文（繁體）](README.zh-Hant.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md) · [Português](README.pt.md) · [Русский](README.ru.md) · **Français** · [Deutsch](README.de.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md)

Application macOS qui reçoit le flux RTMP de DJI Fly sur un Mac du même réseau, affiche un aperçu local via MediaMTX et alimente une chaîne de production FFmpeg intégrée qui ne nécessite pas OBS. À partir d'une seule image, vous diffusez simultanément vers Instagram, TikTok et n'importe quelle destination RTMP, ou vous l'utilisez comme caméra virtuelle dans TikTok LIVE Studio.

## Installation

1. Téléchargez le dernier fichier `.dmg` depuis [Releases](https://github.com/tanerozel/dji-live-bridge/releases).
2. Glissez **DJI Live Bridge** dans **Applications**. La caméra virtuelle ne fonctionne que depuis cet emplacement.
3. FFmpeg n'est pas fourni. Au premier lancement, l'application voit qu'il manque et l'installe pour vous en un clic (via Homebrew), en affichant le journal. Pour le faire vous-même : `brew install ffmpeg`.

L'application est signée et notarisée par Apple : elle s'ouvre sans avertissement.

**Prérequis :** Mac Apple Silicon, macOS 13 ou version ultérieure, FFmpeg/ffprobe 8.1.2 ou version ultérieure dans le `PATH`.

## Diffuser vers Instagram / TikTok

L'onglet `Direct` tient en un seul écran, en trois étapes :

1. **Connectez votre drone** — collez l'adresse `rtmp://…/drone` affichée dans DJI Fly (chemin : **GO FLY → Transmission → Plateformes de diffusion → RTMP**), ou utilisez `Essayer avec un fichier vidéo`. L'étape passe au vert dès que l'image arrive.
2. **Où voulez-vous diffuser ?** — choisissez Instagram, TikTok ou RTMP personnalisé, puis collez l'adresse du serveur et la clé de stream fournies par la plateforme. Sur Instagram : instagram.com → Créer (+) → Vidéo en direct. Instagram délivre une nouvelle clé à chaque direct ; mettez-la à jour avec `Mettre à jour la clé` sur la carte de la destination.
3. **Image et son** — `Vertical 9:16` (valeur par défaut pour Instagram et TikTok), cadrage et micro facultatif. Vos choix sont mémorisés.

Le bouton `DÉMARRER LE DIRECT` se débloque lorsque les trois conditions sont réunies (serveur local, image du drone et destination sélectionnée). Pendant le direct, vous voyez la durée, l'état de chaque destination et les données envoyées ; si une destination échoue, les autres continuent. Sur Instagram, vous devez encore appuyer sur « Passer en direct » là-bas pour rendre la diffusion publique.

L'application ne se connecte à aucun compte et ne valide aucun écran de confirmation à votre place.

## TikTok LIVE Studio (caméra virtuelle)

Ce chemin ne demande ni adresse de serveur ni clé de stream :

1. Dans l'onglet `TikTok LIVE Studio`, appuyez une fois sur `Activer la caméra virtuelle`.
2. Si macOS le demande, autorisez l'extension dans **Réglages Système → Général → Ouverture et extensions → Extensions de caméra**.
3. Quand la carte affiche `Prête à démarrer`, appuyez sur `Démarrer la caméra virtuelle`.
4. Dans TikTok LIVE Studio, ajoutez une source caméra et choisissez **DJI Live Bridge Camera**.
5. Réglez la capture audio de cette source sur `Aucune` et sélectionnez un seul micro dans le contrôle audio principal de LIVE Studio.

Cette caméra ne transporte que la vidéo, volontairement. Si vous remplacez l'application dans /Applications, macOS désactive l'extension : appuyez de nouveau sur `Démarrer la caméra virtuelle`, le bouton relance lui-même l'activation. Le « Go Live » reste manuel dans TikTok LIVE Studio.

## Langues et thèmes

L'application existe en anglais, turc, espagnol, chinois, arabe, hindi, portugais, russe, français, allemand et japonais. Au premier lancement, elle suit la langue du système et revient à l'anglais si besoin. En arabe, toute l'interface passe de droite à gauche, tandis que les valeurs techniques (URL, ports, codecs) restent de gauche à droite. Les noms d'appareils, codecs, protocoles et détails d'erreur ne sont pas traduits, afin que le diagnostic reste exact.

Cinq thèmes sont disponibles : Système, Clair, Sombre, Minuit et Sable.

## Qualité et sécurité

- Sortie constante à 30 i/s, 6 Mbit/s CBR, H.264 High et images clés toutes les 2 secondes, comme l'exige l'ingestion Instagram/Facebook. L'encodeur matériel VideoToolbox est privilégié.
- Avec le cadrage `Image entière`, l'espace vide est rempli par une copie floutée de la vidéo au lieu de bandes noires.
- L'image est encodée une seule fois, puis MediaMTX la retransmet séparément vers chaque destination active.
- Chaque clé de stream est conservée dans sa propre entrée du trousseau macOS. Les clés n'apparaissent jamais dans le fichier de configuration, les arguments FFmpeg ou les journaux.

| Port | Écoute | Usage |
| --- | --- | --- |
| 1935 | Réseau local (`:1935`) | Entrée RTMP de DJI Fly |
| 8554 | `127.0.0.1` | Lecteur RTSP et publication de `/production` |
| 8889 | `127.0.0.1` | Aperçu WHEP |
| 8189 | `127.0.0.1` | WebRTC ICE UDP/TCP |
| 9997 | `127.0.0.1` | API de contrôle MediaMTX |
| 9998 | `127.0.0.1` | Métriques MediaMTX |

Configuration : `~/Library/Application Support/DJI Live Bridge/` · Journaux : `~/Library/Logs/DJI Live Bridge/` · Secrets : trousseau macOS

## Développement

Les détails de compilation, de signature et de notarisation se trouvent dans le [README en anglais](README.md#development).

## Licence

MIT — voir [LICENSE](LICENSE).

## À propos

**Taner Özel** — Développeur

- E-mail: [tanerozel47@gmail.com](mailto:tanerozel47@gmail.com)
- GitHub: [github.com/tanerozel](https://github.com/tanerozel)
- LinkedIn: [linkedin.com/in/tanerozel](https://www.linkedin.com/in/tanerozel)

Ce projet n'est affilié ni à DJI, ni à Instagram, ni à TikTok.
