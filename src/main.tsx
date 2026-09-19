import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyLanguage, loadLanguage } from "./i18n";
import { applyTheme, loadTheme } from "./theme";
import "./styles.css";

// Before the first paint: no white flash, and Arabic starts right-to-left.
applyTheme(loadTheme());
applyLanguage(loadLanguage());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
