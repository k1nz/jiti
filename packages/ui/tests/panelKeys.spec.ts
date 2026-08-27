import { describe, expect, it } from 'vitest';
import { isHidePanelShortcut, modeCycleDirection, shouldInterceptTab } from '../src/panelKeys';

describe('panelKeys', () => {
  const base = { key: 'Tab', metaKey: false, ctrlKey: false, altKey: false, shiftKey: false };

  it('Tab / Shift+Tab 拦截并切模式', () => {
    expect(shouldInterceptTab(base)).toBe(true);
    expect(shouldInterceptTab({ ...base, shiftKey: true })).toBe(true);
    expect(modeCycleDirection(base)).toBe(1);
    expect(modeCycleDirection({ ...base, shiftKey: true })).toBe(-1);
    expect(shouldInterceptTab({ ...base, metaKey: true })).toBe(false);
  });

  it('⌘W / Ctrl+W 隐藏面板', () => {
    expect(isHidePanelShortcut({ ...base, key: 'w', metaKey: true })).toBe(true);
    expect(isHidePanelShortcut({ ...base, key: 'W', ctrlKey: true })).toBe(true);
    expect(isHidePanelShortcut({ ...base, key: 'w' })).toBe(false);
  });

  it('⌘←/→ 切模式，裸方向键不切', () => {
    expect(modeCycleDirection({ ...base, key: 'ArrowRight', metaKey: true })).toBe(1);
    expect(modeCycleDirection({ ...base, key: 'ArrowLeft', ctrlKey: true })).toBe(-1);
    expect(modeCycleDirection({ ...base, key: 'ArrowRight' })).toBe(0);
  });
});
