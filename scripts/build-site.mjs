// Generates the GitHub Pages site into docs/ from site/content/<lang>.json,
// blog sources and the standalone site/privacy-policy.html.
//
// One page per language at its own URL (English at the root, the rest under
// /<code>/), because a search engine can only rank a language it can crawl at
// a stable address — a page that swaps its text with JavaScript cannot be
// indexed per language. Every page therefore carries its own title, canonical
// URL and the full set of hreflang alternates, and the language switcher is
// made of real links.
//
// Run: npm run build:site

import { readFileSync, writeFileSync, mkdirSync, rmSync, existsSync, readdirSync, cpSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BASE = "https://streammydrone.com";
const REPO = "https://github.com/tanerozel/dji-live-bridge";
const RELEASES = `${REPO}/releases/latest`;
// The mobile apps, published on their own as "Stream My Drone".
const PLAY = "https://play.google.com/store/apps/details?id=com.streammydrone.app";
const APP_STORE = "https://apps.apple.com/us/app/stream-my-drone/id6818233095";
const YOUTUBE_CHANNEL = "https://www.youtube.com/channel/UCs8Ofh5G_EP0AXcDSzDlePg";
// Who publishes the site, for search engines and AI answers ("which app is this, who makes it").
const ORGANIZATION = {
  "@type": "Organization",
  "@id": `${BASE}/#organization`,
  name: "Stream My Drone",
  url: `${BASE}/`,
  logo: `${BASE}/img/icon.png`,
  founder: { "@type": "Person", name: "Taner Özel", url: "https://github.com/tanerozel" },
  sameAs: [YOUTUBE_CHANNEL, REPO, PLAY, APP_STORE],
};
// The 1-minute setup video (DJI RC 2 + Android phone) on the @StreamMyDrone
// YouTube channel, one per language: id, title as on YouTube, length in
// seconds, and the publish date when it is not 2026-11-04. A language without
// one shows a phone screenshot instead; a new video needs its poster from
// scripts/build-site-images.sh.
const SETUP_VIDEO_DATE = "2026-11-04";
const SETUP_VIDEOS = {
  en: ["u2Knty4SAL0", "How to Live Stream a DJI Drone to YouTube – No Computer (DJI RC 2 + DJI Fly RTMP)", 61, "2026-11-05"],
  ar: ["kAyZTpOIDjs", "بث مباشر من الدرون DJI على يوتيوب بدون كمبيوتر (DJI RC 2 + DJI Fly RTMP)", 67, "2026-11-05"],
  de: ["kqowcn2gpaM", "DJI Drohne Livestream auf YouTube – ohne PC (DJI RC 2 + DJI Fly RTMP)", 64, "2026-11-05"],
  es: ["DbzbuCCEe6s", "Cómo transmitir en vivo con dron DJI a YouTube sin PC (DJI RC 2 + DJI Fly RTMP)", 70, "2026-11-05"],
  fr: ["sF1sWzlpCiA", "DJI Live Stream : diffuser son drone en direct sur YouTube sans PC (DJI RC 2 + DJI Fly RTMP)", 64, "2026-11-05"],
  hi: ["hQ0BEY0EAMw", "Drone se Live Kaise Kare? DJI ड्रोन लाइव स्ट्रीम YouTube पर, बिना कंप्यूटर (DJI RC 2)", 65, "2026-11-05"],
  tr: ["34k-AjVjFyw", "Drone ile Canlı Yayın Nasıl Yapılır? DJI Drone'dan YouTube'a Bilgisayarsız (DJI RC 2)", 61],
  ja: ["tKgVg-YVSF0", "DJIドローンのライブ配信のやり方｜PCなしでYouTubeへ（DJI RC 2 + DJI Fly RTMP）", 67],
  it: ["_h9aSUOw4ig", "DJI Fly Live Stream: diretta YouTube dal drone DJI senza PC (DJI RC 2 + RTMP)", 66],
  pt: ["wtV2WIiJbiU", "Como fazer transmissão ao vivo com drone DJI no YouTube sem PC (DJI RC 2 + DJI Fly RTMP)", 68],
  ru: ["9NuggZa0pPM", "Трансляция с дрона DJI на YouTube без компьютера (DJI RC 2 + DJI Fly RTMP)", 63],
  id: ["gCkpUxZHwZ0", "Cara Live Streaming Drone DJI ke YouTube tanpa PC (DJI RC 2 + DJI Fly RTMP)", 64],
  ko: ["qFly4GtKiWU", "DJI 드론 라이브 스트리밍 방법 | PC 없이 유튜브 생중계 (DJI RC 2 + DJI Fly RTMP)", 64],
  zh: ["u6ebI5-Fzj4", "大疆无人机直播教程：不用电脑直播到 YouTube（DJI RC 2 + DJI Fly RTMP）", 62],
  "zh-Hant": ["M4zjTqQlHx8", "DJI 無人機直播教學：不用電腦直播到 YouTube（DJI RC 2 + DJI Fly RTMP）", 64],
};
const setupVideo = (code) => {
  const entry = SETUP_VIDEOS[code];
  return entry && { id: entry[0], title: entry[1], seconds: entry[2], date: entry[3] || SETUP_VIDEO_DATE };
};
// Google Analytics 4. Empty string removes the tag from every page.
const ANALYTICS_ID = "G-2HCY063P0W";

// dir: the writing direction. hreflang: what a search engine matches against,
// which is not always the folder name (Simplified Chinese is zh-Hans).
const LOCALES = [
  { code: "en", dir: "ltr", name: "English", hreflang: "en", og: "en_US" },
  { code: "tr", dir: "ltr", name: "Türkçe", hreflang: "tr", og: "tr_TR" },
  { code: "es", dir: "ltr", name: "Español", hreflang: "es", og: "es_ES" },
  { code: "zh", dir: "ltr", name: "中文（简体）", hreflang: "zh-Hans", og: "zh_CN" },
  { code: "zh-Hant", dir: "ltr", name: "中文（繁體）", hreflang: "zh-Hant", og: "zh_TW" },
  { code: "ar", dir: "rtl", name: "العربية", hreflang: "ar", og: "ar_AR" },
  { code: "hi", dir: "ltr", name: "हिन्दी", hreflang: "hi", og: "hi_IN" },
  { code: "pt", dir: "ltr", name: "Português", hreflang: "pt", og: "pt_BR" },
  { code: "ru", dir: "ltr", name: "Русский", hreflang: "ru", og: "ru_RU" },
  { code: "fr", dir: "ltr", name: "Français", hreflang: "fr", og: "fr_FR" },
  { code: "de", dir: "ltr", name: "Deutsch", hreflang: "de", og: "de_DE" },
  { code: "ja", dir: "ltr", name: "日本語", hreflang: "ja", og: "ja_JP" },
  { code: "ko", dir: "ltr", name: "한국어", hreflang: "ko", og: "ko_KR" },
  { code: "id", dir: "ltr", name: "Bahasa Indonesia", hreflang: "id", og: "id_ID" },
  { code: "it", dir: "ltr", name: "Italiano", hreflang: "it", og: "it_IT" },
];

const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const css = readFileSync(join(root, "site/styles.css"), "utf8");

const urlFor = (code) => (code === "en" ? `${BASE}/` : `${BASE}/${code}/`);
const pathFor = (code) => (code === "en" ? "docs" : `docs/${code}`);
// Assets live once, at the site root; a sub-page reaches them by going up.
const assetPrefix = (code) => (code === "en" ? "" : "../");

const escapeHtml = (value) =>
  value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const stripTags = (value) => value.replace(/<[^>]+>/g, "");

// Decorative icons: each button keeps its visible, localized accessible name.
const BUTTON_ICONS = {
  apple: '<path d="M17.05 12.54c.03 3.02 2.65 4.03 2.68 4.04-.02.07-.42 1.44-1.38 2.85-.83 1.21-1.69 2.42-3.04 2.45-1.33.03-1.76-.79-3.28-.79-1.52 0-2 .76-3.26.82-1.3.05-2.3-1.31-3.13-2.52-1.7-2.47-3-6.98-1.26-10.02.86-1.51 2.4-2.47 4.06-2.5 1.27-.03 2.47.87 3.24.87.78 0 2.23-1.08 3.76-.92.64.03 2.44.26 3.59 1.95-.09.06-2.14 1.24-2.12 3.77ZM14.55 5.17c.69-.84 1.16-2 1.03-3.17-.99.04-2.19.66-2.9 1.5-.63.73-1.19 1.91-1.04 3.04 1.1.09 2.22-.56 2.91-1.37Z"/>',
  windows: '<path d="M2 4.5 10.5 3v8.25H2Zm10-1.75L22 1v10.25H12ZM2 12.75h8.5V21L2 19.5Zm10 0h10V23l-10-1.75Z"/>',
  play: '<path fill="#34a853" d="m3 2 11 10L3 22Z"/><path fill="#4285f4" d="m3 2 14 8-3 2Z"/><path fill="#fbbc04" d="m17 10 4 2-4 2-3-2Z"/><path fill="#ea4335" d="m3 22 11-10 3 2Z"/>',
  github: '<path d="M12 .75a11.25 11.25 0 0 0-3.56 21.92c.56.1.77-.24.77-.54v-2.1c-3.13.68-3.79-1.33-3.79-1.33-.51-1.3-1.25-1.65-1.25-1.65-1.02-.7.08-.69.08-.69 1.13.08 1.73 1.16 1.73 1.16 1 1.72 2.64 1.22 3.28.93.1-.73.39-1.22.72-1.5-2.5-.28-5.13-1.25-5.13-5.57 0-1.23.44-2.23 1.16-3.01-.12-.28-.5-1.43.11-2.98 0 0 .95-.3 3.09 1.15A10.77 10.77 0 0 1 12 6.16c.96 0 1.92.13 2.82.38 2.14-1.45 3.08-1.15 3.08-1.15.61 1.55.23 2.7.11 2.98.72.78 1.16 1.78 1.16 3.01 0 4.33-2.64 5.29-5.15 5.57.41.35.76 1.03.76 2.08v3.1c0 .3.2.65.78.54A11.25 11.25 0 0 0 12 .75Z"/>',
  download: '<path d="M11 2h2v11l4-4 1.4 1.4L12 16.8l-6.4-6.4L7 9l4 4ZM3 16h2v4h14v-4h2v6H3Z"/>',
  globe: '<g fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/></g>',
  phone: '<path d="M7 1h10a2 2 0 0 1 2 2v18a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V3a2 2 0 0 1 2-2Zm0 3v15h10V4Zm4 16v1.5h2V20Z"/>',
};
const buttonIcon = (name) =>
  `<svg class="btn-icon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false">${BUTTON_ICONS[name]}</svg>`;

// --- blog sources --------------------------------------------------------
//
// site/blog/<lang>.json holds a language's blog labels; a language without one
// has no blog. site/blog/posts.json lists the posts in display order, and
// site/blog/posts/<id>/<lang>.json is one post in one language. Every post
// exists in English (the x-default); the other languages are optional, and a
// post only lists the languages it was really written in as alternates.

const BLOG = LOCALES.filter((locale) => existsSync(join(root, `site/blog/${locale.code}.json`))).map(
  (locale) => ({ locale, labels: JSON.parse(readFileSync(join(root, `site/blog/${locale.code}.json`), "utf8")) }),
);
const POST_IDS = JSON.parse(readFileSync(join(root, "site/blog/posts.json"), "utf8"));
const POSTS = POST_IDS.map((id) => ({
  id,
  byLang: Object.fromEntries(
    BLOG.filter(({ locale }) => existsSync(join(root, `site/blog/posts/${id}/${locale.code}.json`))).map(
      ({ locale }) => [
        locale.code,
        JSON.parse(readFileSync(join(root, `site/blog/posts/${id}/${locale.code}.json`), "utf8")),
      ],
    ),
  ),
}));

// The images a post may show, with their real size so the layout does not jump.
const IMAGES = {
  "go-live.png": [1600, 1260],
  "virtual-camera.png": [1600, 576],
  "advanced.png": [1600, 1111],
};

// The mobile app is what the blog sells: every blog page shows the localized
// Play Store screenshots from site/img/app/<lang>/ (made by
// scripts/build-site-images.sh) and links to both stores.
const APP_SHOT_SIZE = [540, 960];
const APP_SHARE_SIZE = [1024, 500];
// Which screenshots a post shows, most relevant first: the first is the promo
// at the top, the next four the gallery further down.
const DEFAULT_SHOTS = ["live", "connect", "preview", "multistream", "mic"];
const POST_SHOTS = {
  youtube: ["youtube", "live", "multistream", "connect", "reconnect"],
  instagram: ["instagram", "live", "multistream", "connect", "preview"],
  "instagram-rtmps": ["instagram", "live", "connect", "multistream", "reconnect"],
  facebook: ["live", "multistream", "connect", "reconnect", "preview"],
  tiktok: ["live", "multistream", "connect", "preview", "mic"],
  twitch: ["live", "multistream", "connect", "reconnect", "mic"],
  kick: ["live", "multistream", "connect", "reconnect", "mic"],
  multistream: ["multistream", "live", "youtube", "instagram", "reconnect"],
  "vs-restream": ["multistream", "live", "reconnect", "connect", "youtube"],
  "dji-fly-rtmp": ["connect", "preview", "live", "multistream", "reconnect"],
  "without-nginx": ["connect", "live", "multistream", "reconnect", "preview"],
  "rc2-microphone": ["mic", "connect", "live", "preview", "multistream"],
};
// Posts that mention the RC 2 point to the guide for DJI Fly's microphone
// requirement on that controller, which the Android app works around.
const MIC_POST = "rc2-microphone";
// The closing box shows the first of these the post has not shown yet.
const END_SHOTS = ["multistream", "live", "reconnect", "mic", "preview", "connect", "youtube", "instagram"];

const hasBlog = (code) => BLOG.some(({ locale }) => locale.code === code);
const blogUrl = (code) => (code === "en" ? `${BASE}/blog/` : `${BASE}/${code}/blog/`);
const blogDir = (code) => (code === "en" ? "docs/blog" : `docs/${code}/blog`);
// A language without a blog links to the English one.
const blogHomeFor = (code) => blogUrl(hasBlog(code) ? code : "en");
const postUrl = (code, post) => `${blogUrl(code)}${post.slug}/`;
// From docs/blog/ or docs/<code>/blog/<slug>/ back up to docs/.
const blogAsset = (code, depth) => "../".repeat(depth + (code === "en" ? 0 : 1));

const analytics = () =>
  ANALYTICS_ID
    ? `<script async src="https://www.googletagmanager.com/gtag/js?id=${ANALYTICS_ID}"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', '${ANALYTICS_ID}');
</script>`
    : "";

function head(locale, content) {
  const url = urlFor(locale.code);
  const asset = assetPrefix(locale.code);
  const alternates = LOCALES.map(
    (other) => `<link rel="alternate" hreflang="${other.hreflang}" href="${urlFor(other.code)}">`,
  ).join("\n");

  const software = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "Stream My Drone",
    description: stripTags(content.meta.description),
    applicationCategory: "MultimediaApplication",
    operatingSystem: "macOS 13 or newer, Windows 10/11",
    softwareVersion: version,
    url,
    downloadUrl: RELEASES,
    inLanguage: locale.hreflang,
    image: `${BASE}/img/go-live.png`,
    license: "https://opensource.org/licenses/MIT",
    isAccessibleForFree: true,
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
    author: { "@type": "Person", name: "Taner Özel", url: "https://github.com/tanerozel" },
    publisher: { "@id": ORGANIZATION["@id"] },
    dateModified: siteUpdated(),
  };

  const site = [
    { "@context": "https://schema.org", ...ORGANIZATION },
    {
      "@context": "https://schema.org",
      "@type": "WebSite",
      "@id": `${url}#website`,
      name: "Stream My Drone",
      url,
      inLanguage: locale.hreflang,
      publisher: { "@id": ORGANIZATION["@id"] },
    },
  ];

  const android = {
    "@context": "https://schema.org",
    "@type": "MobileApplication",
    name: "Stream My Drone",
    description: stripTags(content.hero.mobileLead),
    applicationCategory: "MultimediaApplication",
    operatingSystem: "Android 9 or newer, iOS 17 or newer",
    url: PLAY,
    installUrl: [PLAY, APP_STORE],
    inLanguage: locale.hreflang,
    image: shareImage(locale.code),
    author: { "@type": "Person", name: "Taner Özel", url: "https://github.com/tanerozel" },
  };

  const video = setupVideo(locale.code);
  const videoData = video && {
    "@context": "https://schema.org",
    "@type": "VideoObject",
    name: video.title,
    description: stripTags(content.hero.mobileLead),
    inLanguage: locale.hreflang,
    thumbnailUrl: `${BASE}/img/video/${locale.code}.webp`,
    uploadDate: video.date,
    duration: `PT${Math.floor(video.seconds / 60)}M${video.seconds % 60}S`,
    embedUrl: `https://www.youtube.com/embed/${video.id}`,
    url: `https://www.youtube.com/shorts/${video.id}`,
  };

  const faq = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    inLanguage: locale.hreflang,
    mainEntity: content.faq.items.map((item) => ({
      "@type": "Question",
      name: stripTags(item.q),
      acceptedAnswer: { "@type": "Answer", text: stripTags(item.a) },
    })),
  };

  return `<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
${analytics()}
<title>${escapeHtml(content.meta.title)}</title>
<meta name="description" content="${escapeHtml(stripTags(content.meta.description))}">
<meta name="keywords" content="${escapeHtml(content.meta.keywords)}">
<meta name="author" content="Taner Özel">
<meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1">
<meta name="theme-color" content="#f5f5f7" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#161618" media="(prefers-color-scheme: dark)">
<link rel="canonical" href="${url}">
${alternates}
<link rel="alternate" hreflang="x-default" href="${BASE}/">
<link rel="icon" href="${asset}img/icon.png">
<link rel="apple-touch-icon" href="${asset}img/icon.png">
<meta property="og:site_name" content="Stream My Drone">
<meta property="og:title" content="${escapeHtml(content.meta.ogTitle)}">
<meta property="og:description" content="${escapeHtml(stripTags(content.meta.ogDescription))}">
<meta property="og:image" content="${shareImage(locale.code)}">
<meta property="og:image:width" content="${APP_SHARE_SIZE[0]}">
<meta property="og:image:height" content="${APP_SHARE_SIZE[1]}">
<meta property="og:image:alt" content="${escapeHtml(stripTags(content.hero.mobilePill))}">
<meta property="og:url" content="${url}">
<meta property="og:type" content="website">
<meta property="og:locale" content="${locale.og}">
${LOCALES.filter((other) => other.code !== locale.code)
  .map((other) => `<meta property="og:locale:alternate" content="${other.og}">`)
  .join("\n")}
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escapeHtml(content.meta.ogTitle)}">
<meta name="twitter:description" content="${escapeHtml(stripTags(content.meta.ogDescription))}">
<meta name="twitter:image" content="${shareImage(locale.code)}">
${site.map((data) => `<script type="application/ld+json">${JSON.stringify(data)}</script>`).join("\n")}
<script type="application/ld+json">${JSON.stringify(software)}</script>
<script type="application/ld+json">${JSON.stringify(android)}</script>
<script type="application/ld+json">${JSON.stringify(faq)}</script>${
  videoData ? `\n<script type="application/ld+json">${JSON.stringify(videoData)}</script>` : ""
}
<style>
${css}</style>`;
}

// links: the languages this page exists in, each with its own URL. The home
// page exists in every language; a blog post only in the ones it was written in.
function languageMenu(locale, links = LOCALES.map((other) => ({ locale: other, href: urlFor(other.code) }))) {
  const items = links
    .map(
      ({ locale: other, href }) =>
        `<a href="${href}" hreflang="${other.hreflang}" lang="${other.hreflang}"${
          other.code === locale.code ? ' aria-current="true"' : ""
        }>${other.name}</a>`,
    )
    .join("");
  return `<details class="langmenu">
        <summary aria-label="${escapeHtml(locale.nav)}"><span aria-hidden="true">🌐</span><span class="langname">${escapeHtml(locale.name)}</span></summary>
        <div class="sheet">${items}</div>
      </details>`;
}

function page(locale, content) {
  const asset = assetPrefix(locale.code);
  const menu = languageMenu({ ...locale, nav: content.nav.language });

  const cards = content.features.cards
    .map((card) => `<div class="card"><h3>${card.h}</h3><p>${card.p}</p></div>`)
    .join("\n      ");
  const howSteps = content.how.steps
    .map((step) => `<li><div><strong>${step.t}</strong><span>${step.d}</span></div></li>`)
    .join("\n      ");
  const androidItems = content.android.items.map((item) => `<li>${item}</li>`).join("\n          ");
  const tiktokItems = content.tiktok.items.map((item) => `<li>${item}</li>`).join("\n          ");
  const installSteps = content.install.steps
    .map((step) => `<li><div><strong>${step.t}</strong><span>${step.d}</span></div></li>`)
    .join("\n      ");
  const faqItems = content.faq.items
    .map((item) => `<details><summary>${item.q}</summary><p>${item.a}</p></details>`)
    .join("\n      ");
  const langLinks = LOCALES.map(
    (other) =>
      `<a href="${urlFor(other.code)}" hreflang="${other.hreflang}" lang="${other.hreflang}"${
        other.code === locale.code ? ' aria-current="true"' : ""
      }>${other.name}</a>`,
  ).join("\n      ");

  return `<!doctype html>
<html lang="${locale.hreflang}" dir="${locale.dir}">
<head>
${head(locale, content)}
</head>
<body>

<a class="skip" href="#main">${content.nav.skip}</a>

<header>
  <div class="wrap bar">
    <a class="brand" href="${urlFor(locale.code)}"><img src="${asset}img/icon.png" alt="" width="30" height="30">Stream My Drone</a>
    <nav>
      <a href="#android">${content.nav.android}</a>
      <a href="#desktop">macOS · Windows</a>
      <a href="#features">${content.nav.features}</a>
      <a href="#tiktok">${content.nav.tiktok}</a>
      <a href="#faq">${content.nav.faq}</a>
      <a href="${blogHomeFor(locale.code)}">${content.nav.blog}</a>
      ${menu}
      <a class="btn small primary" href="#app">${buttonIcon("phone")}${blogLabels(locale.code).download}</a>
    </nav>
  </div>
</header>

<main id="main">

<div class="wrap hero hero-app" id="app">
  <div class="hero-text">
    <span class="pill"><span class="dot"></span>${content.hero.mobilePill}</span>
    <h1>${content.hero.h1}</h1>
    <p class="lead">${content.hero.mobileLead}</p>
    ${storeButtons(locale.code, "primary")}
    <p class="meta"><a href="#desktop">${content.hero.desktopLink} →</a></p>
  </div>
  ${heroMedia(locale, content, asset)}
</div>

<section id="android">
  <div class="wrap">
    <h2>${content.android.h2}</h2>
    <p class="sub">${content.android.sub}</p>
    <div class="split top">
      <div>
        <ul class="plain">
          ${androidItems}
        </ul>
        <p class="note" style="margin-top:16px">${content.android.note}</p>
        <div class="cta" style="justify-content:flex-start;margin-top:22px">
          <a class="btn primary" href="${PLAY}" data-store="play">${buttonIcon("play")}${content.android.cta}</a>
        </div>
        <p class="note" style="margin-top:22px">${content.android.iosNote}</p>
        <div class="cta" style="justify-content:flex-start;margin-top:12px">
          <a class="btn" href="${APP_STORE}" data-store="ios">${buttonIcon("apple")}${content.android.ctaIos}</a>
        </div>
      </div>
      <div class="shot phone"><img src="${asset}img/app/${locale.code}/multistream.webp" alt="${escapeHtml(blogLabels(locale.code).app.shots.multistream)}" width="${APP_SHOT_SIZE[0]}" height="${APP_SHOT_SIZE[1]}" loading="lazy"></div>
    </div>
    <div class="gallery-row home-gallery">${["connect", "youtube", "instagram", "reconnect"]
      .map((name) => appShot(locale.code, blogLabels(locale.code), asset, name))
      .join("")}</div>
  </div>
</section>

<section id="desktop">
  <div class="wrap hero desktop-hero">
    <span class="pill"><span class="dot"></span>${content.hero.pill}</span>
    <h2>${content.desktop.h2}</h2>
    <p class="lead">${content.hero.lead}</p>
    <div class="cta">
      <a class="btn primary" href="${RELEASES}">${buttonIcon("apple")}${content.hero.ctaMac}</a>
      <a class="btn" href="${RELEASES}">${buttonIcon("windows")}${content.hero.ctaWin}</a>
      <a class="btn" href="${REPO}">${buttonIcon("github")}${content.hero.ctaSource}</a>
    </div>
    <p class="meta">${content.hero.meta}</p>
    <div class="shot"><img src="${asset}img/go-live.png" alt="${escapeHtml(stripTags(content.hero.shotAlt))}" width="1600" height="1260" loading="lazy"></div>
    <p class="caption">${content.hero.caption}</p>
  </div>
</section>

<section id="features">
  <div class="wrap">
    <h2>${content.features.h2}</h2>
    <p class="sub">${content.features.sub}</p>
    <div class="grid">
      ${cards}
    </div>
  </div>
</section>

<section id="how">
  <div class="wrap">
    <h2>${content.how.h2}</h2>
    <p class="sub">${content.how.sub}</p>
    <ol class="steps">
      ${howSteps}
    </ol>
  </div>
</section>

<section id="tiktok">
  <div class="wrap">
    <h2>${content.tiktok.h2}</h2>
    <p class="sub">${content.tiktok.sub}</p>
    <div class="split">
      <div>
        <ul class="plain">
          ${tiktokItems}
        </ul>
        <p class="note" style="margin-top:16px">${content.tiktok.note}</p>
      </div>
      <div class="shot"><img src="${asset}img/virtual-camera.png" alt="${escapeHtml(stripTags(content.tiktok.shotAlt))}" width="1600" height="576" loading="lazy"></div>
    </div>
  </div>
</section>

<section>
  <div class="wrap">
    <h2>${content.details.h2}</h2>
    <p class="sub">${content.details.sub}</p>
    <div class="shot" style="margin-top:28px"><img src="${asset}img/advanced.png" alt="${escapeHtml(stripTags(content.details.shotAlt))}" width="1600" height="1111" loading="lazy"></div>
  </div>
</section>

<section id="install">
  <div class="wrap">
    <h2>${content.install.h2}</h2>
    <ol class="steps">
      ${installSteps}
    </ol>
    <div class="cta" style="justify-content:flex-start;margin-top:26px">
      <a class="btn primary" href="${RELEASES}">${buttonIcon("download")}${content.install.cta}</a>
    </div>
    <h3 style="margin-top:34px;font-size:16px">${content.install.langsTitle}</h3>
    <div class="langs">
      ${langLinks}
    </div>
  </div>
</section>

<section id="faq">
  <div class="wrap">
    <h2>${content.faq.h2}</h2>
    <div class="faq">
      ${faqItems}
    </div>
  </div>
</section>

${guidesSection(locale)}
</main>

${footer(content)}

${appBar(blogLabels(locale.code), asset)}

</body>
</html>
`;
}

function footer(content) {
  return `<footer>
  <div class="wrap">
    <strong style="color:var(--text)">Taner Özel</strong> — ${content.footer.role}
    <div class="foot-links">
      <a href="mailto:tanerozel47@gmail.com">tanerozel47@gmail.com</a>
      <a href="https://github.com/tanerozel">GitHub</a>
      <a href="https://www.linkedin.com/in/tanerozel">LinkedIn</a>
      <a href="${REPO}/issues">${content.footer.issue}</a>
      <a href="${BASE}/privacy-policy.html">${content.footer.privacy}</a>
      <a href="${BASE}/llms.txt">llms.txt</a>
    </div>
    <p class="note">${content.footer.note}</p>
  </div>
</footer>`;
}

// --- blog pages ------------------------------------------------------------

const author = { "@type": "Person", name: "Taner Özel", url: "https://github.com/tanerozel" };

// The home page's list of guides, in the languages that have a blog.
function guidesSection(locale) {
  const blog = BLOG.find((entry) => entry.locale.code === locale.code);
  if (!blog) return "";
  return `<section id="guides">
  <div class="wrap">
    <h2>${blog.labels.guides.h2}</h2>
    <p class="sub">${blog.labels.guides.sub}</p>
    ${postCards(locale.code, POSTS)}
    <p style="margin-top:22px"><a href="${blogUrl(locale.code)}">${blog.labels.guides.all} →</a></p>
  </div>
</section>
`;
}

function postCards(code, posts) {
  const cards = posts
    .filter((post) => post.byLang[code])
    .map((post) => {
      const content = post.byLang[code];
      return `<a class="card post-card" href="${postUrl(code, content)}"><h3>${content.h1}</h3><p>${content.description}</p></a>`;
    })
    .join("\n      ");
  return `<div class="grid">
      ${cards}
    </div>`;
}

const formatDate = (locale, iso) =>
  new Intl.DateTimeFormat(locale.hreflang, { dateStyle: "long", timeZone: "UTC" }).format(new Date(iso));

function blogHead(locale, { title, description, url, alternates, asset, type, jsonld, published, updated }) {
  const links = alternates
    .map(({ locale: other, href }) => `<link rel="alternate" hreflang="${other.hreflang}" href="${href}">`)
    .join("\n");
  const fallback = alternates.find(({ locale: other }) => other.code === "en");
  const article = published
    ? `\n<meta property="article:published_time" content="${published}">\n<meta property="article:modified_time" content="${updated}">\n<meta property="article:author" content="Taner Özel">`
    : "";
  return `<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
${analytics()}
<title>${escapeHtml(title)}</title>
<meta name="description" content="${escapeHtml(stripTags(description))}">
<meta name="author" content="Taner Özel">
<meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1">
<meta name="theme-color" content="#f5f5f7" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#161618" media="(prefers-color-scheme: dark)">
<link rel="canonical" href="${url}">
${links}
<link rel="alternate" hreflang="x-default" href="${fallback.href}">
<link rel="icon" href="${asset}img/icon.png">
<link rel="apple-touch-icon" href="${asset}img/icon.png">
<meta property="og:site_name" content="Stream My Drone">
<meta property="og:title" content="${escapeHtml(title)}">
<meta property="og:description" content="${escapeHtml(stripTags(description))}">
<meta property="og:image" content="${shareImage(locale.code)}">
<meta property="og:image:width" content="${APP_SHARE_SIZE[0]}">
<meta property="og:image:height" content="${APP_SHARE_SIZE[1]}">
<meta property="og:url" content="${url}">
<meta property="og:type" content="${type}">
<meta property="og:locale" content="${locale.og}">${article}
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escapeHtml(title)}">
<meta name="twitter:description" content="${escapeHtml(stripTags(description))}">
<meta name="twitter:image" content="${shareImage(locale.code)}">
${jsonld.map((data) => `<script type="application/ld+json">${JSON.stringify(data)}</script>`).join("\n")}
<style>
${css}</style>`;
}

function blogHeader(locale, labels, menuLinks, asset) {
  const menu = languageMenu({ ...locale, nav: labels.language }, menuLinks);
  return `<a class="skip" href="#main">${labels.skip}</a>

<header>
  <div class="wrap bar">
    <a class="brand" href="${urlFor(locale.code)}"><img src="${asset}img/icon.png" alt="" width="30" height="30">Stream My Drone</a>
    <nav>
      <a href="${urlFor(locale.code)}">${labels.home}</a>
      <a href="${blogUrl(locale.code)}">${labels.blog}</a>
      ${menu}
      <a class="btn small primary" href="#app">${buttonIcon("phone")}${labels.download}</a>
    </nav>
  </div>
</header>`;
}

// --- mobile app blocks on blog pages -----------------------------------------

const shareImage = (code) => `${BASE}/img/app/${code}/share.jpg`;
const postShots = (post) => POST_SHOTS[post.id] || DEFAULT_SHOTS;

function appShot(code, labels, asset, name, extra = "") {
  const [width, height] = APP_SHOT_SIZE;
  return `<img src="${asset}img/app/${code}/${name}.webp" alt="${escapeHtml(labels.app.shots[name])}" width="${width}" height="${height}" loading="lazy" decoding="async"${extra}>`;
}

// Every language has a blog, so its labels (the app texts, the screenshot
// alt texts) serve the home page too.
const blogLabels = (code) => BLOG.find(({ locale }) => locale.code === code).labels;

// data-store lets the small script at the end of the page send a phone
// straight to its own store; without it both buttons simply work.
function storeButtons(code, style = "store") {
  const home = homeContent[code];
  return `<div class="cta store-buttons"><a class="btn ${style}" href="${PLAY}" data-store="play">${buttonIcon("play")}${home.android.cta}</a><a class="btn ${style}" href="${APP_STORE}" data-store="ios">${buttonIcon("apple")}${home.android.ctaIos}</a></div>`;
}

// The setup video as a poster that links to YouTube. The page script swaps it
// for a youtube-nocookie player on click, so nothing loads from YouTube until
// someone asks for the video, and without JavaScript the link still works.
function videoFrame(code, asset, label, alt, priority) {
  const video = setupVideo(code);
  return `<div class="video-frame"><a class="video" href="https://www.youtube.com/shorts/${video.id}" data-video="${video.id}" data-title="${escapeHtml(video.title)}"><img src="${asset}img/video/${code}.webp" alt="${escapeHtml(alt)}" width="${APP_SHOT_SIZE[0]}" height="${APP_SHOT_SIZE[1]}"${priority ? ' fetchpriority="high"' : ' loading="lazy"'}><span class="video-play"><svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" focusable="false"><path fill="currentColor" d="M8 5v14l11-7Z"/></svg>${label}</span></a></div>`;
}

function heroMedia(locale, content, asset) {
  if (setupVideo(locale.code)) {
    return `<div class="hero-media">${videoFrame(locale.code, asset, content.hero.videoPlay, content.hero.posterAlt, true)}</div>`;
  }
  const [width, height] = APP_SHOT_SIZE;
  return `<div class="hero-media"><img class="hero-shot" src="${asset}img/app/${locale.code}/live.webp" alt="${escapeHtml(blogLabels(locale.code).app.shots.live)}" width="${width}" height="${height}" fetchpriority="high"></div>`;
}

// The promo near the top of every blog page; #app is what the header button
// and the bar at the bottom of a phone screen jump to.
function appPromo(code, labels, asset, shots, withVideo = false) {
  const { app } = labels;
  const home = homeContent[code];
  const phones =
    withVideo && setupVideo(code)
      ? videoFrame(code, asset, home.hero.videoPlay, home.hero.posterAlt, false)
      : shots.map((name) => appShot(code, labels, asset, name)).join("");
  return `<aside class="promo${shots.length > 1 ? " fan" : ""}" id="app" aria-labelledby="app-title">
<div class="promo-text">
<span class="promo-badge">${buttonIcon("phone")}${app.badge}</span>
<h2 id="app-title">${app.h}</h2>
<p>${app.p}</p>
<ul class="promo-points">${app.points.map((point) => `<li>${point}</li>`).join("")}</ul>
${storeButtons(code)}
<p class="promo-price">${app.price}</p>
</div>
<div class="promo-phones">${phones}</div>
</aside>`;
}

function micCallout(locale, labels, post) {
  const target = POSTS.find((other) => other.id === MIC_POST);
  if (!target || post.id === MIC_POST || !target.byLang[locale.code]) return "";
  if (!JSON.stringify(post.byLang.en).includes("RC 2")) return "";
  return `<p class="callout mic-callout">${labels.micCallout.text} <a href="${postUrl(locale.code, target.byLang[locale.code])}">${labels.micCallout.link} →</a></p>\n`;
}

function appGallery(code, labels, asset, shots) {
  return `<div class="gallery">
<p class="gallery-h"><strong>${labels.app.galleryH}</strong><span>${labels.app.gallerySub}</span></p>
<div class="gallery-row">${shots.map((name) => appShot(code, labels, asset, name)).join("")}</div>
${storeButtons(code)}
</div>`;
}

function appCtaBox(locale, labels, asset, shot) {
  return `<div class="cta-box promo-end">
<div class="promo-text">
<h2>${labels.cta.h}</h2>
<p>${labels.cta.p}</p>
${storeButtons(locale.code)}
<p class="promo-more"><a href="${RELEASES}">${labels.cta.button} →</a><a href="${urlFor(locale.code)}">${labels.cta.more} →</a></p>
</div>
<div class="promo-phones">${appShot(locale.code, labels, asset, shot)}</div>
</div>`;
}

// Fixed to the bottom of a phone screen (hidden on wider ones by the CSS).
function appBar(labels, asset) {
  return `<div class="appbar">
<img src="${asset}img/icon.png" alt="" width="40" height="40">
<p><strong>Stream My Drone</strong><span>${labels.app.barSub}</span></p>
<a class="btn primary small" href="#app" data-store="auto">${labels.app.barButton}</a>
</div>
${PAGE_SCRIPT}`;
}

// Sends a phone's "Get the app" straight to its own store, plays the setup
// video in place, and counts store clicks and video plays in Analytics.
const PAGE_SCRIPT = `<script>(function(){
var u=navigator.userAgent,s=/android/i.test(u)?"play":/iphone|ipad|ipod/i.test(u)||(/macintosh/i.test(u)&&navigator.maxTouchPoints>1)?"ios":"";
var h={play:${JSON.stringify(PLAY)},ios:${JSON.stringify(APP_STORE)}};
var ev=function(n,p){if(window.gtag)gtag("event",n,p)};
if(s){document.querySelectorAll('a[data-store="auto"]').forEach(function(a){a.href=h[s]});document.documentElement.classList.add("on-"+s)}
document.querySelectorAll("a[data-store]").forEach(function(a){a.addEventListener("click",function(){ev("store_click",{store:a.href.indexOf("apple.com")>0?"ios":a.href.indexOf("google.com")>0?"play":"page"})})});
document.querySelectorAll("a.video[data-video]").forEach(function(a){a.addEventListener("click",function(e){e.preventDefault();var f=document.createElement("iframe");f.src="https://www.youtube-nocookie.com/embed/"+a.dataset.video+"?autoplay=1&playsinline=1&rel=0";f.title=a.dataset.title;f.allow="autoplay; encrypted-media; picture-in-picture; fullscreen";f.allowFullscreen=true;a.replaceWith(f);ev("setup_video_play",{video_id:a.dataset.video})})});
})();</script>`;

function breadcrumbs(locale, labels, trail) {
  const items = [
    { name: labels.home, url: urlFor(locale.code) },
    { name: labels.blog, url: blogUrl(locale.code) },
    ...trail,
  ];
  const html = `<nav class="crumbs" aria-label="${escapeHtml(labels.breadcrumb)}">${items
    .map((item, index) =>
      index === items.length - 1
        ? `<span aria-current="page">${item.name}</span>`
        : `<a href="${item.url}">${item.name}</a>`,
    )
    .join('<span aria-hidden="true">›</span>')}</nav>`;
  const data = {
    "@context": "https://schema.org",
    "@type": "BreadcrumbList",
    itemListElement: items.map((item, index) => ({
      "@type": "ListItem",
      position: index + 1,
      name: stripTags(item.name),
      item: item.url,
    })),
  };
  return { html, data };
}

// A section is a heading and a list of blocks; each block has exactly one key.
function renderBlock(block, asset, code) {
  if (block.p) return `<p>${block.p}</p>`;
  // { "video": "<caption>" }: the language's setup video, with a caption.
  if (block.video) {
    if (!setupVideo(code)) return "";
    const home = homeContent[code];
    return `<figure class="post-video">${videoFrame(code, asset, home.hero.videoPlay, home.hero.posterAlt, false)}<figcaption>${block.video}</figcaption></figure>`;
  }
  if (block.note) return `<p class="callout">${block.note}</p>`;
  if (block.list) return `<ul class="plain">${block.list.map((item) => `<li>${item}</li>`).join("")}</ul>`;
  if (block.steps) {
    return `<ol class="steps">${block.steps
      .map((step) => `<li><div><strong>${step.t}</strong><span>${step.d}</span></div></li>`)
      .join("")}</ol>`;
  }
  if (block.table) {
    const head = block.table.head.map((cell) => `<th scope="col">${cell}</th>`).join("");
    const rows = block.table.rows
      .map((row) => `<tr>${row.map((cell, index) => (index === 0 ? `<th scope="row">${cell}</th>` : `<td>${cell}</td>`)).join("")}</tr>`)
      .join("");
    return `<div class="table"><table><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>`;
  }
  if (block.img) {
    const [width, height] = IMAGES[block.img];
    return `<div class="shot"><img src="${asset}img/${block.img}" alt="${escapeHtml(stripTags(block.alt))}" width="${width}" height="${height}" loading="lazy"></div>`;
  }
  throw new Error(`unknown blog block: ${JSON.stringify(block).slice(0, 80)}`);
}

function postPage(locale, labels, post, content) {
  const url = postUrl(locale.code, content);
  const asset = blogAsset(locale.code, 2);
  const languages = BLOG.filter(({ locale: other }) => post.byLang[other.code]).map(({ locale: other }) => ({
    locale: other,
    href: postUrl(other.code, post.byLang[other.code]),
  }));
  const crumbs = breadcrumbs(locale, labels, [{ name: content.platform, url }]);

  const shots = postShots(post);
  // The screenshot gallery goes after the second section, where a reader has
  // seen what the workflow needs and before the long troubleshooting part.
  const galleryAfter = Math.min(1, content.sections.length - 1);
  const sections = content.sections
    .map(
      (section, index) =>
        `<h2 id="${section.id}">${section.h2}</h2>\n${section.blocks.map((block) => renderBlock(block, asset, locale.code)).join("\n")}${
          index === galleryAfter ? `\n${appGallery(locale.code, labels, asset, shots.slice(1, 5))}` : ""
        }`,
    )
    .join("\n\n");
  const toc = [...content.sections.map((section) => ({ id: section.id, h2: section.h2 })), { id: "faq", h2: labels.faq }]
    .map((item) => `<li><a href="#${item.id}">${item.h2}</a></li>`)
    .join("");
  const faq = content.faq
    .map((item) => `<details><summary>${item.q}</summary><p>${item.a}</p></details>`)
    .join("\n");
  const others = POSTS.filter((other) => other.id !== post.id);

  const blogPosting = {
    "@context": "https://schema.org",
    "@type": "BlogPosting",
    headline: stripTags(content.h1),
    description: stripTags(content.description),
    inLanguage: locale.hreflang,
    datePublished: content.published,
    dateModified: content.updated,
    url,
    mainEntityOfPage: url,
    image: shareImage(locale.code),
    author,
    publisher: author,
    isPartOf: { "@type": "Blog", name: stripTags(labels.meta.title), url: blogUrl(locale.code) },
    about: { "@type": "SoftwareApplication", name: "Stream My Drone", url: urlFor(locale.code) },
  };
  const howtoSection = content.sections.find((section) => section.howto);
  const howtoSteps = howtoSection ? howtoSection.blocks.find((block) => block.steps).steps : [];
  const howto = howtoSection && {
    "@context": "https://schema.org",
    "@type": "HowTo",
    name: stripTags(howtoSection.h2),
    inLanguage: locale.hreflang,
    tool: [{ "@type": "HowToTool", name: "Stream My Drone" }, { "@type": "HowToTool", name: "DJI Fly" }],
    step: howtoSteps.map((step, index) => ({
      "@type": "HowToStep",
      position: index + 1,
      name: stripTags(step.t),
      text: stripTags(step.d),
      url: `${url}#${howtoSection.id}`,
    })),
  };
  const faqData = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    inLanguage: locale.hreflang,
    mainEntity: content.faq.map((item) => ({
      "@type": "Question",
      name: stripTags(item.q),
      acceptedAnswer: { "@type": "Answer", text: stripTags(item.a) },
    })),
  };

  return `<!doctype html>
<html lang="${locale.hreflang}" dir="${locale.dir}">
<head>
${blogHead(locale, {
  title: content.title,
  description: content.description,
  url,
  alternates: languages,
  asset,
  type: "article",
  published: content.published,
  updated: content.updated,
  jsonld: [blogPosting, howto, faqData, crumbs.data].filter(Boolean),
})}
</head>
<body>

${blogHeader(locale, labels, languages, asset)}

<main id="main">
<article class="wrap article">
${crumbs.html}
<h1>${content.h1}</h1>
<p class="byline">${labels.by} <a href="https://github.com/tanerozel" rel="author">Taner Özel</a> · ${labels.updated} <time datetime="${content.updated}">${formatDate(locale, content.updated)}</time></p>
<p class="answer">${content.lead}</p>

${appPromo(locale.code, labels, asset, shots.slice(0, 1), true)}
${micCallout(locale, labels, post)}
<div class="keyfacts">
<h2 id="summary">${labels.keyFacts}</h2>
<ul>${content.summary.map((item) => `<li>${item}</li>`).join("")}</ul>
</div>

<nav class="toc" aria-labelledby="toc-title"><strong id="toc-title">${labels.toc}</strong><ol>${toc}</ol></nav>

${sections}

<h2 id="faq">${labels.faq}</h2>
<div class="faq">
${faq}
</div>

${appCtaBox(locale, labels, asset, END_SHOTS.find((name) => !shots.includes(name)))}

<h2 id="related">${labels.related}</h2>
${postCards(locale.code, others)}
</article>
</main>

${footer(homeContent[locale.code])}

${appBar(labels, asset)}

</body>
</html>
`;
}

function blogIndexPage(locale, labels) {
  const url = blogUrl(locale.code);
  const asset = blogAsset(locale.code, 1);
  const languages = BLOG.map(({ locale: other }) => ({ locale: other, href: blogUrl(other.code) }));
  const crumbs = breadcrumbs(locale, labels, []);
  const posts = POSTS.filter((post) => post.byLang[locale.code]);
  const blog = {
    "@context": "https://schema.org",
    "@type": "Blog",
    name: stripTags(labels.meta.title),
    description: stripTags(labels.meta.description),
    url,
    inLanguage: locale.hreflang,
    author,
    blogPost: posts.map((post) => {
      const content = post.byLang[locale.code];
      return {
        "@type": "BlogPosting",
        headline: stripTags(content.h1),
        url: postUrl(locale.code, content),
        datePublished: content.published,
        dateModified: content.updated,
        author,
      };
    }),
  };

  return `<!doctype html>
<html lang="${locale.hreflang}" dir="${locale.dir}">
<head>
${blogHead(locale, {
  title: labels.meta.title,
  description: labels.meta.description,
  url,
  alternates: languages,
  asset,
  type: "website",
  jsonld: [blog, crumbs.data],
})}
</head>
<body>

${blogHeader(locale, labels, languages, asset)}

<main id="main">
<div class="wrap blog-index">
${crumbs.html}
<h1>${labels.h1}</h1>
<p class="lead">${labels.lead}</p>
${appPromo(locale.code, labels, asset, ["connect", "live", "multistream"])}
${postCards(locale.code, posts)}
${appCtaBox(locale, labels, asset, "multistream")}
</div>
</main>

${footer(homeContent[locale.code])}

${appBar(labels, asset)}

</body>
</html>
`;
}

// The home pages change with the newest guide or setup video; a fixed date keeps CI's rebuild identical.
function siteUpdated() {
  const videoDates = Object.values(SETUP_VIDEOS).map((video) => video[3] ?? SETUP_VIDEO_DATE);
  return [...POSTS.map((post) => post.byLang.en.updated), ...videoDates].sort().at(-1);
}

// Every blog URL with the languages it exists in, for the sitemap and llms.txt.
function blogEntries() {
  const entries = [];
  entries.push({
    languages: BLOG.map(({ locale }) => ({ locale, href: blogUrl(locale.code) })),
    lastmod: POSTS.map((post) => post.byLang.en.updated).sort().at(-1),
  });
  for (const post of POSTS) {
    entries.push({
      languages: BLOG.filter(({ locale }) => post.byLang[locale.code]).map(({ locale }) => ({
        locale,
        href: postUrl(locale.code, post.byLang[locale.code]),
      })),
      lastmod: post.byLang.en.updated,
    });
  }
  return entries;
}

function sitemap() {
  const entries = LOCALES.map((locale) => {
    const alternates = LOCALES.map(
      (other) =>
        `    <xhtml:link rel="alternate" hreflang="${other.hreflang}" href="${urlFor(other.code)}"/>`,
    ).join("\n");
    return `  <url>
    <loc>${urlFor(locale.code)}</loc>
${alternates}
    <xhtml:link rel="alternate" hreflang="x-default" href="${BASE}/"/>
    <lastmod>${siteUpdated()}</lastmod>
    <changefreq>weekly</changefreq>
    <priority>${locale.code === "en" ? "1.0" : "0.8"}</priority>
  </url>`;
  }).join("\n");

  const blog = blogEntries()
    .flatMap(({ languages, lastmod }) => {
      const alternates = languages
        .map(({ locale, href }) => `    <xhtml:link rel="alternate" hreflang="${locale.hreflang}" href="${href}"/>`)
        .join("\n");
      const fallback = languages.find(({ locale }) => locale.code === "en").href;
      return languages.map(
        ({ href }) => `  <url>
    <loc>${href}</loc>
${alternates}
    <xhtml:link rel="alternate" hreflang="x-default" href="${fallback}"/>
    <lastmod>${lastmod}</lastmod>
    <changefreq>monthly</changefreq>
    <priority>0.7</priority>
  </url>`,
      );
    })
    .join("\n");

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${entries}
${blog}
</urlset>
`;
}

// Content-Signal: Cloudflare's Content Signals Policy (contentsignals.org). Crawlers that
// do not know it ignore the line (RFC 9309), so it costs nothing.
const CONTENT_SIGNAL = "Content-Signal: search=yes, ai-input=yes, ai-train=yes";

// AI crawlers already match `*`; naming them only makes the welcome explicit. A crawler with
// its own group ignores `*`, so the group repeats the same rules.
const AI_BOTS = [
  "GPTBot",
  "OAI-SearchBot",
  "ChatGPT-User",
  "ClaudeBot",
  "Claude-SearchBot",
  "Claude-User",
  "Google-Extended",
  "PerplexityBot",
  "Perplexity-User",
  "Applebot-Extended",
  "meta-externalagent",
  "CCBot",
];

const robots = () => `User-agent: *
${CONTENT_SIGNAL}
Allow: /

# AI search and assistant crawlers are welcome.
${AI_BOTS.map((bot) => `User-agent: ${bot}`).join("\n")}
${CONTENT_SIGNAL}
Allow: /

Sitemap: ${BASE}/sitemap.xml
`;

// Spawning's ai.txt (site.spawning.ai): the same permission for AI use, for tools that look for it.
const aiTxt = () => `# Stream My Drone: text and images may be used by AI systems, with a link back.
User-Agent: *
Allow: /
`;

// llmstxt.org format: a short, factual map of the project for language models,
// which otherwise have to guess from the rendered page.
function llms(content) {
  return `# Stream My Drone

> A free, open-source desktop app (macOS and Windows) that receives the RTMP stream DJI Fly sends
> from a DJI drone and restreams it to Instagram, TikTok and any other RTMP destination at the same
> time, or exposes it as a virtual camera named "Stream My Drone Camera" for TikTok LIVE Studio,
> OBS and Zoom.

Everything runs locally: the drone video never passes through a third-party server. FFmpeg and the
media server are bundled, so nothing else has to be installed. Stream keys are kept in the operating
system's credential vault. Version ${version}. MIT licence; the bundled FFmpeg is LGPL, built
without GPL components.

Key facts:
- Platforms: macOS 13 or newer (Apple Silicon and Intel, signed and notarized), Windows 10/11 (streaming, preview, recording, diagnostics and built-in virtual camera tested; unsigned installer).
- Android: a separate paid app, "Stream My Drone" (Android 9 or newer, one-time purchase on Google Play), receives the DJI Fly stream on the phone and forwards it unchanged to several RTMP/RTMPS destinations; no virtual camera, no portrait conversion.
- Output: 1080x1920 portrait, 30 fps, 6 Mbps CBR, H.264 High, 2-second keyframes; empty space filled with a blurred copy of the picture.
- Input: whatever DJI Fly sends over RTMP, often 720p from the controller.
- Also receives DJI Osmo cameras (Osmo Action, Osmo Pocket, Osmo 360) that stream RTMP from the DJI Mimo app.
- The virtual camera carries video only, so TikTok LIVE Studio keeps control of audio.
- Not affiliated with DJI, Instagram, TikTok or Apple.

## Documentation
- [Home page](${BASE}/): what the app does, how to set it up, FAQ.
- [Full site text](${BASE}/llms-full.txt): every section of the home page and every guide as plain text.
- [Blog](${BASE}/blog/): step-by-step guides for each drone and streaming platform, in ${BLOG.length} languages.
- [README](${REPO}/blob/main/README.md): installation, streaming guide, development notes.
- [Turkish README](${REPO}/blob/main/README.tr.md): the same document in Turkish; 13 further languages sit beside it.
- [Releases](${RELEASES}): downloads for macOS and Windows.
- [Stream My Drone on Google Play](${PLAY}): the Android app.
- [Stream My Drone on the App Store](${APP_STORE}): the iOS app.
- [Dependencies](${BASE}/DEPENDENCIES.md): third-party components and their licences.
- [Security policy](${BASE}/SECURITY.md): how to report a vulnerability.

## Guides
${POSTS.map((post) => {
  const content = post.byLang.en;
  return `- [${stripTags(content.h1)}](${postUrl("en", content)}): ${stripTags(content.description)}`;
}).join("\n")}

## Languages
${LOCALES.map((locale) => `- [${locale.name}](${urlFor(locale.code)})`).join("\n")}

## Source
- [Repository](${REPO})
- [Issue tracker](${REPO}/issues)
`;
}

const line = (text) => stripTags(text).replace(/\s+/g, " ").trim();

function postText(content) {
  const block = (item) => {
    if (item.p) return [line(item.p)];
    if (item.note) return [line(item.note)];
    if (item.list) return item.list.map((entry) => `- ${line(entry)}`);
    if (item.steps) return item.steps.map((step, index) => `${index + 1}. **${line(step.t)}** ${line(step.d)}`);
    if (item.table) {
      return [
        `| ${item.table.head.map(line).join(" | ")} |`,
        `| ${item.table.head.map(() => "---").join(" | ")} |`,
        ...item.table.rows.map((row) => `| ${row.map(line).join(" | ")} |`),
      ];
    }
    return [];
  };
  return [
    `## ${line(content.h1)}`,
    `Source: ${postUrl("en", content)} (updated ${content.updated})`,
    "",
    line(content.lead),
    "",
    ...content.summary.map((item) => `- ${line(item)}`),
    "",
    ...content.sections.flatMap((section) => [`### ${line(section.h2)}`, ...section.blocks.flatMap(block), ""]),
    `### FAQ`,
    ...content.faq.flatMap((item) => [`**${line(item.q)}** ${line(item.a)}`, ""]),
  ];
}

function llmsFull(content) {
  const parts = [
    `# ${line(content.meta.title)}`,
    "",
    line(content.meta.description),
    "",
    `## ${line(content.features.h2)}`,
    line(content.features.sub),
    ...content.features.cards.map((card) => `- **${line(card.h)}**: ${line(card.p)}`),
    "",
    `## ${line(content.how.h2)}`,
    line(content.how.sub),
    ...content.how.steps.map((step, index) => `${index + 1}. **${line(step.t)}** ${line(step.d)}`),
    "",
    `## ${line(content.android.h2)}`,
    line(content.android.sub),
    ...content.android.items.map((item) => `- ${line(item)}`),
    line(content.android.note),
    `Google Play: ${PLAY}`,
    line(content.android.iosNote),
    `App Store: ${APP_STORE}`,
    "",
    `## ${line(content.tiktok.h2)}`,
    line(content.tiktok.sub),
    ...content.tiktok.items.map((item) => `- ${line(item)}`),
    line(content.tiktok.note),
    "",
    `## ${line(content.install.h2)}`,
    ...content.install.steps.map((step, index) => `${index + 1}. **${line(step.t)}** ${line(step.d)}`),
    "",
    `## ${line(content.faq.h2)}`,
    ...content.faq.items.flatMap((item) => [`### ${line(item.q)}`, line(item.a), ""]),
    ...POSTS.flatMap((post) => postText(post.byLang.en)),
    `---`,
    `Taner Özel — ${line(content.footer.role)} · ${REPO}`,
    line(content.footer.note),
    "",
  ];
  return parts.join("\n");
}

// --- build --------------------------------------------------------------

const available = readdirSync(join(root, "site/content"))
  .filter((file) => file.endsWith(".json"))
  .map((file) => file.replace(/\.json$/, ""));
const missing = LOCALES.filter((locale) => !available.includes(locale.code)).map((l) => l.code);
if (missing.length) {
  console.error(`site/content is missing: ${missing.join(", ")}`);
  process.exit(1);
}

// Remove the previous language folders so a removed language cannot linger;
// the same for the English blog, so a removed post cannot either.
for (const locale of LOCALES) {
  const folder = join(root, locale.code === "en" ? blogDir("en") : pathFor(locale.code));
  if (existsSync(folder)) rmSync(folder, { recursive: true });
}

const english = JSON.parse(readFileSync(join(root, "site/content/en.json"), "utf8"));
const keyShape = (value, prefix = "") =>
  typeof value === "object" && value !== null
    ? Object.entries(value).flatMap(([key, inner]) => keyShape(inner, `${prefix}${key}.`))
    : [prefix.slice(0, -1)];
const expected = keyShape(english).join("|");

const homeContent = Object.fromEntries(
  LOCALES.map((locale) => [
    locale.code,
    JSON.parse(readFileSync(join(root, `site/content/${locale.code}.json`), "utf8")),
  ]),
);

for (const locale of LOCALES) {
  const content = homeContent[locale.code];
  if (keyShape(content).join("|") !== expected) {
    console.error(`site/content/${locale.code}.json does not have the same shape as en.json`);
    process.exit(1);
  }
  const folder = join(root, pathFor(locale.code));
  mkdirSync(folder, { recursive: true });
  writeFileSync(join(folder, "index.html"), page(locale, content));
}

// A translation must carry the same labels and, for a post, the same sections
// and blocks as the English text: a missing block is a missing step.
const englishLabels = keyShape(BLOG.find(({ locale }) => locale.code === "en").labels).join("|");
for (const { locale, labels } of BLOG) {
  if (keyShape(labels).join("|") !== englishLabels) {
    console.error(`site/blog/${locale.code}.json does not have the same shape as en.json`);
    process.exit(1);
  }
}
const slugs = new Set();
for (const post of POSTS) {
  if (!post.byLang.en) {
    console.error(`site/blog/posts/${post.id}/en.json is missing`);
    process.exit(1);
  }
  const shape = keyShape(post.byLang.en).join("|");
  for (const [code, content] of Object.entries(post.byLang)) {
    if (keyShape(content).join("|") !== shape) {
      console.error(`site/blog/posts/${post.id}/${code}.json does not have the same shape as en.json`);
      process.exit(1);
    }
    if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(content.slug) || slugs.has(`${code}/${content.slug}`)) {
      console.error(`site/blog/posts/${post.id}/${code}.json: bad or duplicate slug "${content.slug}"`);
      process.exit(1);
    }
    slugs.add(`${code}/${content.slug}`);
  }
}
// The phone screenshots and share images, in every blog language.
for (const { locale } of BLOG) {
  if (!existsSync(join(root, "site/img/app", locale.code, "share.jpg"))) {
    console.error(`site/img/app/${locale.code} is missing: run scripts/build-site-images.sh`);
    process.exit(1);
  }
}
for (const code of Object.keys(SETUP_VIDEOS)) {
  if (!existsSync(join(root, "site/img/video", `${code}.webp`))) {
    console.error(`site/img/video/${code}.webp is missing: run scripts/build-site-images.sh`);
    process.exit(1);
  }
}
for (const folder of ["app", "video"]) {
  rmSync(join(root, "docs/img", folder), { recursive: true, force: true });
  cpSync(join(root, "site/img", folder), join(root, "docs/img", folder), { recursive: true });
}

let blogPages = 0;
for (const { locale, labels } of BLOG) {
  mkdirSync(join(root, blogDir(locale.code)), { recursive: true });
  writeFileSync(join(root, blogDir(locale.code), "index.html"), blogIndexPage(locale, labels));
  blogPages += 1;
  for (const post of POSTS) {
    const content = post.byLang[locale.code];
    if (!content) continue;
    const folder = join(root, blogDir(locale.code), content.slug);
    mkdirSync(folder, { recursive: true });
    writeFileSync(join(folder, "index.html"), postPage(locale, labels, post, content));
    blogPages += 1;
  }
}

// Pages runs Jekyll over docs/ by default; this turns that off so the files
// are served exactly as generated.
writeFileSync(join(root, "docs/privacy-policy.html"), readFileSync(join(root, "site/privacy-policy.html"), "utf8"));
writeFileSync(join(root, "docs/CNAME"), "streammydrone.com\n");
writeFileSync(join(root, "docs/.nojekyll"), "");
writeFileSync(join(root, "docs/sitemap.xml"), sitemap());
writeFileSync(join(root, "docs/robots.txt"), robots());
writeFileSync(join(root, "docs/ai.txt"), aiTxt());
mkdirSync(join(root, "docs/.well-known"), { recursive: true });
writeFileSync(join(root, "docs/.well-known/ai.txt"), aiTxt());
writeFileSync(join(root, "docs/llms.txt"), llms(english));
writeFileSync(join(root, "docs/llms-full.txt"), llmsFull(english));

console.log(`built ${LOCALES.length} pages, ${blogPages} blog pages, sitemap.xml, robots.txt, ai.txt, llms.txt, llms-full.txt`);
