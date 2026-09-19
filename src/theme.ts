export const THEMES = ["system", "light", "dark", "midnight", "sand"] as const;
export type Theme = (typeof THEMES)[number];

const STORAGE_KEY = "dji-live-bridge.theme";
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

export function loadTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && (THEMES as readonly string[]).includes(stored)) return stored as Theme;
  } catch {
    // Storage unavailable; fall through to the default.
  }
  return "light";
}

export function saveTheme(theme: Theme) {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // The theme still applies for this session.
  }
}

/** Sets `data-theme` on <html>; "system" follows the macOS appearance. */
export function applyTheme(theme: Theme) {
  const resolved = theme === "system" ? (darkQuery().matches ? "dark" : "light") : theme;
  document.documentElement.dataset.theme = resolved;
}

/** Re-applies "system" when macOS switches between light and dark. */
export function watchSystemTheme(theme: Theme) {
  if (theme !== "system") return () => {};
  const query = darkQuery();
  const update = () => applyTheme("system");
  query.addEventListener("change", update);
  return () => query.removeEventListener("change", update);
}
