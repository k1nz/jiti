/** 选中捕获：用 epoch 丢掉上一次热键迟到的剪贴板结果。 */

export function shouldApplyCapture(
  eventEpoch: number,
  liveEpoch: number,
  inputDirty: boolean,
): boolean {
  return eventEpoch === liveEpoch && !inputDirty;
}

/** 热键后迟到的剪贴板兜底：只在输入仍为空时采用，避免覆盖已填入的选区。 */
export function shouldApplyDelayedCapture(
  eventEpoch: number,
  liveEpoch: number,
  inputDirty: boolean,
  currentInput: string,
): boolean {
  return shouldApplyCapture(eventEpoch, liveEpoch, inputDirty) && currentInput.trim() === '';
}

/** 翻译/语法热键在拿到非空选区后自动提交。 */
export function shouldAutoSubmitOnCapture(mode: string, text: string): boolean {
  return (mode === 'translate' || mode === 'grammar') && text.trim().length > 0;
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
