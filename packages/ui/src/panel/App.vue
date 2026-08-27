<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  IconAbc,
  IconCopy,
  IconHistory,
  IconLanguage,
  IconNotebook,
  IconPin,
  IconPinned,
  IconPlayerPlay,
  IconSettings,
  IconTrash,
} from '@tabler/icons-vue';
import { commands, events } from '../ipc/bindings';
import type {
  EngineErrorPayload,
  HistoryEntry,
  PermissionItem,
  PermissionsSnapshot,
  ProviderConfig_Serialize,
  ProviderView,
  ProvidersConfig_Serialize,
  ProvidersSnapshot,
  TestProviderResult,
  TranslateRequest_Deserialize,
  HotkeysSnapshot,
  TranslateResult,
  SelectedText,
  GrammarProgressEvent_Deserialize,
  GrammarRequest_Deserialize,
} from '../ipc/bindings';
import { Channel } from '@tauri-apps/api/core';
import {
  isStalePreferredCapture,
  shouldApplyCapture,
  shouldApplyDelayedCapture,
  shouldAutoSubmitOnCapture,
} from '../capture';
import { isRecordingHotkey, shouldHidePanelOnEscape } from '../hotkeys';
import { footerPermissionWarning } from '../permissions';
import { usePanelStore, type HotkeyKind, type PanelMode } from '../stores/panel';
import { useGrammarStore } from '../stores/grammar';
import Hotkeys from './Hotkeys.vue';
import Onboarding from './Onboarding.vue';
import GrammarView from './components/GrammarView.vue';

const store = usePanelStore();
const grammar = useGrammarStore();
const searchEl = ref<HTMLInputElement | null>(null);

const TABS: ReadonlyArray<{ key: PanelMode; label: string; icon: Component }> = [
  { key: 'translate', label: '翻译', icon: IconLanguage },
  { key: 'grammar', label: '语法', icon: IconAbc },
  { key: 'mistakes', label: '错题本', icon: IconNotebook },
  { key: 'history', label: '历史', icon: IconHistory },
  { key: 'settings', label: '设置', icon: IconSettings },
];

const PLACEHOLDERS: Record<PanelMode, { icon: Component; line: string }> = {
  translate: { icon: IconLanguage, line: '输入文本，或选中一段文字后按快捷键' },
  grammar: { icon: IconAbc, line: '输入英语文本，或选中一段文字后按快捷键' },
  mistakes: { icon: IconNotebook, line: '还没有错题，检查一次就有了' },
  history: { icon: IconHistory, line: '暂无历史' },
  settings: { icon: IconSettings, line: '设置加载中' },
};

const target = ref('zh');
const translateStatus = ref<'idle' | 'loading' | 'done' | 'error'>('idle');
const translateResult = ref<TranslateResult | null>(null);
const translateError = ref<EngineErrorPayload | null>(null);
const history = ref<HistoryEntry[]>([]);
const settings = ref<ProvidersSnapshot | null>(null);
const hotkeys = ref<HotkeysSnapshot | null>(null);
const permissions = ref<PermissionsSnapshot | null>(null);
const showOnboarding = ref(false);
const keyInputs = ref<Record<string, string>>({});
const testResults = ref<Record<string, TestProviderResult | null>>({});
const testing = ref<Record<string, boolean>>({});
const copyLabel = ref('');
const captureEpoch = ref(0);
const inputDirty = ref(false);
const lastCommitted = ref('');
let applyingCapture = false;

let unlistenHotkey: UnlistenFn | undefined;
let unlistenVisibility: UnlistenFn | undefined;
let unlistenEngineError: UnlistenFn | undefined;
let unlistenCapture: UnlistenFn | undefined;
let permissionPoll: number | undefined;

function unwrap<T>(promise: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: unknown }>) {
  return promise.then((result) => {
    if (result.status === 'ok') return result.data;
    throw result.error;
  });
}

function onShellClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest('button, input, a, select, textarea')) {
    searchEl.value?.focus();
  }
}

function hasCjk(text: string) {
  return /[\u3400-\u4dbf\u4e00-\u9fff]/.test(text);
}

function guessTarget(text: string) {
  return hasCjk(text) ? 'en' : 'zh';
}

async function runTranslate(text = store.input) {
  const input = text.trim();
  if (!input) return;
  translateStatus.value = 'loading';
  translateError.value = null;
  translateResult.value = null;
  const request: TranslateRequest_Deserialize = {
    text: input,
    from: hasCjk(input) ? 'zh' : null,
    to: target.value,
  };
  try {
    const result = await unwrap(commands.translate(request));
    translateResult.value = result;
    translateStatus.value = 'done';
    await reloadHistory();
  } catch (err) {
    translateError.value = err as EngineErrorPayload;
    translateStatus.value = 'error';
  }
}

async function runGrammar(text = store.input) {
  const input = text.trim();
  if (!input) return;
  const id = grammar.begin();
  const onProgress = new Channel<GrammarProgressEvent_Deserialize>();
  onProgress.onmessage = (event) => grammar.applyProgress(id, event);
  const request: GrammarRequest_Deserialize = { text: input, engine: 'llm' };
  try {
    const result = await unwrap(commands.grammarCheck(request, onProgress));
    grammar.finish(id, result);
    await reloadHistory();
  } catch (err) {
    grammar.fail(id, err as EngineErrorPayload);
  }
}

function onSearchEnter() {
  if (store.activeMode === 'translate') void runTranslate();
  if (store.activeMode === 'grammar') void runGrammar();
}

function applyCapturedText(selected: SelectedText, epoch: number) {
  if (!shouldApplyCapture(epoch, captureEpoch.value, inputDirty.value)) return;
  if (isStalePreferredCapture(selected.text, selected.method, lastCommitted.value)) return;
  applyingCapture = true;
  store.input = selected.text;
  void nextTick(() => {
    applyingCapture = false;
  });
  // 捕获后绝不抢焦点：否则下次热键会读到自己输入框里的旧选区。
  if (!selected.text.trim()) return;
  lastCommitted.value = selected.text;
  if (!shouldAutoSubmitOnCapture(store.activeMode, selected.text)) return;
  if (store.activeMode === 'translate') {
    target.value = guessTarget(selected.text);
    void runTranslate(selected.text);
    return;
  }
  void runGrammar(selected.text);
}

function applyDelayedCapture(selected: SelectedText, epoch: number) {
  if (
    !shouldApplyDelayedCapture(
      epoch,
      captureEpoch.value,
      inputDirty.value,
      store.input,
    )
  ) {
    return;
  }
  applyCapturedText(selected, epoch);
}

function onSearchInput() {
  if (applyingCapture) return;
  inputDirty.value = true;
}

async function copyResult() {
  const output =
    store.activeMode === 'grammar'
      ? grammar.correctedText
      : translateResult.value?.output;
  if (!output) return;
  try {
    await navigator.clipboard.writeText(output);
  } catch {
    const textarea = document.createElement('textarea');
    textarea.value = output;
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand('copy');
    textarea.remove();
  }
  copyLabel.value = '已复制';
  window.setTimeout(() => {
    if (copyLabel.value === '已复制') copyLabel.value = '';
  }, 1200);
}

async function reloadHistory() {
  try {
    history.value = await unwrap(commands.historyList());
  } catch {
    history.value = [];
  }
}

async function clearHistory() {
  await unwrap(commands.historyClear());
  await reloadHistory();
}

function providerView(id: string) {
  return settings.value?.providers.find((provider) => provider.id === id) ?? null;
}

function viewToConfig(view: ProviderView): ProviderConfig_Serialize {
  return {
    enabled: view.enabled,
    baseUrl: view.baseUrl,
    model: view.model,
    kind: view.kind,
    temperature: view.temperature,
    maxTokens: view.maxTokens,
    formality: view.formality,
  };
}

function buildConfig(): ProvidersConfig_Serialize {
  const snapshot = settings.value;
  if (!snapshot) throw new Error('设置尚未加载');
  const config = (id: string) => viewToConfig(providerView(id) ?? {
    id,
    label: id,
    enabled: false,
    hasKey: false,
    kind: null,
    baseUrl: null,
    model: null,
    temperature: null,
    maxTokens: null,
    formality: null,
    testable: id !== 'youdao',
  });
  return {
    deepl: config('deepl'),
    llm: config('llm'),
    youdao: config('youdao'),
    defaultTranslate: snapshot.defaultTranslate,
    writeHistory: snapshot.writeHistory,
  };
}

async function saveSettings() {
  try {
    settings.value = await unwrap(commands.providersSave(buildConfig()));
  } catch (err) {
    translateError.value = {
      provider: 'settings',
      code: 'invalid_config',
      message: String(err),
      hint: null,
      copyable: `[jiti] settings ${String(err)}`,
    };
  }
}

async function loadSettings() {
  try {
    settings.value = await unwrap(commands.providersSnapshot());
  } catch (err) {
    PLACEHOLDERS.settings.line = `设置加载失败：${String(err)}`;
  }
  try {
    hotkeys.value = await commands.hotkeysSnapshot();
  } catch {
    // 热键快照失败时占位符走平台默认。
  }
  await refreshPermissions();
}

function onHotkeysUpdated(snapshot: HotkeysSnapshot) {
  hotkeys.value = snapshot;
}

async function refreshPermissions() {
  try {
    const snapshot = await commands.permissionsSnapshot();
    permissions.value = snapshot;
    if (snapshot.needsOnboarding) showOnboarding.value = true;
  } catch {
    // 权限查询失败时不打断主流程；设置卡会保持上次状态。
  }
}

function startPermissionPoll() {
  if (permissionPoll !== undefined) return;
  permissionPoll = window.setInterval(() => {
    void refreshPermissions();
  }, 1200);
}

function stopPermissionPoll() {
  if (permissionPoll === undefined) return;
  window.clearInterval(permissionPoll);
  permissionPoll = undefined;
}

async function enablePermission(id: string) {
  await unwrap(commands.requestPermission(id));
  await refreshPermissions();
  startPermissionPoll();
}

async function finishOnboarding() {
  permissions.value = await unwrap(commands.completeOnboarding());
  showOnboarding.value = false;
  stopPermissionPoll();
  if (store.visible && footerPermissionWarning(permissions.value)) startPermissionPoll();
}

async function restartApp() {
  await commands.restartApp();
}

function accessibilityItem(): PermissionItem | null {
  return permissions.value?.items.find((item) => item.id === 'accessibility') ?? null;
}

function onToggleProvider(id: string, event: Event) {
  const view = providerView(id);
  if (!view) return;
  view.enabled = (event.target as HTMLInputElement).checked;
  void saveSettings();
}

function onProviderField(id: string, field: 'baseUrl' | 'model', event: Event) {
  const view = providerView(id);
  if (!view) return;
  const value = (event.target as HTMLInputElement).value;
  if (field === 'baseUrl') view.baseUrl = value;
  else view.model = value;
  void saveSettings();
}

function onDefaultChange(event: Event) {
  if (!settings.value) return;
  settings.value.defaultTranslate = (event.target as HTMLSelectElement).value;
  void saveSettings();
}

function onWriteHistory(event: Event) {
  if (!settings.value) return;
  settings.value.writeHistory = (event.target as HTMLInputElement).checked;
  void saveSettings();
}

async function saveKey(id: string) {
  const key = (keyInputs.value[id] ?? '').trim();
  await unwrap(commands.providerSaveApiKey(id, key));
  keyInputs.value[id] = '';
  await loadSettings();
}

async function testProvider(id: string) {
  testing.value[id] = true;
  try {
    testResults.value[id] = await unwrap(commands.providerTest(id));
  } catch (err) {
    testResults.value[id] = { ok: false, reason: String(err) };
  } finally {
    testing.value[id] = false;
  }
}

async function openAccessibility() {
  await unwrap(commands.openPermissionSettings('accessibility'));
  startPermissionPoll();
}

function formatTime(value: string) {
  const date = new Date(value.replace(' ', 'T') + 'Z');
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN', { hour12: false });
}

const statusText = computed(() => {
  const snapshot = settings.value;
  if (!snapshot) return 'M0 骨架 · 引擎未配置';
  const active = snapshot.providers.find((provider) => provider.id === snapshot.defaultTranslate);
  if (!active) return '引擎未设置';
  return `${active.label} · ${active.hasKey ? 'Key 已配置' : 'Key 未配置'}`;
});

const permissionWarning = computed(() => footerPermissionWarning(permissions.value));
const accessItem = computed(() => accessibilityItem());

const translateShortcut = computed(() => {
  const bind = hotkeys.value?.bindings.find((item) => item.id === 'translate');
  if (bind?.display) return bind.display;
  return permissions.value?.platform === 'windows' ? 'Ctrl+Shift+T' : '⌥⌘T';
});

const translateHint = computed(() => `输入文本，或选中一段文字后按 ${translateShortcut.value}`);

const grammarShortcut = computed(() => {
  const bind = hotkeys.value?.bindings.find((item) => item.id === 'grammar');
  if (bind?.display) return bind.display;
  return permissions.value?.platform === 'windows' ? 'Ctrl+Alt+G' : '⌥⌘G';
});

const grammarHint = computed(() => `输入英语文本，或选中一段文字后按 ${grammarShortcut.value}`);

const placeholder = computed(() => {
  if (store.activeMode === 'translate') return translateHint.value;
  if (store.activeMode === 'grammar') return grammarHint.value;
  return TABS.find((tab) => tab.key === store.activeMode)?.label ?? '输入';
});

async function togglePin() {
  const next = !store.pinned;
  try {
    store.setPinned(await commands.setPanelPinned(next));
  } catch {
    /* 保持当前固定态 */
  }
}

async function onKeydown(e: KeyboardEvent) {
  if (isRecordingHotkey.value) {
    e.preventDefault();
    return;
  }
  if (e.key === 'Escape') {
    if (!shouldHidePanelOnEscape()) return;
    e.preventDefault();
    await commands.hidePopup();
    return;
  }
  if (showOnboarding.value) return;
  if (e.key === 'Tab') {
    e.preventDefault();
    store.cycleMode(e.shiftKey ? -1 : 1);
    searchEl.value?.focus();
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'c') {
    const tag = (document.activeElement as HTMLElement | null)?.tagName;
    const hasCopy =
      (store.activeMode === 'grammar' && grammar.correctedText) ||
      (store.activeMode !== 'grammar' && translateResult.value);
    if (tag !== 'INPUT' && tag !== 'TEXTAREA' && hasCopy) {
      e.preventDefault();
      await copyResult();
    }
  }
}

function onInputFocus(e: FocusEvent) {
  (e.target as HTMLInputElement).select();
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown);
  unlistenHotkey = await events.hotkeyPressed.listen((event) => {
    const kind = event.payload.mode as HotkeyKind;
    store.onHotkey(kind);
    if (kind !== 'translate' && kind !== 'grammar') return;
    captureEpoch.value = event.payload.epoch;
    inputDirty.value = false;
    applyingCapture = true;
    store.input = '';
    void nextTick(() => {
      applyingCapture = false;
    });
    if (kind === 'translate') {
      translateStatus.value = 'idle';
      translateResult.value = null;
      translateError.value = null;
    }
    if (kind === 'grammar') grammar.reset();
    applyCapturedText(event.payload.selection, event.payload.epoch);
  });
  unlistenCapture = await events.captureChanged.listen((event) => {
    if (store.activeMode !== 'translate' && store.activeMode !== 'grammar') return;
    applyDelayedCapture(event.payload.selection, event.payload.epoch);
  });
  unlistenVisibility = await listen<string>('panel://visibility', (event) => {
    store.onVisibility(event.payload === 'shown');
  });
  unlistenEngineError = await events.engineError.listen((event) => {
    translateError.value = event.payload;
    if (store.activeMode === 'translate') translateStatus.value = 'error';
  });
  await loadSettings();
  try {
    store.setPinned(await commands.panelPinned());
  } catch {
    store.setPinned(false);
  }
  if (permissions.value?.needsOnboarding) {
    showOnboarding.value = true;
    startPermissionPoll();
    await unwrap(commands.showPopup(null));
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  unlistenHotkey?.();
  unlistenVisibility?.();
  unlistenEngineError?.();
  unlistenCapture?.();
  stopPermissionPoll();
});

watch(
  () => store.activeMode,
  (mode) => {
    if (mode === 'history') void reloadHistory();
    if (mode === 'settings') void loadSettings();
  },
);

watch(
  () => store.visible,
  (visible) => {
    if (visible) void refreshPermissions();
    if (visible && (showOnboarding.value || permissionWarning.value)) startPermissionPoll();
    if (!visible && !showOnboarding.value) stopPermissionPoll();
  },
);
</script>

<template>
  <div class="shell" data-tauri-drag-region="deep" @click="onShellClick">
    <Onboarding
      v-if="showOnboarding && permissions"
      :snapshot="permissions"
      @enable="enablePermission"
      @skip="finishOnboarding"
      @start="finishOnboarding"
      @restart="restartApp"
      @recheck="refreshPermissions"
    >
      <template #pin>
        <button
          class="pin-btn"
          type="button"
          :class="{ active: store.pinned }"
          :aria-pressed="store.pinned"
          :aria-label="store.pinned ? '取消固定窗口' : '固定窗口'"
          :title="store.pinned ? '取消固定' : '固定窗口，失去焦点时保持打开'"
          @click.stop="togglePin"
        >
          <IconPinned v-if="store.pinned" :size="13" :stroke-width="2" />
          <IconPin v-else :size="13" :stroke-width="2" />
        </button>
      </template>
    </Onboarding>
    <template v-else>
        <header class="search-wrap">
      <input
        ref="searchEl"
        v-model="store.input"
        class="search"
        type="text"
        :placeholder="placeholder"
        spellcheck="false"
        autocomplete="off"
        @focus="onInputFocus"
        @input="onSearchInput"
        @keydown.enter.prevent="onSearchEnter"
      />
    </header>

    <nav class="tabs" role="tablist" aria-label="模式">
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
      <section v-if="store.activeMode === 'translate'" class="translate-view">
        <div class="toolbar">
          <select v-model="target" class="native-select" aria-label="目标语言">
            <option value="zh">中文</option>
            <option value="en">English</option>
          </select>
          <button
            class="action"
            type="button"
            :disabled="translateStatus === 'loading' || !store.input.trim()"
            @click="runTranslate()"
          >
            <IconPlayerPlay :size="14" :stroke-width="1.75" />
            翻译
          </button>
          <span class="spacer"></span>
          <button
            v-if="translateResult"
            class="icon-btn"
            type="button"
            aria-label="复制结果"
            title="复制结果"
            @click="copyResult"
          >
            <IconCopy :size="15" :stroke-width="1.75" />
          </button>
          <span v-if="copyLabel" class="copy-label">{{ copyLabel }}</span>
        </div>

        <div v-if="translateStatus === 'loading'" class="state-box" aria-live="polite">
          <span class="spinner" aria-hidden="true"></span>
          <span>正在翻译</span>
        </div>
        <div v-else-if="translateStatus === 'error' && translateError" class="error-box" data-tauri-drag-region="false" aria-live="assertive">
          <div class="error-title">{{ translateError.code }} · {{ translateError.message }}</div>
          <div v-if="translateError.hint" class="muted">{{ translateError.hint }}</div>
          <code class="copyable">{{ translateError.copyable }}</code>
        </div>
        <div v-else-if="translateResult" class="result-box" data-tauri-drag-region="false">
          <p class="output">{{ translateResult.output }}</p>
          <div class="meta">
            <span>{{ translateResult.engine }}</span>
            <span v-if="translateResult.detectedFrom">{{ translateResult.detectedFrom }} → {{ translateResult.target }}</span>
            <span>{{ translateResult.durationMs }} ms</span>
          </div>
        </div>
        <div v-else class="state-box">
          <IconLanguage :size="26" :stroke-width="1.5" />
          <span>{{ translateHint }}</span>
        </div>
      </section>

      <GrammarView
        v-else-if="store.activeMode === 'grammar'"
        :input="store.input"
        :hint="grammarHint"
        :copy-label="copyLabel"
        @check="runGrammar()"
        @copy="copyResult"
      />

      <section v-else-if="store.activeMode === 'history'" class="history-view">
        <div class="pane-header">
          <span>历史记录</span>
          <button
            class="action subtle"
            type="button"
            :disabled="history.length === 0"
            @click="clearHistory"
          >
            <IconTrash :size="14" :stroke-width="1.75" />
            清空
          </button>
        </div>
        <div v-if="history.length === 0" class="state-box">
          <IconHistory :size="26" :stroke-width="1.5" />
          <span>暂无历史</span>
        </div>
        <ul v-else class="history-list">
          <li v-for="entry in history" :key="entry.id" class="history-row" data-tauri-drag-region="false">
            <div class="history-input">{{ entry.input }}</div>
            <div class="history-output">{{ entry.output }}</div>
            <div class="meta">
              <span>{{ entry.kind === 'grammar' ? '语法' : '翻译' }}</span>
              <span>{{ entry.engine }}</span>
              <span>{{ entry.durationMs }} ms</span>
              <span>{{ formatTime(entry.createdAt) }}</span>
            </div>
          </li>
        </ul>
      </section>

      <section v-else-if="store.activeMode === 'settings'" class="settings-view">
        <div
          v-if="accessItem && !accessItem.granted"
          class="permission-card"
        >
          <span>辅助功能权限未开启</span>
          <p class="muted">{{ accessItem.hint }}</p>
          <button class="action" type="button" @click="openAccessibility">打开系统设置</button>
        </div>

        <p v-else-if="permissions?.platform === 'windows'" class="muted settings-note">
          Windows 通过 UI Automation 读取选中文本，无需额外系统授权。
        </p>

        <Hotkeys
          v-if="hotkeys"
          :snapshot="hotkeys"
          @updated="onHotkeysUpdated"
        />

        <div class="pane-header">
          <span>引擎与 Key</span>
          <button class="action subtle" type="button" @click="loadSettings">刷新</button>
        </div>
        <p class="muted settings-note">API Key 保存在本机 keys.json，不写入钥匙串。</p>

        <div v-if="settings" class="setting-row">
          <label class="field-label" for="default-engine">默认引擎</label>
          <select
            id="default-engine"
            class="native-select"
            :value="settings.defaultTranslate"
            @change="onDefaultChange"
          >
            <option v-for="provider in settings.providers" :key="provider.id" :value="provider.id">
              {{ provider.label }}
            </option>
          </select>
          <label class="check">
            <input type="checkbox" :checked="settings.writeHistory" @change="onWriteHistory" />
            写历史
          </label>
        </div>

        <div v-if="settings" class="provider-stack" data-tauri-drag-region="false">
          <div v-for="provider in settings.providers" :key="provider.id" class="provider-card">
            <div class="provider-head">
              <strong>{{ provider.label }}</strong>
              <label class="toggle">
                <input
                  type="checkbox"
                  :checked="provider.enabled"
                  @change="onToggleProvider(provider.id, $event)"
                />
                <span>{{ provider.enabled ? '已启用' : '未启用' }}</span>
              </label>
            </div>
            <div class="field-grid">
              <label class="field-label">Base URL</label>
              <input
                class="text-input"
                type="url"
                :value="provider.baseUrl ?? ''"
                spellcheck="false"
                @change="onProviderField(provider.id, 'baseUrl', $event)"
              />
              <template v-if="provider.id === 'llm'">
                <label class="field-label">Model</label>
                <input
                  class="text-input"
                  type="text"
                  :value="provider.model ?? ''"
                  spellcheck="false"
                  @change="onProviderField(provider.id, 'model', $event)"
                />
              </template>
            </div>
            <div class="key-row">
              <input
                class="text-input mono"
                type="password"
                v-model="keyInputs[provider.id]"
                placeholder="API Key"
                autocomplete="off"
                spellcheck="false"
              />
              <button class="action subtle" type="button" @click="saveKey(provider.id)">
                保存 Key
              </button>
            </div>
            <div class="test-row">
              <span class="badge" :class="{ on: provider.hasKey }">
                {{ provider.hasKey ? 'Key 已配置' : 'Key 未配置' }}
              </span>
              <button
                v-if="provider.testable"
                class="action subtle"
                type="button"
                :disabled="testing[provider.id]"
                @click="testProvider(provider.id)"
              >
                {{ testing[provider.id] ? '测试中' : '测试连接' }}
              </button>
              <span v-if="testResults[provider.id]" class="test-result" :class="{ fail: !testResults[provider.id]?.ok }">
                {{ testResults[provider.id]?.ok ? 'OK' : '失败' }}
              </span>
            </div>
            <p v-if="testResults[provider.id]?.reason" class="test-reason">
              {{ testResults[provider.id]?.reason }}
            </p>
          </div>
        </div>
      </section>

      <div v-else class="placeholder" :key="store.activeMode">
        <component :is="PLACEHOLDERS[store.activeMode].icon" :size="28" :stroke-width="1.5" />
        <p>{{ PLACEHOLDERS[store.activeMode].line }}</p>
      </div>
    </main>

    <footer class="status">
      <span class="status-item">
        <span class="dot" aria-hidden="true"></span>
        {{ statusText }}
      </span>
      <button
        v-if="permissionWarning"
        class="status-warn"
        type="button"
        @click="store.setActiveMode('settings')"
      >
        {{ permissionWarning }}
      </button>
      <span class="spacer"></span>
      <span class="status-item mono">Esc 隐藏 · Tab 切模式</span>
      <button
        class="pin-btn"
        type="button"
        :class="{ active: store.pinned }"
        :aria-pressed="store.pinned"
        :aria-label="store.pinned ? '取消固定窗口' : '固定窗口'"
        :title="store.pinned ? '取消固定' : '固定窗口，失去焦点时保持打开'"
        @click.stop="togglePin"
      >
        <IconPinned v-if="store.pinned" :size="13" :stroke-width="2" />
        <IconPin v-else :size="13" :stroke-width="2" />
      </button>
    </footer>
    </template>
  </div>
</template>
