import ar from "./locales/ar.json";
import de from "./locales/de.json";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import hi from "./locales/hi.json";
import id from "./locales/id.json";
import it from "./locales/it.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import pt from "./locales/pt.json";
import ru from "./locales/ru.json";
import tr from "./locales/tr.json";
import zh from "./locales/zh.json";
import zhHant from "./locales/zh-Hant.json";

export type TranslationParams = Record<string, string | number>;
export type TranslationKey = keyof typeof en;
export type Language = keyof typeof dictionaries;

const dictionaries = { en, tr, es, zh, "zh-Hant": zhHant, ar, hi, pt, ru, fr, de, ja, ko, id, it };

/** Names are written in each language itself, as language pickers should be. */
export const LANGUAGES: { code: Language; name: string }[] = [
  { code: "en", name: "English" },
  { code: "es", name: "Español" },
  { code: "zh", name: "中文（简体）" },
  { code: "zh-Hant", name: "中文（繁體）" },
  { code: "ar", name: "العربية" },
  { code: "hi", name: "हिन्दी" },
  { code: "pt", name: "Português" },
  { code: "ru", name: "Русский" },
  { code: "fr", name: "Français" },
  { code: "de", name: "Deutsch" },
  { code: "ja", name: "日本語" },
  { code: "ko", name: "한국어" },
  { code: "id", name: "Bahasa Indonesia" },
  { code: "it", name: "Italiano" },
  { code: "tr", name: "Türkçe" },
];

const RTL_LANGUAGES: Language[] = ["ar"];
const STORAGE_KEY = "dji-live-bridge.language";

function isLanguage(value: string): value is Language {
  return value in dictionaries;
}

export function loadLanguage(): Language {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && isLanguage(stored)) return stored;
  } catch {
    // Storage unavailable; fall back to the system language.
  }
  for (const candidate of navigator.languages ?? [navigator.language]) {
    const tag = candidate.toLowerCase();
    // Traditional Chinese regions must not fall back to the Simplified file.
    if (tag.startsWith("zh")) {
      return /hant|tw|hk|mo/.test(tag) ? "zh-Hant" : "zh";
    }
    // "pt-BR" still matches "pt".
    const base = tag.split("-")[0];
    if (isLanguage(base)) return base;
  }
  return "en";
}

export function saveLanguage(language: Language) {
  try {
    localStorage.setItem(STORAGE_KEY, language);
  } catch {
    // The choice still applies for this session.
  }
  applyLanguage(language);
}

/** Sets <html lang> and switches the layout direction for Arabic. */
export function applyLanguage(language: Language) {
  document.documentElement.lang = language;
  document.documentElement.dir = RTL_LANGUAGES.includes(language) ? "rtl" : "ltr";
}

export function translate(language: Language, key: string, params: TranslationParams = {}) {
  const dictionary = dictionaries[language] as Record<string, string>;
  const template = dictionary[key] ?? (en as Record<string, string>)[key] ?? key;
  return Object.entries(params).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, String(value)),
    template,
  );
}

/**
 * Every language must define every key: a missing one would silently fall back
 * to English mid-sentence, and an extra one is a typo that never renders.
 */
export function assertTranslationParity() {
  const expected = Object.keys(en).sort();
  for (const [code, dictionary] of Object.entries(dictionaries)) {
    const actual = Object.keys(dictionary).sort();
    if (actual.join("\n") !== expected.join("\n")) {
      const missing = expected.filter((key) => !actual.includes(key));
      const extra = actual.filter((key) => !expected.includes(key));
      throw new Error(
        `Locale "${code}" does not match English. Missing: ${missing.join(", ") || "none"}. Extra: ${extra.join(", ") || "none"}.`,
      );
    }
  }
  return true;
}

assertTranslationParity();
