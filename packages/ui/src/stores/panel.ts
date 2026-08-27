import { defineStore } from 'pinia';
import { ref } from 'vue';

/** 面板四个模式 Tab（§8.3）；设置已拆成独立窗口。 */
export type PanelMode = 'translate' | 'grammar' | 'mistakes' | 'history';

/** 全局热键直达的目的（统一面板 = 保留上次模式）。 */
export type HotkeyKind = 'translate' | 'grammar' | 'panel';

export const MODE_ORDER: readonly PanelMode[] = [
  'translate',
  'grammar',
  'mistakes',
  'history',
];

export const usePanelStore = defineStore('panel', () => {
  const activeMode = ref<PanelMode>('translate');
  const visible = ref(false);
  const pinned = ref(false);
  const input = ref('');
  const lastHotkey = ref<HotkeyKind>('panel');

  function setActiveMode(m: PanelMode) {
    activeMode.value = m;
  }

  /** 全局热键：直达模式并唤起面板；统一面板保留上次模式（§7.3）。 */
  function onHotkey(kind: HotkeyKind) {
    lastHotkey.value = kind;
    if (kind === 'translate' || kind === 'grammar') {
      activeMode.value = kind;
    }
    visible.value = true;
  }

  /** panel://visibility 事件：shown / hidden。 */
  function onVisibility(showing: boolean) {
    visible.value = showing;
  }

  /** 图钉固定：失焦不再自动隐藏。 */
  function setPinned(value: boolean) {
    pinned.value = value;
  }

  /** Tab 切模式：循环向。 */
  function cycleMode(direction: 1 | -1) {
    const i = MODE_ORDER.indexOf(activeMode.value);
    const n = MODE_ORDER.length;
    activeMode.value = MODE_ORDER[(i + direction + n) % n];
  }

  function getTabOrder(): readonly PanelMode[] {
    return MODE_ORDER;
  }

  return {
    activeMode,
    visible,
    pinned,
    input,
    lastHotkey,
    setActiveMode,
    onHotkey,
    onVisibility,
    setPinned,
    cycleMode,
    getTabOrder,
  };
});