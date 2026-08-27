import { describe, expect, it } from 'vitest';
import { acceleratorFromCombo, isRecordingHotkey, shouldHidePanelOnEscape } from '../src/hotkeys';

describe('acceleratorFromCombo', () => {
  it('把 Windows 翻译默认键写成 Ctrl+Shift+T', () => {
    expect(
      acceleratorFromCombo({
        code: 'KeyT',
        ctrlKey: true,
        shiftKey: true,
        altKey: false,
        metaKey: false,
      }),
    ).toBe('shift+control+KeyT');
  });

  it('把 macOS 翻译默认键写成 ⌥⌘T', () => {
    expect(
      acceleratorFromCombo({
        code: 'KeyT',
        ctrlKey: false,
        shiftKey: false,
        altKey: true,
        metaKey: true,
      }),
    ).toBe('alt+super+KeyT');
  });

  it('单独修饰键或裸字母不能作为热键', () => {
    expect(
      acceleratorFromCombo({
        code: 'ShiftLeft',
        ctrlKey: false,
        shiftKey: true,
        altKey: false,
        metaKey: false,
      }),
    ).toBeNull();
    expect(
      acceleratorFromCombo({
        code: 'KeyT',
        ctrlKey: false,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      }),
    ).toBeNull();
  });

  it('录制中面板不应把 Esc 当成关窗', () => {
    isRecordingHotkey.value = true;
    expect(shouldHidePanelOnEscape()).toBe(false);
    isRecordingHotkey.value = false;
    expect(shouldHidePanelOnEscape()).toBe(true);
    expect(shouldHidePanelOnEscape(true)).toBe(false);
  });
});
