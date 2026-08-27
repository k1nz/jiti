/** 面板级快捷键：Tab / Shift+Tab 循环切模式；⌘←/→ 仍可用。 */

export type KeyLike = {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
};

export function isHidePanelShortcut(event: KeyLike): boolean {
  return (event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === 'w';
}

/** Tab / Shift+Tab，或 ⌘←/→ · Ctrl+←/→ 循环切模式。裸方向键留给输入框与 Tab 条。 */
export function modeCycleDirection(event: KeyLike): 1 | -1 | 0 {
  if (shouldInterceptTab(event)) return event.shiftKey ? -1 : 1;
  const chord = event.metaKey || event.ctrlKey;
  if (!chord || event.altKey) return 0;
  if (event.key === 'ArrowRight') return 1;
  if (event.key === 'ArrowLeft') return -1;
  return 0;
}

export function shouldInterceptTab(event: KeyLike): boolean {
  if (event.key !== 'Tab') return false;
  return !event.metaKey && !event.ctrlKey && !event.altKey;
}
