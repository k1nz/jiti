import { defineStore } from 'pinia';
import { ref } from 'vue';

/** 面板五个模式 Tab（§8.3）。 */
export type PanelMode = 'translate' | 'grammar' | 'mistakes' | 'history' | 'settings';

/** 全局热键直达的目的（统一面板 = 保留上次模式）。 */
export type HotkeyKind = 'translate' | 'grammar' | 'panel';

export const MODE_ORDER: readonly PanelMode[] = [
  'translate',
  'grammar',
  'mistakes',
  'history',
  'settings',
];

export const usePanelStore = defineStore('panel', () => {
  const activeMode = ref<PanelMode>('translate');
  const visible = ref(false);
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
    input,
    lastHotkey,
    setActiveMode,
    onHotkey,
    onVisibility,
    cycleMode,
    getTabOrder,
  };
});