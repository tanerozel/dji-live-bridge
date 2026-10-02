// Checks the generated site in docs/ the way a crawler would read it.
//
// The point is that a mistake here is invisible: a page still looks fine in a
// browser while pointing every language at the same canonical URL, or shipping
// English text under lang="ja". So every claim the pages make about themselves
// is verified against what they should say.
//
// Run: npm run check:site

import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BASE = "https://tanerozel.github.io/dji-live-bridge";
// Keep in step with scripts/build-site.mjs.
const ANALYTICS_ID = "G-2HCY063P0W";

const LOCALES = [
  { code: "en", dir: "ltr", hreflang: "en" },
  { code: "tr", dir: "ltr", hreflang: "tr" },
  { code: "es", dir: "ltr", hreflang: "es" },
  { code: "zh", dir: "ltr", hreflang: "zh-Hans" },
  { code: "zh-Hant", dir: "ltr", hreflang: "zh-Hant" },
  { code: "ar", dir: "rtl", hreflang: "ar" },
  { code: "hi", dir: "ltr", hreflang: "hi" },
  { code: "pt", dir: "ltr", hreflang: "pt" },
  { code: "ru", dir: "ltr", hreflang: "ru" },
  { code: "fr", dir: "ltr", hreflang: "fr" },
  { code: "de", dir: "ltr", hreflang: "de" },
  { code: "ja", dir: "ltr", hreflang: "ja" },
  { code: "ko", dir: "ltr", hreflang: "ko" },
  { code: "id", dir: "ltr", hreflang: "id" },
  { code: "it", dir: "ltr", hreflang: "it" },
];

const problems = [];
const warnings = [];
const fail = (where, message) => problems.push(`${where}: ${message}`);
const warn = (where, message) => warnings.push(`${where}: ${message}`);

const urlFor = (code) => (code === "en" ? `${BASE}/` : `${BASE}/${code}/`);
const fileFor = (code) => join(root, code === "en" ? "docs/index.html" : `docs/${code}/index.html`);
const pick = (html, pattern) => (html.match(pattern) || [])[1];

const titles = new Map();
const descriptions = new Map();
let englishTitle = "";

const sourceCss = readFileSync(join(root, "site/styles.css"), "utf8");
if (/nav\s+a:not\(\.btn\)\s*\{\s*display\s*:\s*none/.test(sourceCss)) {
  fail("site/styles.css", "mobile navigation rule also hides the language menu links");
}

for (const locale of LOCALES) {
  const file = fileFor(locale.code);
  const where = locale.code;
  if (!existsSync(file)) {
    fail(where, "page is missing");
    continue;
  }
  const html = readFileSync(file, "utf8");

  const lang = pick(html, /<html lang="([^"]+)"/);
  if (lang !== locale.hreflang) fail(where, `lang="${lang}" should be "${locale.hreflang}"`);
  const dir = pick(html, /<html [^>]*dir="([^"]+)"/);
  if (dir !== locale.dir) fail(where, `dir="${dir}" should be "${locale.dir}"`);

  const title = pick(html, /<title>([^<]*)<\/title>/);
  const description = pick(html, /<meta name="description" content="([^"]*)"/);
  if (!title) fail(where, "no title");
  if (!description) fail(where, "no meta description");
  if (locale.code === "en") englishTitle = title;
  if (title && titles.has(title)) fail(where, `title is identical to ${titles.get(title)}`);
  if (title) titles.set(title, where);
  if (description && descriptions.has(description)) {
    fail(where, `description is identical to ${descriptions.get(description)}`);
  }
  if (description) descriptions.set(description, where);
  if (title && title.length > 70) warn(where, `title is ${title.length} characters`);
  if (description && (description.length < 110 || description.length > 185)) {
    warn(where, `description is ${description.length} characters`);
  }

  const canonical = pick(html, /<link rel="canonical" href="([^"]+)"/);
  if (canonical !== urlFor(locale.code)) {
    fail(where, `canonical is ${canonical}, expected ${urlFor(locale.code)}`);
  }

  for (const other of LOCALES) {
    const alternate = `<link rel="alternate" hreflang="${other.hreflang}" href="${urlFor(other.code)}">`;
    if (!html.includes(alternate)) fail(where, `missing hreflang for ${other.hreflang}`);
  }
  if (!html.includes(`<link rel="alternate" hreflang="x-default" href="${BASE}/">`)) {
    fail(where, "missing hreflang x-default");
  }

  const headings = html.match(/<h1[^>]*>/g) || [];
  if (headings.length !== 1) fail(where, `${headings.length} <h1> elements, expected 1`);

  const blocks = [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)];
  if (blocks.length !== 3) fail(where, `${blocks.length} JSON-LD blocks, expected 3`);
  for (const [, body] of blocks) {
    try {
      const data = JSON.parse(body);
      if (data["@type"] === "FAQPage" && data.mainEntity.length !== 8) {
        fail(where, `FAQ structured data has ${data.mainEntity.length} questions, expected 8`);
      }
      if (data["@type"] === "SoftwareApplication" && data.url !== urlFor(locale.code)) {
        fail(where, "SoftwareApplication url does not match the page");
      }
    } catch (error) {
      fail(where, `JSON-LD does not parse: ${error.message}`);
    }
  }

  for (const [, tag] of html.matchAll(/<img ([^>]+)>/g)) {
    const alt = (tag.match(/alt="([^"]*)"/) || [])[1];
    if (alt === undefined) fail(where, "an <img> has no alt attribute");
    const source = (tag.match(/src="([^"]+)"/) || [])[1];
    const asset = join(root, locale.code === "en" ? "docs" : `docs/${locale.code}`, source);
    if (!existsSync(asset)) fail(where, `image not found: ${source}`);
    if (!/width="\d+"/.test(tag) || !/height="\d+"/.test(tag)) {
      warn(where, `image without width/height: ${source}`);
    }
  }

  if (/undefined|\[object Object\]/.test(html)) fail(where, "rendered 'undefined' or '[object Object]'");

  // The analytics tag is easy to lose in a template change and impossible to
  // notice afterwards: the pages look identical, the numbers just stop.
  if (ANALYTICS_ID) {
    const loader = `<script async src="https://www.googletagmanager.com/gtag/js?id=${ANALYTICS_ID}"></script>`;
    if (!html.includes(loader)) fail(where, "the analytics tag is missing");
    if (!html.includes(`gtag('config', '${ANALYTICS_ID}')`)) {
      fail(where, "the analytics tag is not configured");
    }
  }

  // A page that still carries the English headline was never translated.
  if (locale.code !== "en" && title === englishTitle) fail(where, "title is still the English one");

  const body = html.split("<body>")[1] || "";
  const text = body.replace(/<[^>]+>/g, " ");
  if (text.includes("Go live from your DJI drone") && locale.code !== "en") {
    fail(where, "body still contains the English headline");
  }
}

// --- blog ----------------------------------------------------------------
//
// The expected pages are worked out from the sources in site/blog, not from
// what the build happened to write, so a post the build skipped is caught.

const blogUrl = (code) => (code === "en" ? `${BASE}/blog/` : `${BASE}/${code}/blog/`);
const blogFile = (code, slug) =>
  join(root, code === "en" ? "docs/blog" : `docs/${code}/blog`, slug ? `${slug}/index.html` : "index.html");
const readJson = (path) => JSON.parse(readFileSync(join(root, path), "utf8"));

const blogLocales = LOCALES.filter((locale) => existsSync(join(root, `site/blog/${locale.code}.json`)));
const blogPages = [];
if (blogLocales.length) {
  const indexLanguages = blogLocales.map((locale) => ({ locale, url: blogUrl(locale.code) }));
  for (const locale of blogLocales) {
    blogPages.push({ locale, file: blogFile(locale.code), url: blogUrl(locale.code), languages: indexLanguages, kind: "index" });
  }
  for (const id of readJson("site/blog/posts.json")) {
    const languages = blogLocales
      .filter((locale) => existsSync(join(root, `site/blog/posts/${id}/${locale.code}.json`)))
      .map((locale) => {
        const { slug } = readJson(`site/blog/posts/${id}/${locale.code}.json`);
        return { locale, slug, url: `${blogUrl(locale.code)}${slug}/` };
      });
    if (!languages.some(({ locale }) => locale.code === "en")) fail(`blog/${id}`, "has no English version");
    for (const entry of languages) {
      blogPages.push({ locale: entry.locale, file: blogFile(entry.locale.code, entry.slug), url: entry.url, languages, kind: "post" });
    }
  }
}

for (const pageInfo of blogPages) {
  const { locale, file, url, languages, kind } = pageInfo;
  const where = url.replace(BASE, "");
  if (!existsSync(file)) {
    fail(where, "page is missing");
    continue;
  }
  const html = readFileSync(file, "utf8");

  if (pick(html, /<html lang="([^"]+)"/) !== locale.hreflang) fail(where, `lang should be "${locale.hreflang}"`);
  if (pick(html, /<html [^>]*dir="([^"]+)"/) !== locale.dir) fail(where, `dir should be "${locale.dir}"`);

  const title = pick(html, /<title>([^<]*)<\/title>/);
  const description = pick(html, /<meta name="description" content="([^"]*)"/);
  if (!title) fail(where, "no title");
  if (!description) fail(where, "no meta description");
  if (title && titles.has(title)) fail(where, `title is identical to ${titles.get(title)}`);
  if (title) titles.set(title, where);
  if (description && descriptions.has(description)) fail(where, `description is identical to ${descriptions.get(description)}`);
  if (description) descriptions.set(description, where);
  if (title && title.length > 75) warn(where, `title is ${title.length} characters`);
  if (description && (description.length < 110 || description.length > 185)) {
    warn(where, `description is ${description.length} characters`);
  }

  const canonical = pick(html, /<link rel="canonical" href="([^"]+)"/);
  if (canonical !== url) fail(where, `canonical is ${canonical}, expected ${url}`);

  // hreflang must name exactly the languages the page exists in, and each of
  // those pages names this one back: a one-way pair is ignored by Google.
  const alternates = [...html.matchAll(/<link rel="alternate" hreflang="([^"]+)" href="([^"]+)">/g)].map(
    ([, hreflang, href]) => `${hreflang} ${href}`,
  );
  const english = languages.find(({ locale: other }) => other.code === "en");
  const expectedAlternates = [
    ...languages.map(({ locale: other, url: href }) => `${other.hreflang} ${href}`),
    `x-default ${english.url}`,
  ];
  if (alternates.sort().join("|") !== expectedAlternates.sort().join("|")) {
    fail(where, `hreflang alternates are ${alternates.join(", ")}`);
  }

  const headings = html.match(/<h1[^>]*>/g) || [];
  if (headings.length !== 1) fail(where, `${headings.length} <h1> elements, expected 1`);

  const types = [];
  for (const [, body] of html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)) {
    try {
      const data = JSON.parse(body);
      types.push(data["@type"]);
      if (data["@type"] === "BlogPosting" && data.url !== url) fail(where, "BlogPosting url does not match the page");
      if (data["@type"] === "FAQPage" && data.mainEntity.length < 3) fail(where, "FAQ has fewer than 3 questions");
    } catch (error) {
      fail(where, `JSON-LD does not parse: ${error.message}`);
    }
  }
  const expectedTypes = kind === "post" ? ["BlogPosting", "FAQPage", "BreadcrumbList"] : ["Blog", "BreadcrumbList"];
  for (const type of expectedTypes) if (!types.includes(type)) fail(where, `no ${type} structured data`);

  for (const [, tag] of html.matchAll(/<img ([^>]+)>/g)) {
    if (!/alt="[^"]*"/.test(tag)) fail(where, "an <img> has no alt attribute");
    const source = (tag.match(/src="([^"]+)"/) || [])[1];
    if (!existsSync(join(dirname(file), source))) fail(where, `image not found: ${source}`);
  }
  // Relative links (between posts, up to the site) must land on a real page.
  for (const [, href] of html.matchAll(/<a [^>]*href="(\.\.\/[^"#]*)"/g)) {
    const target = join(dirname(file), href, href.endsWith("/") ? "index.html" : "");
    if (!existsSync(target)) fail(where, `broken link: ${href}`);
  }
  for (const [, href] of html.matchAll(/<a [^>]*href="(https:\/\/tanerozel\.github\.io\/dji-live-bridge\/[^"#]*)"/g)) {
    const path = href.slice(BASE.length + 1);
    const target = join(root, "docs", path, path === "" || path.endsWith("/") ? "index.html" : "");
    if (!existsSync(target)) fail(where, `broken link: ${href}`);
  }

  if (/undefined|\[object Object\]/.test(html)) fail(where, "rendered 'undefined' or '[object Object]'");
  if (ANALYTICS_ID && !html.includes(`gtag('config', '${ANALYTICS_ID}')`)) fail(where, "the analytics tag is missing");
}

// Site-wide files.
const sitemapFile = join(root, "docs/sitemap.xml");
if (!existsSync(sitemapFile)) {
  fail("sitemap.xml", "missing");
} else {
  const sitemap = readFileSync(sitemapFile, "utf8");
  const locations = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
  const expected = [...LOCALES.map((locale) => urlFor(locale.code)), ...blogPages.map((entry) => entry.url)];
  if (locations.length !== expected.length) {
    fail("sitemap.xml", `${locations.length} URLs, expected ${expected.length}`);
  }
  for (const location of expected) {
    if (!locations.includes(location)) fail("sitemap.xml", `missing ${location}`);
  }
}

const robotsFile = join(root, "docs/robots.txt");
if (!existsSync(robotsFile)) fail("robots.txt", "missing");
else if (!readFileSync(robotsFile, "utf8").includes(`${BASE}/sitemap.xml`)) {
  fail("robots.txt", "does not point at the sitemap");
}

for (const name of ["llms.txt", "llms-full.txt"]) {
  const file = join(root, "docs", name);
  if (!existsSync(file)) fail(name, "missing");
  else if (!readFileSync(file, "utf8").startsWith("# ")) fail(name, "does not start with a heading");
}

for (const line of warnings) console.warn(`warning  ${line}`);
if (problems.length) {
  for (const line of problems) console.error(`error    ${line}`);
  console.error(`\n${problems.length} problem(s) found.`);
  process.exit(1);
}
console.log(`site checks passed for ${LOCALES.length} languages and ${blogPages.length} blog pages (${warnings.length} warning(s)).`);
