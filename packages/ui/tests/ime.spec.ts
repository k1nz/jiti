import { describe, expect, it } from 'vitest';
import { isImeComposing, shouldHandlePanelShortcut } from '../src/ime';

describe('isImeComposing', () => {
  it('isComposing 为真时视为组合输入', () => {
    expect(isImeComposing({ isComposing: true, keyCode: 13 })).toBe(true);
  });

  it('keyCode 229 视为组合输入', () => {
    expect(isImeComposing({ isComposing: false, keyCode: 229 })).toBe(true);
  });

  it('普通按键可以处理', () => {
    expect(isImeComposing({ isComposing: false, keyCode: 13 })).toBe(false);
    expect(shouldHandlePanelShortcut({ isComposing: false, keyCode: 27 })).toBe(true);
    expect(shouldHandlePanelShortcut({ isComposing: true })).toBe(false);
  });
});
