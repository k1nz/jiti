import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePanelStore } from '../src/stores/panel';

describe('panel store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('翻译热键直达翻译模式并唤起面板', () => {
    const store = usePanelStore();
    store.setActiveMode('history');
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
    expect(store.activeMode).toBe('mistakes');
    store.cycleMode(1);
    expect(store.activeMode).toBe('history');
    store.cycleMode(1);
    expect(store.activeMode).toBe('translate');
    store.cycleMode(-1);
    expect(store.activeMode).toBe('history');
  });

  it('visibility 事件同步显隐状态', () => {
    const store = usePanelStore();
    store.onVisibility(false);
    expect(store.visible).toBe(false);
    store.onVisibility(true);
    expect(store.visible).toBe(true);
  });

  it('图钉固定态默认为关，可切换', () => {
    const store = usePanelStore();
    expect(store.pinned).toBe(false);
    store.setPinned(true);
    expect(store.pinned).toBe(true);
    store.setPinned(false);
    expect(store.pinned).toBe(false);
  });
});