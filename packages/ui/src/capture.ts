/** 选中捕获：用 epoch 丢掉上一次热键迟到的剪贴板结果。 */

export function shouldApplyCapture(
  eventEpoch: number,
  liveEpoch: number,
  inputDirty: boolean,
): boolean {
  return eventEpoch === liveEpoch && !inputDirty;
}

/**
 * 浏览器 AX/UIA 经常仍报告上一次选区。
 * 与已经填入输入框的文本相同则视为过期，等剪贴板覆盖。
 */
export function isStalePreferredCapture(
  text: string,
  method: string,
  lastCommitted: string,
): boolean {
  if (method !== 'ax' && method !== 'uia') return false;
  return text.length > 0 && text === lastCommitted;
}
