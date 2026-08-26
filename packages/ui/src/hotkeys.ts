/** 全局热键录制：KeyboardEvent → tauri-plugin-global-shortcut 可解析的 accelerator。 */

import { ref } from 'vue';

/** 录制进行中：面板级 Esc/Tab 必须让路，否则会关窗或切走设置页。 */
export const isRecordingHotkey = ref(false);

export function shouldHidePanelOnEscape(): boolean {
  return !isRecordingHotkey.value;
}

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
