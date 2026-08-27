import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { bootPreferences } from '../bootstrap';
import { i18n } from '../i18n';
import './style.css';

// ── WebView 生存三件套（§4.6 A.1）：空转 rAF 保活 ──────────────────────
export function keepWarm() {
  requestAnimationFrame(keepWarm);
}
keepWarm();

// ── A.9 字体兜底预热（双语应用必做）──────────────────────────────────
export function prewarmFontFallbacks() {
  const span = document.createElement('span');
  span.setAttribute('aria-hidden', 'true');
  span.style.cssText =
    'position:absolute;left:-9999px;top:0;opacity:0;pointer-events:none;user-select:none;';
  span.textContent = '😀🎉✨📦🚀 中文 日本語 한국어 ∑∫√ ✓✗';
  document.body.appendChild(span);
  void span.getBoundingClientRect();
  requestAnimationFrame(() => requestAnimationFrame(() => span.remove()));
}
prewarmFontFallbacks();

void bootPreferences();
createApp(App).use(createPinia()).use(i18n).mount('#app');
