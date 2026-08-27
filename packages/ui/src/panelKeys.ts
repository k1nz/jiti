/** 面板级快捷键：Tab 自然遍历，模式切换走方向键/组合键。 */

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

/** ⌘←/→ 或 Ctrl+←/→ 循环切模式。裸方向键留给输入框与 Tab 条。 */
export function modeCycleDirection(event: KeyLike): 1 | -1 | 0 {
  const chord = event.metaKey || event.ctrlKey;
  if (!chord || event.altKey) return 0;
  if (event.key === 'ArrowRight') return 1;
  if (event.key === 'ArrowLeft') return -1;
  return 0;
}

export function shouldInterceptTab(_event: KeyLike): boolean {
  return false;
}
