import { createI18n } from 'vue-i18n';
import { applyDocumentLocale, resolveLocale, type LocalePref } from '../appearance';
import enUS from './en-US.json';
import zhCN from './zh-CN.json';

export const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
  },
});

export function syncI18n(pref: LocalePref) {
  const locale = resolveLocale(pref);
  i18n.global.locale.value = locale;
  applyDocumentLocale(locale);
}
