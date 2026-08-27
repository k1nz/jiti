import { describe, expect, it } from 'vitest';
import { isCancelledEngineError, shouldShowDelayedSpinner } from '../src/loading';

describe('delayed spinner', () => {
  it('忙碌且无内容时 200ms 后才显示', () => {
    expect(
      shouldShowDelayedSpinner({ busy: true, hasContent: false, elapsedMs: 0 }),
    ).toBe(false);
    expect(
      shouldShowDelayedSpinner({ busy: true, hasContent: false, elapsedMs: 199 }),
    ).toBe(false);
    expect(
      shouldShowDelayedSpinner({ busy: true, hasContent: false, elapsedMs: 200 }),
    ).toBe(true);
  });

  it('已有流式内容或未忙碌时不显示', () => {
    expect(
      shouldShowDelayedSpinner({ busy: true, hasContent: true, elapsedMs: 800 }),
    ).toBe(false);
    expect(
      shouldShowDelayedSpinner({ busy: false, hasContent: false, elapsedMs: 800 }),
    ).toBe(false);
  });

  it('cancelled 错误码可识别', () => {
    expect(isCancelledEngineError({ code: 'cancelled' })).toBe(true);
    expect(isCancelledEngineError({ code: 'network' })).toBe(false);
    expect(isCancelledEngineError(null)).toBe(false);
  });
});
