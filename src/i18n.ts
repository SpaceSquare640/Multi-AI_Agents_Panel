import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en/translation.json";
import zhHant from "./locales/zh-Hant/translation.json";
import zhHans from "./locales/zh-Hans/translation.json";
import fr from "./locales/fr/translation.json";
import de from "./locales/de/translation.json";
import ja from "./locales/ja/translation.json";
import ko from "./locales/ko/translation.json";

export const LANGUAGE_STORAGE_KEY = "multi-ai-agents-panel:language";

/** All seven languages now ship real translations (see the Backlog/i18n
 *  research note in the Obsidian vault for the earlier "English only"
 *  state this replaces). English remains the fallback for any key a
 *  translation is missing or a future 8th language doesn't cover yet. */
i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    "zh-Hant": { translation: zhHant },
    "zh-Hans": { translation: zhHans },
    fr: { translation: fr },
    de: { translation: de },
    ja: { translation: ja },
    ko: { translation: ko },
  },
  lng: localStorage.getItem(LANGUAGE_STORAGE_KEY) ?? "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false, // React already escapes.
  },
});

export default i18n;
