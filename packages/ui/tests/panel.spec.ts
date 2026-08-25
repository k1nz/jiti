import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePanelStore } from '../src/stores/panel';

describe('panel store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('翻译热键直达翻译模式并唤起面板', () => {
    const store = usePanelStore();
    store.setActiveMode('settings');
    store.onHotkey('translate');
    expect(store.activeMode).toBe('translate');
    expect(store.visible).toBe(true);
    expect(store.lastHotkey).toBe('translate');
  });

  it('语法热键直达语法模式', () => {
    const store = usePanelStore();
    store.onHotkey('grammar');
    expect(store.activeMode).toBe('grammar');
  });

  it('统一面板热键保留上次模式（§7.3）', () => {
    const store = usePanelStore();
    store.setActiveMode('grammar');
    store.onHotkey('panel');
    expect(store.activeMode).toBe('grammar');
    expect(store.visible).toBe(true);
  });

  it('Tab 循环切模式并回绕', () => {
    const store = usePanelStore();
    expect(store.activeMode).toBe('translate');
    store.cycleMode(1);
    expect(store.activeMode).toBe('grammar');
    store.cycleMode(1);
    store.cycleMode(1);
    store.cycleMode(1);
    expect(store.activeMode).toBe('settings');
    store.cycleMode(1);
    expect(store.activeMode).toBe('translate');
    store.cycleMode(-1);
    expect(store.activeMode).toBe('settings');
  });

  it('visibility 事件同步显隐状态', () => {
    const store = usePanelStore();
    store.onVisibility(false);
    expect(store.visible).toBe(false);
    store.onVisibility(true);
    expect(store.visible).toBe(true);
  });
});