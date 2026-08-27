<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  IconAdjustmentsHorizontal,
  IconKeyboard,
  IconNotebook,
  IconPlugConnected,
  IconSearch,
} from '@tabler/icons-vue';
import { commands, events } from '../ipc/bindings';
import type {
  EngineErrorPayload,
  HotkeysSnapshot,
  PermissionItem,
  PermissionsSnapshot,
  PreferencesPatch,
  PreferencesSnapshot,
  ProviderConfig_Serialize,
  ProviderView,
  ProvidersConfig_Serialize,
  ProvidersSnapshot,
  TestProviderResult,
  ThemePref,
  UiLocale,
} from '../ipc/bindings';
import { unwrap } from '../ipc/unwrap';
import { useMistakesStore } from '../stores/mistakes';
import Hotkeys from './Hotkeys.vue';

type SettingsSection = 'general' | 'shortcuts' | 'engines' | 'mistakes';

const NAV: ReadonlyArray<{
  id: SettingsSection;
  icon: Component;
  labelKey: string;
  keys: string[];
}> = [
  {
    id: 'general',
    icon: IconAdjustmentsHorizontal,
    labelKey: 'settings.general',
    keys: [
      'settings.appearance',
      'settings.locale',
      'settings.theme',
      'settings.startup',
      'settings.autostart',
      'settings.permissions',
      'permissions.accessibility.title',
    ],
  },
  {
    id: 'shortcuts',
    icon: IconKeyboard,
    labelKey: 'hotkeys.title',
    keys: ['hotkeys.translate', 'hotkeys.grammar', 'hotkeys.panel', 'hotkeys.reset'],
  },
  {
    id: 'engines',
    icon: IconPlugConnected,
    labelKey: 'settings.engines',
    keys: [
      'settings.enginesDefaults',
      'settings.defaultEngine',
      'settings.writeHistory',
      'settings.baseUrl',
      'settings.model',
      'settings.apiKey',
    ],
  },
  {
    id: 'mistakes',
    icon: IconNotebook,
    labelKey: 'settings.mistakes',
    keys: ['settings.autoCollect', 'settings.defaultStatus'],
  },
];

const { t } = useI18n();
const mistakes = useMistakesStore();
const settings = ref<ProvidersSnapshot | null>(null);
const hotkeys = ref<HotkeysSnapshot | null>(null);
const permissions = ref<PermissionsSnapshot | null>(null);
const prefs = ref<PreferencesSnapshot | null>(null);
const keyInputs = ref<Record<string, string>>({});
const testResults = ref<Record<string, TestProviderResult | null>>({});
const testing = ref<Record<string, boolean>>({});
const saveError = ref<EngineErrorPayload | null>(null);
const autostartBusy = ref(false);
const searchQuery = ref('');
const activeSection = ref<SettingsSection>('general');
const searchEl = ref<HTMLInputElement | null>(null);
let permissionPoll: number | undefined;

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
  if (!snapshot) throw new Error(t('settings.notLoaded'));
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
    saveError.value = null;
  } catch (err) {
    saveError.value = {
      provider: 'settings',
      code: 'invalid_config',
      message: String(err),
      hint: null,
      copyable: `[jiti] settings ${String(err)}`,
    };
  }
}

async function loadMistakePrefs() {
  try {
    mistakes.setPreferences(await unwrap(commands.mistakesPreferences()));
  } catch {
    // 偏好失败时沿用默认。
  }
}

async function refreshPermissions() {
  try {
    permissions.value = await commands.permissionsSnapshot();
  } catch {
    // 权限查询失败时不打断设置页。
  }
}

function startPermissionPoll() {
  if (permissionPoll !== undefined) return;
  permissionPoll = window.setInterval(() => {
    void refreshPermissions();
  }, 1200);
}

async function loadAll() {
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
  try {
    prefs.value = await commands.preferencesSnapshot();
  } catch {
    prefs.value = null;
  }
  await refreshPermissions();
  await loadMistakePrefs();
}

async function patchPrefs(patch: PreferencesPatch) {
  const previous = prefs.value;
  autostartBusy.value = patch.autostart !== undefined;
  try {
    prefs.value = await unwrap(commands.preferencesUpdate(patch));
    saveError.value = null;
  } catch (err) {
    prefs.value = previous;
    saveError.value = {
      provider: 'settings',
      code: 'invalid_config',
      message: String(err),
      hint: previous?.autostart.hint ?? null,
      copyable: `[jiti] preferences ${String(err)}`,
    };
  } finally {
    autostartBusy.value = false;
  }
}

function onLocaleChange(event: Event) {
  void patchPrefs({ locale: (event.target as HTMLSelectElement).value as UiLocale });
}

function onThemeChange(event: Event) {
  void patchPrefs({ theme: (event.target as HTMLSelectElement).value as ThemePref });
}

function onAutostart(event: Event) {
  const enabled = (event.target as HTMLInputElement).checked;
  if (prefs.value) {
    prefs.value = {
      ...prefs.value,
      autostart: { ...prefs.value.autostart, enabled },
    };
  }
  void patchPrefs({ autostart: enabled });
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

async function onAutoCollect(event: Event) {
  const autoCollect = (event.target as HTMLInputElement).checked;
  try {
    mistakes.setPreferences(
      await unwrap(commands.mistakesSetPreferences({
        ...mistakes.preferences,
        autoCollect,
      })),
    );
  } catch {
    /* 保持当前偏好 */
  }
}

async function onDefaultMistakeStatus(event: Event) {
  const defaultStatus = (event.target as HTMLSelectElement).value;
  try {
    mistakes.setPreferences(
      await unwrap(commands.mistakesSetPreferences({
        ...mistakes.preferences,
        defaultStatus: defaultStatus || 'open',
      })),
    );
  } catch {
    /* 保持当前偏好 */
  }
}

async function saveKey(id: string) {
  const key = (keyInputs.value[id] ?? '').trim();
  await unwrap(commands.providerSaveApiKey(id, key));
  keyInputs.value[id] = '';
  await loadAll();
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

function onHotkeysUpdated(snapshot: HotkeysSnapshot) {
  hotkeys.value = snapshot;
}

function permissionCopy(item: PermissionItem, field: 'title' | 'description' | 'hintOff' | 'hintOn') {
  return t(`permissions.${item.id}.${field}`);
}

const accessItem = computed(() =>
  permissions.value?.items.find((item) => item.id === 'accessibility') ?? null,
);

function matchesQuery(...values: Array<string | undefined | null>) {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return true;
  return values.some((value) => (value ?? '').toLowerCase().includes(query));
}

const visibleNav = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return NAV;
  return NAV.filter((item) => {
    if (String(t(item.labelKey)).toLowerCase().includes(query)) return true;
    if (item.keys.some((key) => String(t(key)).toLowerCase().includes(query))) return true;
    if (item.id === 'engines' && settings.value) {
      return settings.value.providers.some((provider) =>
        provider.label.toLowerCase().includes(query),
      );
    }
    return false;
  });
});

watch(visibleNav, (nav) => {
  if (!nav.some((item) => item.id === activeSection.value) && nav[0]) {
    activeSection.value = nav[0].id;
  }
});

function onFindShortcut(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
    event.preventDefault();
    searchEl.value?.focus();
    searchEl.value?.select();
  }
}

onMounted(async () => {
  document.title = t('settings.title');
  window.addEventListener('keydown', onFindShortcut);
  await loadAll();
  events.preferencesChanged.listen((event) => {
    prefs.value = event.payload.snapshot;
    document.title = t('settings.title');
  });
  searchEl.value?.focus();
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onFindShortcut);
  if (permissionPoll !== undefined) {
    window.clearInterval(permissionPoll);
  }
});
</script>

<template>
  <div class="settings-shell">
    <aside class="settings-sidebar">
      <h1 class="settings-brand">{{ t('settings.title') }}</h1>
      <label class="settings-search">
        <IconSearch :size="14" :stroke-width="1.75" />
        <input
          ref="searchEl"
          v-model="searchQuery"
          type="search"
          :placeholder="t('settings.search')"
          :aria-label="t('settings.searchAria')"
          spellcheck="false"
          autocomplete="off"
        />
      </label>
      <nav class="settings-nav" :aria-label="t('settings.navAria')">
        <button
          v-for="item in visibleNav"
          :key="item.id"
          class="settings-nav-item"
          :class="{ active: activeSection === item.id }"
          type="button"
          :aria-current="activeSection === item.id ? 'page' : undefined"
          @click="activeSection = item.id"
        >
          <component :is="item.icon" :size="16" :stroke-width="1.75" />
          <span>{{ t(item.labelKey) }}</span>
        </button>
        <p v-if="!visibleNav.length" class="settings-nav-empty">{{ t('settings.noResults') }}</p>
      </nav>
    </aside>

    <main class="settings-detail">
      <div
        v-if="saveError"
        class="error-box"
        aria-live="assertive"
      >
        <div class="error-title">{{ saveError.code }} · {{ saveError.message }}</div>
        <div v-if="saveError.hint" class="muted">{{ saveError.hint }}</div>
      </div>

      <p v-if="!visibleNav.length" class="settings-detail-empty">{{ t('settings.noResults') }}</p>

      <template v-else-if="activeSection === 'general'">
        <div
          v-if="accessItem && !accessItem.granted && matchesQuery(t('settings.permissionOff'), t('permissions.accessibility.title'))"
          class="settings-banner"
        >
          <strong>{{ t('settings.permissionOff') }}</strong>
          <p>{{ permissionCopy(accessItem, 'hintOff') }}</p>
          <button class="action primary" type="button" @click="openAccessibility">
            {{ t('settings.openSystem') }}
          </button>
        </div>

        <section
          v-if="prefs && matchesQuery(t('settings.appearance'), t('settings.locale'), t('settings.theme'))"
          class="settings-group"
        >
          <div class="settings-group-head">
            <h2>{{ t('settings.appearance') }}</h2>
          </div>
          <div class="settings-list">
            <div
              v-if="matchesQuery(t('settings.locale'), t('settings.localeSystem'), t('settings.localeZh'), t('settings.localeEn'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.locale') }}</span>
                <p>{{ t('settings.localeHint') }}</p>
              </div>
              <select
                id="ui-locale"
                class="native-select"
                :value="prefs.locale"
                @change="onLocaleChange"
              >
                <option value="system">{{ t('settings.localeSystem') }}</option>
                <option value="zh-CN">{{ t('settings.localeZh') }}</option>
                <option value="en-US">{{ t('settings.localeEn') }}</option>
              </select>
            </div>
            <div
              v-if="matchesQuery(t('settings.theme'), t('settings.themeSystem'), t('settings.themeLight'), t('settings.themeDark'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.theme') }}</span>
                <p>{{ t('settings.themeHint') }}</p>
              </div>
              <select
                id="ui-theme"
                class="native-select"
                :value="prefs.theme"
                @change="onThemeChange"
              >
                <option value="system">{{ t('settings.themeSystem') }}</option>
                <option value="light">{{ t('settings.themeLight') }}</option>
                <option value="dark">{{ t('settings.themeDark') }}</option>
              </select>
            </div>
          </div>
        </section>

        <section
          v-if="prefs && matchesQuery(t('settings.startup'), t('settings.autostart'))"
          class="settings-group"
        >
          <div class="settings-group-head">
            <h2>{{ t('settings.startup') }}</h2>
          </div>
          <div class="settings-list">
            <div class="settings-item">
              <div class="settings-item-copy">
                <span>{{ t('settings.autostart') }}</span>
                <p>{{ prefs.autostart.hint || t('settings.autostartHint') }}</p>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  role="switch"
                  :checked="prefs.autostart.enabled"
                  :disabled="autostartBusy"
                  :aria-checked="prefs.autostart.enabled"
                  @change="onAutostart"
                />
                <span class="switch-track" />
              </label>
            </div>
          </div>
        </section>

        <section
          v-if="(accessItem || permissions?.platform === 'windows') && matchesQuery(t('settings.permissions'), t('permissions.accessibility.title'), t('settings.windowsNote'))"
          class="settings-group"
        >
          <div class="settings-group-head">
            <h2>{{ t('settings.permissions') }}</h2>
          </div>
          <div class="settings-list">
            <div v-if="accessItem" class="settings-item">
              <div class="settings-item-copy">
                <span>{{ permissionCopy(accessItem, 'title') }}</span>
                <p>{{ permissionCopy(accessItem, accessItem.granted ? 'hintOn' : 'description') }}</p>
              </div>
              <template v-if="accessItem.granted">
                <span class="badge on">{{ t('settings.permissionOn') }}</span>
              </template>
              <button
                v-else
                class="action"
                type="button"
                @click="openAccessibility"
              >
                {{ t('settings.openSystem') }}
              </button>
            </div>
            <div v-else-if="permissions?.platform === 'windows'" class="settings-item">
              <div class="settings-item-copy">
                <span>{{ t('settings.permissions') }}</span>
                <p>{{ t('settings.windowsNote') }}</p>
              </div>
            </div>
          </div>
        </section>
      </template>

      <template v-else-if="activeSection === 'shortcuts'">
        <Hotkeys
          v-if="hotkeys"
          :snapshot="hotkeys"
          @updated="onHotkeysUpdated"
        />
      </template>

      <template v-else-if="activeSection === 'engines'">
        <section
          v-if="settings && matchesQuery(t('settings.enginesDefaults'), t('settings.defaultEngine'), t('settings.writeHistory'))"
          class="settings-group"
        >
          <div class="settings-group-head">
            <h2>{{ t('settings.enginesDefaults') }}</h2>
            <span class="spacer" />
            <button class="action subtle" type="button" @click="loadAll">{{ t('settings.refresh') }}</button>
          </div>
          <div class="settings-list">
            <div
              v-if="matchesQuery(t('settings.defaultEngine'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.defaultEngine') }}</span>
                <p>{{ t('settings.defaultEngineHint') }}</p>
              </div>
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
            </div>
            <div
              v-if="matchesQuery(t('settings.writeHistory'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.writeHistory') }}</span>
                <p>{{ t('settings.writeHistoryHint') }}</p>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  role="switch"
                  :checked="settings.writeHistory"
                  :aria-checked="settings.writeHistory"
                  @change="onWriteHistory"
                />
                <span class="switch-track" />
              </label>
            </div>
          </div>
          <p class="settings-footnote">{{ t('settings.keyNote') }}</p>
        </section>

        <template v-for="provider in settings?.providers ?? []" :key="provider.id">
        <section
          v-if="matchesQuery(provider.label, t('settings.baseUrl'), t('settings.model'), t('settings.apiKey'), t('settings.enabled'), t('settings.disabled'))"
          class="settings-group"
        >
          <div class="settings-group-head">
            <h2>{{ provider.label }}</h2>
            <span class="badge" :class="{ on: provider.hasKey }">
              {{ provider.hasKey ? t('translate.keyOn') : t('translate.keyOff') }}
            </span>
          </div>
          <div class="settings-list">
            <div class="settings-item">
              <div class="settings-item-copy">
                <span>{{ provider.enabled ? t('settings.enabled') : t('settings.disabled') }}</span>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  role="switch"
                  :checked="provider.enabled"
                  :aria-checked="provider.enabled"
                  @change="onToggleProvider(provider.id, $event)"
                />
                <span class="switch-track" />
              </label>
            </div>
            <div class="settings-item stack">
              <div class="settings-item-copy">
                <span>{{ t('settings.baseUrl') }}</span>
              </div>
              <input
                class="text-input"
                type="url"
                :value="provider.baseUrl ?? ''"
                spellcheck="false"
                @change="onProviderField(provider.id, 'baseUrl', $event)"
              />
            </div>
            <div v-if="provider.id === 'llm'" class="settings-item stack">
              <div class="settings-item-copy">
                <span>{{ t('settings.model') }}</span>
              </div>
              <input
                class="text-input"
                type="text"
                :value="provider.model ?? ''"
                spellcheck="false"
                @change="onProviderField(provider.id, 'model', $event)"
              />
            </div>
            <div class="settings-item stack">
              <div class="settings-item-copy">
                <span>{{ t('settings.apiKey') }}</span>
              </div>
              <div class="settings-item-actions">
                <input
                  class="text-input mono"
                  type="password"
                  v-model="keyInputs[provider.id]"
                  :placeholder="t('settings.apiKey')"
                  autocomplete="off"
                  spellcheck="false"
                  @keydown.enter="saveKey(provider.id)"
                />
                <button class="action" type="button" @click="saveKey(provider.id)">
                  {{ t('settings.saveKey') }}
                </button>
              </div>
            </div>
            <div v-if="provider.testable" class="settings-item">
              <div class="settings-item-copy">
                <span>{{ t('settings.test') }}</span>
                <p v-if="testResults[provider.id]?.reason">{{ testResults[provider.id]?.reason }}</p>
              </div>
              <div class="settings-item-actions">
                <span
                  v-if="testResults[provider.id]"
                  class="test-result"
                  :class="{ fail: !testResults[provider.id]?.ok }"
                >
                  {{ testResults[provider.id]?.ok ? t('settings.testOk') : t('settings.testFail') }}
                </span>
                <button
                  class="action"
                  type="button"
                  :disabled="testing[provider.id]"
                  @click="testProvider(provider.id)"
                >
                  {{ testing[provider.id] ? t('settings.testing') : t('settings.test') }}
                </button>
              </div>
            </div>
          </div>
        </section>
        </template>
      </template>

      <template v-else-if="activeSection === 'mistakes'">
        <section class="settings-group">
          <div class="settings-group-head">
            <h2>{{ t('settings.mistakes') }}</h2>
          </div>
          <div class="settings-list">
            <div
              v-if="matchesQuery(t('settings.autoCollect'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.autoCollect') }}</span>
                <p>{{ t('settings.autoCollectHint') }}</p>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  role="switch"
                  :checked="mistakes.preferences.autoCollect !== false"
                  :aria-checked="mistakes.preferences.autoCollect !== false"
                  @change="onAutoCollect"
                />
                <span class="switch-track" />
              </label>
            </div>
            <div
              v-if="matchesQuery(t('settings.defaultStatus'))"
              class="settings-item"
            >
              <div class="settings-item-copy">
                <span>{{ t('settings.defaultStatus') }}</span>
                <p>{{ t('settings.defaultStatusHint') }}</p>
              </div>
              <select
                id="mistake-status"
                class="native-select"
                :value="mistakes.preferences.defaultStatus ?? 'open'"
                @change="onDefaultMistakeStatus"
              >
                <option value="open">{{ t('status.open') }}</option>
                <option value="learned">{{ t('status.learned') }}</option>
                <option value="archived">{{ t('status.archived') }}</option>
              </select>
            </div>
          </div>
        </section>
      </template>
    </main>
  </div>
</template>
