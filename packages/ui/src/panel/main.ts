import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import './style.css';

// ── WebView 生存三件套（§4.6 A.1）：空转 rAF 保活 ──────────────────────
// 隐藏（alpha=0, 前置）的 WebView 也可能被 WebKit 判定“不再活跃”而降频；
// 空转 rAF 让帧预算保持分配，首次显示即满帧。
export function keepWarm() {
  requestAnimationFrame(keepWarm);
}
keepWarm();

// ── A.9 字体兜底预热（双语应用必做）──────────────────────────────────
// 第一个 CJK / emoji 出现前预装 PingFang SC / Apple Color Emoji，双 rAF 后移除。
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

createApp(App).use(createPinia()).mount('#app');