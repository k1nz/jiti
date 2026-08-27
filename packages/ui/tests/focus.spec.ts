import { describe, expect, it } from 'vitest';
import { shouldFocusSearchOnShellClick } from '../src/focus';

describe('shouldFocusSearchOnShellClick', () => {
  const chrome = {
    interactive: false,
    selectedText: '',
    selectable: false,
    dragDistance: 0,
  };

  it('空白 chrome 单击会聚焦搜索框', () => {
    expect(shouldFocusSearchOnShellClick(chrome)).toBe(true);
  });

  it('点在按钮/输入等控件上不抢焦点', () => {
    expect(shouldFocusSearchOnShellClick({ ...chrome, interactive: true })).toBe(false);
  });

  it('点在可选文字上不抢焦点', () => {
    expect(shouldFocusSearchOnShellClick({ ...chrome, selectable: true })).toBe(false);
  });

  it('已经选中文字时不抢焦点', () => {
    expect(shouldFocusSearchOnShellClick({ ...chrome, selectedText: 'hello' })).toBe(false);
  });

  it('拖拽选中后 mouseup 不抢焦点', () => {
    expect(shouldFocusSearchOnShellClick({ ...chrome, dragDistance: 12 })).toBe(false);
  });

  it('轻微抖动仍视为单击', () => {
    expect(shouldFocusSearchOnShellClick({ ...chrome, dragDistance: 3 })).toBe(true);
  });
});
