import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en/translation.json";

/** First real slice of i18n infra — see the Backlog/i18n research note in
 *  the Obsidian vault for why this is deliberately narrow: only
 *  `Settings.tsx` is converted to `useTranslation()` so far (proof this
 *  actually works end-to-end, matching this project's "prove one thing
 *  really works before claiming coverage" pattern), not every screen.
 *  English is the only real locale; nothing else is wired up here yet —
 *  the "coming soon" language list in Settings.tsx is honest, not a
 *  placeholder for something secretly already working. */
i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
  },
  lng: "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false, // React already escapes.
  },
});

export default i18n;
