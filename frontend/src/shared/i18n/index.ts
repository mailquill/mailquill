import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import { en } from './en'
import { de } from './de'

export const LANG_STORAGE_KEY = 'mailquill-lang'

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en }, de: { translation: de } },
    fallbackLng: 'en',
    supportedLngs: ['en', 'de'],
    load: 'languageOnly',
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: LANG_STORAGE_KEY,
      caches: ['localStorage'],
    },
    interpolation: { escapeValue: false },
  })

export type LangPref = 'system' | 'en' | 'de'

/** Read the persisted preference ('system' when no explicit choice is stored). */
export function getLangPref(): LangPref {
  const stored = localStorage.getItem(LANG_STORAGE_KEY)
  return stored === 'en' || stored === 'de' ? stored : 'system'
}

/** Apply a language preference; 'system' clears the override and follows the browser. */
export function setLangPref(pref: LangPref) {
  if (pref === 'system') {
    localStorage.removeItem(LANG_STORAGE_KEY)
    const browser = navigator.language.startsWith('de') ? 'de' : 'en'
    i18n.changeLanguage(browser)
  } else {
    i18n.changeLanguage(pref)
  }
}

export default i18n
