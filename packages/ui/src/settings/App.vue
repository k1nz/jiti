<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
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

onMounted(async () => {
  document.title = t('settings.title');
  await loadAll();
  events.preferencesChanged.listen((event) => {
    prefs.value = event.payload.snapshot;
    document.title = t('settings.title');
  });
});
</script>

<template>
  <div class="settings-page">
    <section class="settings-view">
      <div class="pane-header">
        <span>{{ t('settings.general') }}</span>
      </div>
      <div v-if="prefs" class="setting-row wrap">
        <label class="field-label" for="ui-locale">{{ t('settings.locale') }}</label>
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
        <label class="field-label" for="ui-theme">{{ t('settings.theme') }}</label>
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
        <label class="check">
          <input
            type="checkbox"
            :checked="prefs.autostart.enabled"
            :disabled="autostartBusy"
            @change="onAutostart"
          />
          {{ t('settings.autostart') }}
        </label>
      </div>
      <p v-if="prefs?.autostart.hint" class="muted settings-note">{{ prefs.autostart.hint }}</p>

      <div
        v-if="saveError"
        class="error-box"
        aria-live="assertive"
      >
        <div class="error-title">{{ saveError.code }} · {{ saveError.message }}</div>
        <div v-if="saveError.hint" class="muted">{{ saveError.hint }}</div>
      </div>

      <div
        v-if="accessItem && !accessItem.granted"
        class="permission-card"
      >
        <span>{{ t('settings.permissionOff') }}</span>
        <p class="muted">{{ permissionCopy(accessItem, 'hintOff') }}</p>
        <button class="action" type="button" @click="openAccessibility">{{ t('settings.openSystem') }}</button>
      </div>

      <p v-else-if="permissions?.platform === 'windows'" class="muted settings-note">
        {{ t('settings.windowsNote') }}
      </p>

      <Hotkeys
        v-if="hotkeys"
        :snapshot="hotkeys"
        @updated="onHotkeysUpdated"
      />

      <div class="pane-header">
        <span>{{ t('settings.engines') }}</span>
        <button class="action subtle" type="button" @click="loadAll">{{ t('settings.refresh') }}</button>
      </div>
      <p class="muted settings-note">{{ t('settings.keyNote') }}</p>

      <div v-if="settings" class="setting-row">
        <label class="field-label" for="default-engine">{{ t('settings.defaultEngine') }}</label>
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
          {{ t('settings.writeHistory') }}
        </label>
      </div>

      <div class="pane-header">
        <span>{{ t('settings.mistakes') }}</span>
      </div>
      <div class="setting-row">
        <label class="check">
          <input
            type="checkbox"
            :checked="mistakes.preferences.autoCollect !== false"
            @change="onAutoCollect"
          />
          {{ t('settings.autoCollect') }}
        </label>
        <label class="field-label" for="mistake-status">{{ t('settings.defaultStatus') }}</label>
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

      <div v-if="settings" class="provider-stack">
        <div v-for="provider in settings.providers" :key="provider.id" class="provider-card">
          <div class="provider-head">
            <strong>{{ provider.label }}</strong>
            <label class="toggle">
              <input
                type="checkbox"
                :checked="provider.enabled"
                @change="onToggleProvider(provider.id, $event)"
              />
              <span>{{ provider.enabled ? t('settings.enabled') : t('settings.disabled') }}</span>
            </label>
          </div>
          <div class="field-grid">
            <label class="field-label">{{ t('settings.baseUrl') }}</label>
            <input
              class="text-input"
              type="url"
              :value="provider.baseUrl ?? ''"
              spellcheck="false"
              @change="onProviderField(provider.id, 'baseUrl', $event)"
            />
            <template v-if="provider.id === 'llm'">
              <label class="field-label">{{ t('settings.model') }}</label>
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
              :placeholder="t('settings.apiKey')"
              autocomplete="off"
              spellcheck="false"
            />
            <button class="action subtle" type="button" @click="saveKey(provider.id)">
              {{ t('settings.saveKey') }}
            </button>
          </div>
          <div class="test-row">
            <span class="badge" :class="{ on: provider.hasKey }">
              {{ provider.hasKey ? t('translate.keyOn') : t('translate.keyOff') }}
            </span>
            <button
              v-if="provider.testable"
              class="action subtle"
              type="button"
              :disabled="testing[provider.id]"
              @click="testProvider(provider.id)"
            >
              {{ testing[provider.id] ? t('settings.testing') : t('settings.test') }}
            </button>
            <span v-if="testResults[provider.id]" class="test-result" :class="{ fail: !testResults[provider.id]?.ok }">
              {{ testResults[provider.id]?.ok ? t('settings.testOk') : t('settings.testFail') }}
            </span>
          </div>
          <p v-if="testResults[provider.id]?.reason" class="test-reason">
            {{ testResults[provider.id]?.reason }}
          </p>
        </div>
      </div>
    </section>
  </div>
</template>
