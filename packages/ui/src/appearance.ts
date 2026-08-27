export type LocalePref = 'system' | 'zh-CN' | 'en-US';
export type ThemePref = 'system' | 'light' | 'dark';
export type ResolvedLocale = 'zh-CN' | 'en-US';

export function systemLanguage(): string {
  if (typeof navigator === 'undefined') return 'zh-CN';
  return navigator.language || 'zh-CN';
}

export function resolveLocale(
  pref: LocalePref,
  language = systemLanguage(),
): ResolvedLocale {
  if (pref === 'zh-CN' || pref === 'en-US') return pref;
  return language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';
}

export function themeDatasetValue(theme: ThemePref): 'light' | 'dark' | null {
  return theme === 'system' ? null : theme;
}

export function applyTheme(theme: ThemePref) {
  if (typeof document === 'undefined') return;
  const value = themeDatasetValue(theme);
  if (value) document.documentElement.setAttribute('data-theme', value);
  else document.documentElement.removeAttribute('data-theme');
}

export function applyDocumentLocale(locale: ResolvedLocale) {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = locale;
}
