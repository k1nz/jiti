/** IME 组合输入期间不要触发面板快捷键或提交。 */

export function isImeComposing(event: { isComposing?: boolean; keyCode?: number }): boolean {
  return Boolean(event.isComposing) || event.keyCode === 229;
}

export function shouldHandlePanelShortcut(event: {
  isComposing?: boolean;
  keyCode?: number;
}): boolean {
  return !isImeComposing(event);
}
