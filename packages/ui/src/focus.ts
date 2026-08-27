/** 空白 chrome 点击才聚焦搜索框；选中或点在可选文字上时不要抢走选区。 */

const DRAG_THRESHOLD_PX = 4;

export function shouldFocusSearchOnShellClick(input: {
  interactive: boolean;
  selectedText: string;
  selectable: boolean;
  dragDistance: number;
}): boolean {
  if (input.interactive) return false;
  if (input.selectable) return false;
  if (input.selectedText.length > 0) return false;
  if (input.dragDistance > DRAG_THRESHOLD_PX) return false;
  return true;
}

export function isSelectableTextTarget(el: Element | null): boolean {
  let node: Element | null = el;
  while (node instanceof HTMLElement) {
    const value = getComputedStyle(node).userSelect;
    if (value === 'text' || value === 'all') return true;
    node = node.parentElement;
  }
  return false;
}
