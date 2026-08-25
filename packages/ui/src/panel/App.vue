<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, type Component } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  IconAbc,
  IconHistory,
  IconLanguage,
  IconNotebook,
  IconSettings,
} from '@tabler/icons-vue';
import { commands } from '../ipc/bindings';
import { usePanelStore, type HotkeyKind, type PanelMode } from '../stores/panel';

const store = usePanelStore();
const searchEl = ref<HTMLInputElement | null>(null);

const TABS: ReadonlyArray<{ key: PanelMode; label: string; icon: Component }> = [
  { key: 'translate', label: '翻译', icon: IconLanguage },
  { key: 'grammar', label: '语法', icon: IconAbc },
  { key: 'mistakes', label: '错题本', icon: IconNotebook },
  { key: 'history', label: '历史', icon: IconHistory },
  { key: 'settings', label: '设置', icon: IconSettings },
];

const PLACEHOLDERS: Record<PanelMode, { icon: Component; line: string }> = {
  translate: { icon: IconLanguage, line: '翻译 · M1 接入（快捷键唤起时读取选中文本）' },
  grammar: { icon: IconAbc, line: '语法检查 · M2 接入' },
  mistakes: { icon: IconNotebook, line: '还没有错题，检查一次就有了' },
  history: { icon: IconHistory, line: '暂无历史' },
  settings: { icon: IconSettings, line: '设置独立窗口 · M4 完善' },
};

let unlistenHotkey: UnlistenFn | undefined;
let unlistenVisibility: UnlistenFn | undefined;

async function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    await commands.hidePopup();
    return;
  }
  if (e.key === 'Tab') {
    e.preventDefault();
    store.cycleMode(e.shiftKey ? -1 : 1);
    searchEl.value?.focus();
  }
}

function onInputFocus(e: FocusEvent) {
  // §8.3：聚焦即全选，可直接替换
  (e.target as HTMLInputElement).select();
}

function onShellClick(e: MouseEvent) {
  // 点面板空白处聚焦输入框（面板默认不夺焦，用户点击后进入可输入态）
  const target = e.target as HTMLElement;
  if (!target.closest('button, input, a')) {
    searchEl.value?.focus();
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown);
  unlistenHotkey = await listen<string>('hotkey://pressed', (e) => {
    store.onHotkey(e.payload as HotkeyKind);
  });
  unlistenVisibility = await listen<string>('panel://visibility', (e) => {
    store.onVisibility(e.payload === 'shown');
  });
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  unlistenHotkey?.();
  unlistenVisibility?.();
});
</script>

<template>
  <div class="shell" @click="onShellClick">
    <header class="chrome">
      <input
        ref="searchEl"
        v-model="store.input"
        class="search"
        type="text"
        placeholder="输入文本，或选中一段文字后按 ⌥⌘T / ⌥⌘G …"
        spellcheck="false"
        autocomplete="off"
        @focus="onInputFocus"
      />
    </header>

    <nav class="tabs chrome" role="tablist" aria-label="模式">
      <button
        v-for="tab in TABS"
        :key="tab.key"
        class="tab"
        role="tab"
        type="button"
        :aria-selected="store.activeMode === tab.key"
        :tabindex="store.activeMode === tab.key ? 0 : -1"
        :class="{ active: store.activeMode === tab.key }"
        @click="store.setActiveMode(tab.key)"
        @keydown.left.prevent="store.cycleMode(-1)"
        @keydown.right.prevent="store.cycleMode(1)"
      >
        <component :is="tab.icon" :size="16" :stroke-width="1.75" />
        <span>{{ tab.label }}</span>
      </button>
    </nav>

    <main class="content">
      <div class="placeholder" :key="store.activeMode">
        <component :is="PLACEHOLDERS[store.activeMode].icon" :size="28" :stroke-width="1.5" />
        <p>{{ PLACEHOLDERS[store.activeMode].line }}</p>
      </div>
    </main>

    <footer class="status chrome">
      <span class="status-item">
        <span class="dot" aria-hidden="true"></span>
        M0 骨架 · 引擎未配置
      </span>
      <span class="spacer"></span>
      <span class="status-item mono">Esc 隐藏 · Tab 切模式</span>
    </footer>
  </div>
</template>