<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue';
import { useI18n } from 'vue-i18n';
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
  IconSearch,
  IconSettings,
  IconTrash,
} from '@tabler/icons-vue';
import { currentPreferences } from '../bootstrap';
import { commands, events } from '../ipc/bindings';
import type {
  EngineErrorPayload,
  HistoryEntry,
  PermissionsSnapshot,
  ProvidersSnapshot,
  TranslateRequest_Deserialize,
  HotkeysSnapshot,
  TranslateResult,
  SelectedText,
  GrammarProgressEvent_Deserialize,
  GrammarRequest_Deserialize,
  TranslateEnrichedEvent,
} from '../ipc/bindings';
import { Channel } from '@tauri-apps/api/core';
import {
  isStalePreferredCapture,
  shouldApplyCapture,
  shouldApplyDelayedCapture,
  shouldAutoSubmitOnCapture,
} from '../capture';
import { isSelectableTextTarget, shouldFocusSearchOnShellClick } from '../focus';
import { formatTime } from '../format';
import { shouldHandlePanelShortcut } from '../ime';
import { unwrap } from '../ipc/unwrap';
import {
  guessTarget,
  languagePairLabel,
  resolveSource,
  type LangCode,
  type SourceChoice,
} from '../language';
import { isRecordingHotkey, shouldHidePanelOnEscape } from '../hotkeys';
import { hasOpenOverlay } from '../overlays';
import { isHidePanelShortcut, modeCycleDirection } from '../panelKeys';
import { TypeaheadBuffer, typeaheadIndex } from '../typeahead';
import { isCancelledEngineError } from '../loading';
import { footerPermissionWarning } from '../permissions';
import { usePanelStore, type HotkeyKind, type PanelMode } from '../stores/panel';
import { useGrammarStore } from '../stores/grammar';
import Onboarding from './Onboarding.vue';
import GrammarView from './components/GrammarView.vue';
import MistakesView from './components/MistakesView.vue';
import TranslateEnrichment from './components/TranslateEnrichment.vue';

const { t, locale } = useI18n();
const store = usePanelStore();
const grammar = useGrammarStore();
const searchEl = ref<HTMLInputElement | null>(null);

function tx(key: string, values?: Record<string, unknown>) {
  return values ? String(t(key, values)) : String(t(key));
}

const TABS: ReadonlyArray<{ key: PanelMode; icon: Component }> = [
  { key: 'translate', icon: IconLanguage },
  { key: 'grammar', icon: IconAbc },
  { key: 'mistakes', icon: IconNotebook },
  { key: 'history', icon: IconHistory },
];

const source = ref<SourceChoice>('auto');
const target = ref<LangCode>('zh');
const translateStatus = ref<'idle' | 'loading' | 'done' | 'error'>('idle');
const translateSpinner = ref(false);
let translateSpinnerTimer: ReturnType<typeof setTimeout> | undefined;
const translateResult = ref<TranslateResult | null>(null);
const translateError = ref<EngineErrorPayload | null>(null);
const history = ref<HistoryEntry[]>([]);
const settings = ref<ProvidersSnapshot | null>(null);
const hotkeys = ref<HotkeysSnapshot | null>(null);
const permissions = ref<PermissionsSnapshot | null>(null);
const showOnboarding = ref(false);
const copyLabel = ref('');
const captureEpoch = ref(0);
const inputDirty = ref(false);
const composing = ref(false);
const lastCommitted = ref('');
let applyingCapture = false;
const pointerDown = { x: 0, y: 0 };
const historyTypeahead = new TypeaheadBuffer();
const historyFocus = ref(0);

let unlistenHotkey: UnlistenFn | undefined;
let unlistenVisibility: UnlistenFn | undefined;
let unlistenEngineError: UnlistenFn | undefined;
let unlistenCapture: UnlistenFn | undefined;
let unlistenEnrich: UnlistenFn | undefined;
let permissionPoll: number | undefined;

function onShellPointerDown(e: MouseEvent) {
  pointerDown.x = e.clientX;
  pointerDown.y = e.clientY;
}

function onShellClick(e: MouseEvent) {
  const target = e.target;
  if (!(target instanceof Element)) return;
  const dx = e.clientX - pointerDown.x;
  const dy = e.clientY - pointerDown.y;
  if (
    !shouldFocusSearchOnShellClick({
      interactive: Boolean(target.closest('button, input, a, select, textarea, label')),
      selectedText: window.getSelection()?.toString() ?? '',
      selectable: isSelectableTextTarget(target),
      dragDistance: Math.hypot(dx, dy),
    })
  ) {
    return;
  }
  searchEl.value?.focus();
}

function armTranslateSpinner() {
  translateSpinner.value = false;
  if (translateSpinnerTimer !== undefined) clearTimeout(translateSpinnerTimer);
  translateSpinnerTimer = setTimeout(() => {
    if (translateStatus.value === 'loading') translateSpinner.value = true;
  }, 200);
}

async function runTranslate(text = store.input) {
  const input = text.trim();
  if (!input) return;
  translateStatus.value = 'loading';
  translateError.value = null;
  translateResult.value = null;
  armTranslateSpinner();
  const request: TranslateRequest_Deserialize = {
    text: input,
    from: resolveSource(source.value, input),
    to: target.value,
  };
  try {
    const result = await unwrap(commands.translate(request));
    translateResult.value = result;
    translateStatus.value = 'done';
    translateSpinner.value = false;
    await reloadHistory();
  } catch (err) {
    translateError.value = err as EngineErrorPayload;
    translateStatus.value = 'error';
    translateSpinner.value = false;
  }
}

function applyTranslateEnrichment(payload: TranslateEnrichedEvent) {
  const current = translateResult.value;
  if (!current) return;
  if (current.input !== payload.input || current.output !== payload.output) return;
  translateResult.value = {
    ...current,
    enrichmentPending: false,
    enrichment: payload.enrichment
      ? {
          ...payload.enrichment,
          word: payload.word || payload.enrichment.word,
        }
      : null,
  };
}

function enrichmentWord(result: TranslateResult) {
  return result.enrichmentWord?.trim() || result.enrichment?.word?.trim() || result.output;
}

function cardLoading(result: TranslateResult) {
  return Boolean(result.enrichmentPending) && !result.enrichment;
}

async function runGrammar(text = store.input) {
  const input = text.trim();
  if (!input) return;
  const id = grammar.begin();
  const onProgress = new Channel<GrammarProgressEvent_Deserialize>();
  onProgress.onmessage = (event) => grammar.applyProgress(id, event);
  const request: GrammarRequest_Deserialize = {
    text: input,
    engine: 'llm',
    requestId: String(id),
  };
  try {
    const outcome = await unwrap(commands.grammarCheck(request, onProgress));
    grammar.finish(id, outcome);
    await reloadHistory();
  } catch (err) {
    if (isCancelledEngineError(err)) return;
    grammar.fail(id, err as EngineErrorPayload);
  }
}

function onSearchEnter() {
  if (store.activeMode === 'translate') void runTranslate();
  if (store.activeMode === 'grammar') void runGrammar();
}

function onSearchKeydown(e: KeyboardEvent) {
  if (e.key !== 'Enter') return;
  if (!shouldHandlePanelShortcut(e)) return;
  e.preventDefault();
  onSearchEnter();
}

function applyCapturedText(selected: SelectedText, epoch: number) {
  if (composing.value) return;
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
    source.value = 'auto';
    target.value = guessTarget(selected.text);
    void runTranslate(selected.text);
    return;
  }
  void runGrammar(selected.text);
}

function applyDelayedCapture(selected: SelectedText, epoch: number) {
  if (composing.value) return;
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
  copyLabel.value = t('translate.copied');
  window.setTimeout(() => {
    if (copyLabel.value === t('translate.copied')) copyLabel.value = '';
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

async function loadSettings() {
  try {
    settings.value = await unwrap(commands.providersSnapshot());
  } catch {
    settings.value = null;
  }
  try {
    hotkeys.value = await commands.hotkeysSnapshot();
  } catch {
    // 热键快照失败时占位符走平台默认。
  }
  await refreshPermissions();
}

async function refreshPermissions() {
  try {
    const snapshot = await commands.permissionsSnapshot();
    permissions.value = snapshot;
    if (snapshot.needsOnboarding) showOnboarding.value = true;
  } catch {
    // 权限查询失败时不打断主流程。
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
  if (store.visible && footerPermissionWarning(permissions.value, tx)) startPermissionPoll();
}

async function restartApp() {
  await commands.restartApp();
}

async function openSettings() {
  await unwrap(commands.openSettings());
}

function historyKindLabel(kind: string) {
  if (kind === 'grammar') return t('history.kindGrammar');
  if (kind === 'ai_review') return t('history.kindReview');
  return t('history.kindTranslate');
}

const statusText = computed(() => {
  const snapshot = settings.value;
  if (!snapshot) return t('translate.engineUnconfigured');
  const active = snapshot.providers.find((provider) => provider.id === snapshot.defaultTranslate);
  if (!active) return t('translate.engineUnset');
  return t('translate.status', {
    label: active.label,
    key: active.hasKey ? t('translate.keyOn') : t('translate.keyOff'),
  });
});

const permissionWarning = computed(() => footerPermissionWarning(permissions.value, tx));

const translateShortcut = computed(() => {
  const bind = hotkeys.value?.bindings.find((item) => item.id === 'translate');
  if (bind?.display) return bind.display;
  return permissions.value?.platform === 'windows' ? 'Ctrl+Shift+T' : '⌥⌘T';
});

const translateHint = computed(() => t('translate.hint', { shortcut: translateShortcut.value }));

const grammarShortcut = computed(() => {
  const bind = hotkeys.value?.bindings.find((item) => item.id === 'grammar');
  if (bind?.display) return bind.display;
  return permissions.value?.platform === 'windows' ? 'Ctrl+Alt+G' : '⌥⌘G';
});

const grammarHint = computed(() => t('grammar.hint', { shortcut: grammarShortcut.value }));

const placeholder = computed(() => {
  if (store.activeMode === 'translate') return translateHint.value;
  if (store.activeMode === 'grammar') return grammarHint.value;
  return t(`tabs.${store.activeMode}`);
});

function onHistoryKeydown(e: KeyboardEvent) {
  if (e.key.length !== 1 || e.metaKey || e.ctrlKey || e.altKey) return;
  const labels = history.value.map((entry) => entry.input);
  const q = historyTypeahead.push(e.key);
  historyFocus.value = typeaheadIndex(labels, q, historyFocus.value);
  const row = document.querySelector(`[data-history-index="${historyFocus.value}"]`);
  if (row instanceof HTMLElement) row.focus();
}

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
  if (!shouldHandlePanelShortcut(e)) return;
  if (e.key === 'Escape') {
    if (!shouldHidePanelOnEscape(hasOpenOverlay())) return;
    e.preventDefault();
    await commands.hidePopup();
    return;
  }
  if (isHidePanelShortcut(e)) {
    e.preventDefault();
    await commands.hidePopup();
    return;
  }
  if (showOnboarding.value) return;
  if ((e.metaKey || e.ctrlKey) && e.key === ',') {
    e.preventDefault();
    await openSettings();
    return;
  }
  const cycle = modeCycleDirection(e);
  if (cycle !== 0) {
    e.preventDefault();
    store.cycleMode(cycle);
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
  if (composing.value) return;
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
  unlistenEnrich = await events.translateEnriched.listen((event) => {
    applyTranslateEnrichment(event.payload);
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
  unlistenEnrich?.();
  stopPermissionPoll();
});

watch(
  () => store.activeMode,
  (mode) => {
    if (mode === 'history') void reloadHistory();
  },
);

watch(
  () => store.visible,
  (visible) => {
    if (visible) void refreshPermissions();
    if (visible && (showOnboarding.value || permissionWarning.value)) startPermissionPoll();
    if (!visible && !showOnboarding.value) stopPermissionPoll();
    if (visible && currentPreferences().focusOnInvoke && !showOnboarding.value) {
      void nextTick(() => searchEl.value?.focus());
    }
  },
);
</script>

<template>
  <div class="shell" data-tauri-drag-region="deep" @mousedown="onShellPointerDown" @click="onShellClick">
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
          :aria-label="store.pinned ? t('pin.on') : t('pin.off')"
          :title="store.pinned ? t('pin.titleOn') : t('pin.titleOff')"
          @click.stop="togglePin"
        >
          <IconPinned v-if="store.pinned" :size="13" :stroke-width="2" />
          <IconPin v-else :size="13" :stroke-width="2" />
        </button>
      </template>
    </Onboarding>
    <template v-else>
        <header class="search-wrap">
      <IconSearch class="search-icon" :size="18" :stroke-width="1.75" aria-hidden="true" />
      <input
        ref="searchEl"
        v-model="store.input"
        class="search"
        type="text"
        :placeholder="placeholder"
        :aria-label="placeholder"
        spellcheck="false"
        autocomplete="off"
        @focus="onInputFocus"
        @input="onSearchInput"
        @compositionstart="composing = true"
        @compositionend="composing = false"
        @keydown="onSearchKeydown"
      />
    </header>

    <nav class="tabs" role="tablist" :aria-label="t('nav.modes')">
      <div class="tabs-track">
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
        <span>{{ t(`tabs.${tab.key}`) }}</span>
      </button>
      </div>
    </nav>

    <main class="content">
      <section v-if="store.activeMode === 'translate'" class="translate-view">
        <div class="toolbar">
          <div class="lang-pair">
            <select v-model="source" class="native-select" :aria-label="t('lang.auto')">
              <option value="auto">{{ t('lang.auto') }}</option>
              <option value="zh">{{ t('lang.zh') }}</option>
              <option value="en">{{ t('lang.en') }}</option>
            </select>
            <span class="lang-arrow" aria-hidden="true">→</span>
            <select v-model="target" class="native-select" :aria-label="t('lang.zh')">
              <option value="zh">{{ t('lang.zh') }}</option>
              <option value="en">{{ t('lang.en') }}</option>
            </select>
          </div>
          <button
            class="action"
            type="button"
            :disabled="translateStatus === 'loading' || !store.input.trim()"
            @click="runTranslate()"
          >
            <IconPlayerPlay :size="14" :stroke-width="1.75" />
            {{ t('translate.action') }}
          </button>
          <span class="spacer"></span>
          <button
            v-if="translateResult"
            class="icon-btn"
            type="button"
            :aria-label="t('translate.copy')"
            :title="t('translate.copy')"
            @click="copyResult"
          >
            <IconCopy :size="15" :stroke-width="1.75" />
          </button>
          <span v-if="copyLabel" class="copy-label">{{ copyLabel }}</span>
        </div>

        <div v-if="translateSpinner && translateStatus === 'loading'" class="state-box" aria-live="polite">
          <span class="spinner" aria-hidden="true"></span>
          <span>{{ t('translate.loading') }}</span>
        </div>
        <div v-else-if="translateStatus === 'error' && translateError" class="error-box" data-tauri-drag-region="false" aria-live="assertive">
          <div class="error-title">{{ translateError.code }} · {{ translateError.message }}</div>
          <div v-if="translateError.hint" class="muted">{{ translateError.hint }}</div>
          <code class="copyable">{{ translateError.copyable }}</code>
        </div>
        <div v-else-if="translateResult" class="result-box" data-tauri-drag-region="false">
          <p class="output">{{ translateResult.output }}</p>
          <TranslateEnrichment
            v-if="translateResult.enrichmentPending || translateResult.enrichment"
            :word="enrichmentWord(translateResult)"
            :enrichment="translateResult.enrichment"
            :loading="cardLoading(translateResult)"
          />
          <div class="meta">
            <span>{{ translateResult.engine }}</span>
            <span v-if="translateResult.detectedFrom || translateResult.target">{{ languagePairLabel(translateResult.detectedFrom, translateResult.target, t) }}</span>
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

      <MistakesView v-else-if="store.activeMode === 'mistakes'" />

      <section v-else-if="store.activeMode === 'history'" class="history-view">
        <div class="pane-header">
          <span>{{ t('history.title') }}</span>
          <button
            class="action subtle"
            type="button"
            :disabled="history.length === 0"
            @click="clearHistory"
          >
            <IconTrash :size="14" :stroke-width="1.75" />
            {{ t('history.clear') }}
          </button>
        </div>
        <div v-if="history.length === 0" class="state-box">
          <IconHistory :size="26" :stroke-width="1.5" />
          <span>{{ t('history.empty') }}</span>
        </div>
        <ul v-else class="history-list" tabindex="0" @keydown="onHistoryKeydown">
          <li
            v-for="(entry, index) in history"
            :key="entry.id"
            class="history-row"
            data-tauri-drag-region="false"
            :data-history-index="index"
            tabindex="-1"
          >
            <div class="history-input">{{ entry.input }}</div>
            <div class="history-output">{{ entry.output }}</div>
            <div class="meta">
              <span>{{ historyKindLabel(entry.kind) }}</span>
              <span>{{ entry.engine }}</span>
              <span>{{ entry.durationMs }} ms</span>
              <span>{{ formatTime(entry.createdAt, locale) }}</span>
            </div>
          </li>
        </ul>
      </section>
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
        @click="openSettings"
      >
        {{ permissionWarning }}
      </button>
      <span class="spacer"></span>
      <span class="status-keys">
        <kbd>Esc</kbd>
        <span>{{ t('footer.hide') }}</span>
        <kbd>{{ t('footer.modeChord') }}</kbd>
        <span>{{ t('footer.switchMode') }}</span>
      </span>
      <button
        class="icon-btn"
        type="button"
        :aria-label="t('settings.open')"
        :title="t('settings.open')"
        @click.stop="openSettings"
      >
        <IconSettings :size="13" :stroke-width="2" />
      </button>
      <button
        class="pin-btn"
        type="button"
        :class="{ active: store.pinned }"
        :aria-pressed="store.pinned"
        :aria-label="store.pinned ? t('pin.on') : t('pin.off')"
        :title="store.pinned ? t('pin.titleOn') : t('pin.titleOff')"
        @click.stop="togglePin"
      >
        <IconPinned v-if="store.pinned" :size="13" :stroke-width="2" />
        <IconPin v-else :size="13" :stroke-width="2" />
      </button>
    </footer>
    </template>
  </div>
</template>
