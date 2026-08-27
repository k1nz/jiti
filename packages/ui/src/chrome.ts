/** 去掉 WebKit/WebView2 浏览器右键菜单（Inspect / Reload / 词典）。 */

export function disableBrowserChrome() {
  document.addEventListener(
    'contextmenu',
    (event) => {
      event.preventDefault();
    },
    { capture: true },
  );
}
