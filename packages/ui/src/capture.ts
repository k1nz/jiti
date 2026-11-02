/** 选中捕获：用 epoch 丢掉上一次热键迟到的剪贴板结果。 */

export function shouldApplyCapture(
  eventEpoch: number,
  liveEpoch: number,
  inputDirty: boolean,
): boolean {
  return eventEpoch === liveEpoch && !inputDirty;
}

/**
 * 嵌入式浏览器（Foxmail 读信框）的 UIA Value 经常是页面地址，
 * 例如 `file:///.../mail_frame.html?foxhandler=...`，不是选区。
 */
export function isDocumentLocation(text: string): boolean {
  const value = text.replace(/\0/g, '').trim();
  if (!value || value.length > 2048 || /\s/u.test(value)) return false;
  const lower = value.toLowerCase();
  return (
    lower.startsWith('file://') ||
    lower.startsWith('http://') ||
    lower.startsWith('https://') ||
    lower.startsWith('about:')
  );
}

/** 热键后迟到的剪贴板兜底：输入仍为空，或里面只是页面地址时采用。 */
export function shouldApplyDelayedCapture(
  eventEpoch: number,
  liveEpoch: number,
  inputDirty: boolean,
  currentInput: string,
): boolean {
  return (
    shouldApplyCapture(eventEpoch, liveEpoch, inputDirty) &&
    (currentInput.trim() === '' || isDocumentLocation(currentInput))
  );
}

/** 翻译、语法、句法在拿到非空选区后自动提交。 */
export function shouldAutoSubmitOnCapture(mode: string, text: string): boolean {
  return (mode === 'translate' || mode === 'grammar' || mode === 'syntax') && text.trim().length > 0;
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
