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
      <a href="${blogHomeFor(locale.code)}">${content.nav.blog}</a>
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

${guidesSection(locale)}
</main>

${footer(content)}

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
<meta property="og:site_name" content="DJI Live Bridge">
<meta property="og:title" content="${escapeHtml(title)}">
<meta property="og:description" content="${escapeHtml(stripTags(description))}">
<meta property="og:image" content="${BASE}/img/go-live.png">
<meta property="og:url" content="${url}">
<meta property="og:type" content="${type}">
<meta property="og:locale" content="${locale.og}">${article}
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escapeHtml(title)}">
<meta name="twitter:description" content="${escapeHtml(stripTags(description))}">
<meta name="twitter:image" content="${BASE}/img/go-live.png">
${jsonld.map((data) => `<script type="application/ld+json">${JSON.stringify(data)}</script>`).join("\n")}
<style>
${css}</style>`;
}

function blogHeader(locale, labels, menuLinks, asset) {
  const menu = languageMenu({ ...locale, nav: labels.language }, menuLinks);
  return `<a class="skip" href="#main">${labels.skip}</a>

<header>
  <div class="wrap bar">
    <a class="brand" href="${urlFor(locale.code)}"><img src="${asset}img/icon.png" alt="" width="30" height="30">DJI Live Bridge</a>
    <nav>
      <a href="${urlFor(locale.code)}">${labels.home}</a>
      <a href="${blogUrl(locale.code)}">${labels.blog}</a>
      ${menu}
      <a class="btn small" href="${RELEASES}">${labels.download}</a>
    </nav>
  </div>
</header>`;
}

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
function renderBlock(block, asset) {
  if (block.p) return `<p>${block.p}</p>`;
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

  const sections = content.sections
    .map(
      (section) =>
        `<h2 id="${section.id}">${section.h2}</h2>\n${section.blocks.map((block) => renderBlock(block, asset)).join("\n")}`,
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
    image: `${BASE}/img/go-live.png`,
    author,
    publisher: author,
    isPartOf: { "@type": "Blog", name: stripTags(labels.meta.title), url: blogUrl(locale.code) },
    about: { "@type": "SoftwareApplication", name: "DJI Live Bridge", url: urlFor(locale.code) },
  };
  const howtoSection = content.sections.find((section) => section.howto);
  const howtoSteps = howtoSection ? howtoSection.blocks.find((block) => block.steps).steps : [];
  const howto = howtoSection && {
    "@context": "https://schema.org",
    "@type": "HowTo",
    name: stripTags(howtoSection.h2),
    inLanguage: locale.hreflang,
    tool: [{ "@type": "HowToTool", name: "DJI Live Bridge" }, { "@type": "HowToTool", name: "DJI Fly" }],
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

<div class="cta-box">
<h2>${labels.cta.h}</h2>
<p>${labels.cta.p}</p>
<div class="cta"><a class="btn primary" href="${RELEASES}">${labels.cta.button}</a><a class="btn" href="${urlFor(locale.code)}">${labels.cta.more}</a></div>
</div>

<h2 id="related">${labels.related}</h2>
${postCards(locale.code, others)}
</article>
</main>

${footer(homeContent[locale.code])}

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
${postCards(locale.code, posts)}
</div>
</main>

${footer(homeContent[locale.code])}

</body>
</html>
`;
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
- [Full site text](${BASE}/llms-full.txt): every section of the home page and every guide as plain text.
- [Blog](${BASE}/blog/): step-by-step guides for each streaming platform, in English and Turkish.
- [README](${REPO}/blob/main/README.md): installation, streaming guide, development notes.
- [Turkish README](${REPO}/blob/main/README.tr.md): the same document in Turkish; 13 further languages sit beside it.
- [Releases](${RELEASES}): downloads for macOS and Windows.
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
writeFileSync(join(root, "docs/.nojekyll"), "");
writeFileSync(join(root, "docs/sitemap.xml"), sitemap());
writeFileSync(join(root, "docs/robots.txt"), robots());
writeFileSync(join(root, "docs/llms.txt"), llms(english));
writeFileSync(join(root, "docs/llms-full.txt"), llmsFull(english));

console.log(`built ${LOCALES.length} pages, ${blogPages} blog pages, sitemap.xml, robots.txt, llms.txt, llms-full.txt`);
