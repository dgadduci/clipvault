import { derived, get, writable } from "svelte/store";
import en from "../locales/en.json" with { type: "json" };
import es from "../locales/es.json" with { type: "json" };
import pt from "../locales/pt.json" with { type: "json" };
import de from "../locales/de.json" with { type: "json" };
import fr from "../locales/fr.json" with { type: "json" };

export const SUPPORTED_LOCALES = ["en", "es", "pt", "de", "fr"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];
type Catalog = Record<string, string>;
type TranslationValue = string | number | Date;
type TranslationParams = Record<string, TranslationValue>;

const catalogs: Record<Locale, Catalog> = { en, es, pt, de, fr };
export const localeStore = writable<Locale>("en");

function formatValue(locale: Locale, value: TranslationValue): string {
  if (value instanceof Date) {
    return new Intl.DateTimeFormat(locale).format(value);
  }
  if (typeof value === "number") {
    return new Intl.NumberFormat(locale).format(value);
  }
  return value;
}

export function translate(
  key: string,
  params: TranslationParams = {},
  activeLocale: Locale = get(localeStore),
): string {
  const message = catalogs[activeLocale][key] ?? catalogs.en[key] ?? key;
  return message.replace(/\{([a-zA-Z0-9_]+)\}/g, (placeholder, name: string) => {
    const value = params[name];
    return value === undefined ? placeholder : formatValue(activeLocale, value);
  });
}

export function translatePlural(
  key: string,
  count: number,
  params: TranslationParams = {},
  activeLocale: Locale = get(localeStore),
): string {
  const category = new Intl.PluralRules(activeLocale).select(count);
  const pluralKey = `${key}.${category}`;
  const selectedKey = catalogs[activeLocale][pluralKey]
    ? pluralKey
    : `${key}.other`;
  return translate(selectedKey, { ...params, count }, activeLocale);
}

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" &&
    (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

export function setLocale(value: unknown): Locale {
  const next = isLocale(value) ? value : "en";
  localeStore.set(next);
  return next;
}

export const t = derived(localeStore, ($locale) =>
  (key: string, params: TranslationParams = {}) =>
    translate(key, params, $locale),
);

export const tPlural = derived(localeStore, ($locale) =>
  (key: string, count: number, params: TranslationParams = {}) =>
    translatePlural(key, count, params, $locale),
);

export const LANGUAGE_CHANGED_EVENT = "clipvault://language-changed";

let initialization: Promise<void> | undefined;

export function initializeLocalization(): Promise<void> {
  if (initialization) return initialization;
  initialization = (async () => {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      await listen<{ language: string }>(LANGUAGE_CHANGED_EVENT, (event) => {
        setLocale(event.payload.language);
      });
    } catch {
      // Browser preview and test environments do not expose Tauri events.
    }

    try {
      const { settingsGetCommand } = await import("./tauri.ts");
      const settings = await settingsGetCommand();
      setLocale(settings.language);
    } catch {
      setLocale("en");
    }
  })();
  return initialization;
}
