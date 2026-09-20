// Generates the GitHub Pages site into docs/ from site/content/<lang>.json.
//
// One page per language at its own URL (English at the root, the rest under
// /<code>/), because a search engine can only rank a language it can crawl at
// a stable address — a page that swaps its text with JavaScript cannot be
// indexed per language. Every page therefore carries its own title, canonical
// URL and the full set of hreflang alternates, and the language switcher is
// made of real links.
//
// Run: npm run build:site

import { readFileSync, writeFileSync, mkdirSync, rmSync, existsSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BASE = "https://tanerozel.github.io/dji-live-bridge";
const REPO = "https://github.com/tanerozel/dji-live-bridge";
const RELEASES = `${REPO}/releases/latest`;

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

function head(locale, content) {
  const url = urlFor(locale.code);
  const asset = assetPrefix(locale.code);
  const alternates = LOCALES.map(
    (other) => `<link rel="alternate" hreflang="${other.hreflang}" href="${urlFor(other.code)}">`,
  ).join("\n");

  const software = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "DJI Live Bridge",
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
<meta property="og:site_name" content="DJI Live Bridge">
<meta property="og:title" content="${escapeHtml(content.meta.ogTitle)}">
<meta property="og:description" content="${escapeHtml(stripTags(content.meta.ogDescription))}">
<meta property="og:image" content="${BASE}/img/go-live.png">
<meta property="og:image:alt" content="${escapeHtml(stripTags(content.hero.shotAlt))}">
<meta property="og:url" content="${url}">
<meta property="og:type" content="website">
<meta property="og:locale" content="${locale.og}">
${LOCALES.filter((other) => other.code !== locale.code)
  .map((other) => `<meta property="og:locale:alternate" content="${other.og}">`)
  .join("\n")}
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escapeHtml(content.meta.ogTitle)}">
<meta name="twitter:description" content="${escapeHtml(stripTags(content.meta.ogDescription))}">
<meta name="twitter:image" content="${BASE}/img/go-live.png">
<script type="application/ld+json">${JSON.stringify(software)}</script>
<script type="application/ld+json">${JSON.stringify(faq)}</script>
<style>
${css}</style>`;
}

function languageMenu(locale) {
  const items = LOCALES.map(
    (other) =>
      `<a href="${urlFor(other.code)}" hreflang="${other.hreflang}" lang="${other.hreflang}"${
        other.code === locale.code ? ' aria-current="true"' : ""
      }>${other.name}</a>`,
  ).join("");
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
    <a class="brand" href="${urlFor(locale.code)}"><img src="${asset}img/icon.png" alt="" width="30" height="30">DJI Live Bridge</a>
    <nav>
      <a href="#features">${content.nav.features}</a>
      <a href="#how">${content.nav.how}</a>
      <a href="#tiktok">${content.nav.tiktok}</a>
      <a href="#faq">${content.nav.faq}</a>
      ${menu}
      <a class="btn small" href="${REPO}">GitHub</a>
    </nav>
  </div>
</header>

<main id="main">

<div class="wrap hero">
  <span class="pill"><span class="dot"></span>${content.hero.pill}</span>
  <h1>${content.hero.h1}</h1>
  <p class="lead">${content.hero.lead}</p>
  <div class="cta">
    <a class="btn primary" href="${RELEASES}">${content.hero.ctaMac}</a>
    <a class="btn" href="${RELEASES}">${content.hero.ctaWin}</a>
    <a class="btn" href="${REPO}">${content.hero.ctaSource}</a>
  </div>
  <p class="meta">${content.hero.meta}</p>
  <div class="shot"><img src="${asset}img/go-live.png" alt="${escapeHtml(stripTags(content.hero.shotAlt))}" width="1600" height="1260" fetchpriority="high"></div>
  <p class="caption">${content.hero.caption}</p>
</div>

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
      <a class="btn primary" href="${RELEASES}">${content.install.cta}</a>
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

</main>

<footer>
  <div class="wrap">
    <strong style="color:var(--text)">Taner Özel</strong> — ${content.footer.role}
    <div class="foot-links">
      <a href="mailto:tanerozel47@gmail.com">tanerozel47@gmail.com</a>
      <a href="https://github.com/tanerozel">GitHub</a>
      <a href="https://www.linkedin.com/in/tanerozel">LinkedIn</a>
      <a href="${REPO}/issues">${content.footer.issue}</a>
      <a href="${BASE}/llms.txt">llms.txt</a>
    </div>
    <p class="note">${content.footer.note}</p>
  </div>
</footer>

</body>
</html>
`;
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
    <changefreq>weekly</changefreq>
    <priority>${locale.code === "en" ? "1.0" : "0.8"}</priority>
  </url>`;
  }).join("\n");

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${entries}
</urlset>
`;
}

const robots = () => `User-agent: *
Allow: /

Sitemap: ${BASE}/sitemap.xml
`;

// llmstxt.org format: a short, factual map of the project for language models,
// which otherwise have to guess from the rendered page.
function llms(content) {
  return `# DJI Live Bridge

> A free, open-source desktop app (macOS and Windows) that receives the RTMP stream DJI Fly sends
> from a DJI drone and restreams it to Instagram, TikTok and any other RTMP destination at the same
> time, or exposes it as a virtual camera named "DJI Live Bridge Camera" for TikTok LIVE Studio,
> OBS and Zoom.

Everything runs locally: the drone video never passes through a third-party server. FFmpeg and the
media server are bundled, so nothing else has to be installed. Stream keys are kept in the operating
system's credential vault. Version ${version}. MIT licence; the bundled FFmpeg is LGPL, built
without GPL components.

Key facts:
- Platforms: macOS 13 or newer (Apple Silicon and Intel, signed and notarized), Windows 10/11 (beta, unsigned installer).
- Output: 1080x1920 portrait, 30 fps, 6 Mbps CBR, H.264 High, 2-second keyframes; empty space filled with a blurred copy of the picture.
- Input: whatever DJI Fly sends over RTMP, often 720p from the controller.
- The virtual camera carries video only, so TikTok LIVE Studio keeps control of audio.
- Not affiliated with DJI, Instagram, TikTok or Apple.

## Documentation
- [Home page](${BASE}/): what the app does, how to set it up, FAQ.
- [Full site text](${BASE}/llms-full.txt): every section of the home page as plain text.
- [README](${REPO}/blob/main/README.md): installation, streaming guide, development notes.
- [Turkish README](${REPO}/blob/main/README.tr.md): the same document in Turkish; 13 further languages sit beside it.
- [Releases](${RELEASES}): downloads for macOS and Windows.
- [Dependencies](${BASE}/DEPENDENCIES.md): third-party components and their licences.
- [Security policy](${BASE}/SECURITY.md): how to report a vulnerability.

## Languages
${LOCALES.map((locale) => `- [${locale.name}](${urlFor(locale.code)})`).join("\n")}

## Source
- [Repository](${REPO})
- [Issue tracker](${REPO}/issues)
`;
}

function llmsFull(content) {
  const line = (text) => stripTags(text).replace(/\s+/g, " ").trim();
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

// Remove the previous language folders so a removed language cannot linger.
for (const locale of LOCALES) {
  if (locale.code === "en") continue;
  const folder = join(root, pathFor(locale.code));
  if (existsSync(folder)) rmSync(folder, { recursive: true });
}

const english = JSON.parse(readFileSync(join(root, "site/content/en.json"), "utf8"));
const keyShape = (value, prefix = "") =>
  typeof value === "object" && value !== null
    ? Object.entries(value).flatMap(([key, inner]) => keyShape(inner, `${prefix}${key}.`))
    : [prefix.slice(0, -1)];
const expected = keyShape(english).join("|");

for (const locale of LOCALES) {
  const content = JSON.parse(readFileSync(join(root, `site/content/${locale.code}.json`), "utf8"));
  if (keyShape(content).join("|") !== expected) {
    console.error(`site/content/${locale.code}.json does not have the same shape as en.json`);
    process.exit(1);
  }
  const folder = join(root, pathFor(locale.code));
  mkdirSync(folder, { recursive: true });
  writeFileSync(join(folder, "index.html"), page(locale, content));
}

// Pages runs Jekyll over docs/ by default; this turns that off so the files
// are served exactly as generated.
writeFileSync(join(root, "docs/.nojekyll"), "");
writeFileSync(join(root, "docs/sitemap.xml"), sitemap());
writeFileSync(join(root, "docs/robots.txt"), robots());
writeFileSync(join(root, "docs/llms.txt"), llms(english));
writeFileSync(join(root, "docs/llms-full.txt"), llmsFull(english));

console.log(`built ${LOCALES.length} pages, sitemap.xml, robots.txt, llms.txt, llms-full.txt`);
