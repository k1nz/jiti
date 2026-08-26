/** 全局热键录制：KeyboardEvent → tauri-plugin-global-shortcut 可解析的 accelerator。 */

const MODIFIER_CODES = new Set([
  'ShiftLeft',
  'ShiftRight',
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
]);

export type KeyCombo = {
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
};

/** 修饰键单独按下时返回 null；裸字母也拒绝（至少要一个修饰键）。 */
export function acceleratorFromCombo(event: KeyCombo): string | null {
  if (MODIFIER_CODES.has(event.code) || !event.code) return null;
  const parts: string[] = [];
  if (event.shiftKey) parts.push('shift');
  if (event.ctrlKey) parts.push('control');
  if (event.altKey) parts.push('alt');
  if (event.metaKey) parts.push('super');
  if (parts.length === 0) return null;
  parts.push(event.code);
  return parts.join('+');
}
