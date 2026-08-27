/** >200ms 才展示加载态；更快的操作什么都不显示（ship-readiness 55）。 */

export const LOADING_SPINNER_DELAY_MS = 200;

export function shouldShowDelayedSpinner(input: {
  busy: boolean;
  hasContent: boolean;
  elapsedMs: number;
  delayMs?: number;
}): boolean {
  if (!input.busy || input.hasContent) return false;
  return input.elapsedMs >= (input.delayMs ?? LOADING_SPINNER_DELAY_MS);
}

export function isCancelledEngineError(error: unknown): boolean {
  return Boolean(
    error &&
      typeof error === 'object' &&
      'code' in error &&
      (error as { code: unknown }).code === 'cancelled',
  );
}
