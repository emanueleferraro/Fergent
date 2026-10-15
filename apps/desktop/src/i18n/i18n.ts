/**
 * A tiny, dependency-free i18n resolver.
 *
 * English is the default (and only) locale in Phase 0; the catalog is typed
 * so a missing key is a compile error. Additional locales can be registered
 * as more catalogs land in later phases.
 */
import { en, type MessageKey } from './en';

/** The set of supported locales. */
export type Locale = 'en';

/** Interpolation values, e.g. `t('status.theme', { theme: 'Dark' })`. */
export type TranslateParams = Record<string, string | number>;

const catalogs: Record<Locale, Record<MessageKey, string>> = { en };

let currentLocale: Locale = 'en';

/** Switch the active locale. */
export function setLocale(locale: Locale): void {
  currentLocale = locale;
}

/** The active locale. */
export function getLocale(): Locale {
  return currentLocale;
}

/** Resolve a message key, interpolating `{placeholder}` tokens. */
export function t(key: MessageKey, params?: TranslateParams): string {
  const template = catalogs[currentLocale][key];
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (_match, name: string) =>
    name in params ? String(params[name]) : `{${name}}`,
  );
}
