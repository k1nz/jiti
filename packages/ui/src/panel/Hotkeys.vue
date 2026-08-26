<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { commands, type HotkeyId, type HotkeysSnapshot } from '../ipc/bindings';
import { acceleratorFromCombo, isRecordingHotkey } from '../hotkeys';

const props = defineProps<{
  snapshot: HotkeysSnapshot;
}>();

const emit = defineEmits<{
  updated: [snapshot: HotkeysSnapshot];
}>();

const recording = ref<HotkeyId | null>(null);
const error = ref<string | null>(null);
const armed = ref(false);
let pendingAccelerator: string | null = null;
let listening = false;

function unwrap<T>(promise: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: unknown }>) {
  return promise.then((result) => {
    if (result.status === 'ok') return result.data;
    throw result.error;
  });
}

const unregistered = computed(() => props.snapshot.bindings.filter((bind) => !bind.registered));

function attachListeners() {
  if (listening) return;
  window.addEventListener('keydown', onCaptureKeydown, true);
  window.addEventListener('pointerdown', onCapturePointerDown, true);
  listening = true;
}

function detachListeners() {
  if (!listening) return;
  window.removeEventListener('keydown', onCaptureKeydown, true);
  window.removeEventListener('pointerdown', onCapturePointerDown, true);
  listening = false;
}

function onCapturePointerDown(event: PointerEvent) {
  if (recording.value === null) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest('.hotkey-bind')) return;
  void stopRecord();
}

function onCaptureKeydown(event: KeyboardEvent) {
  if (recording.value === null || event.repeat) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (event.key === 'Escape') {
    void stopRecord();
    return;
  }
  const accelerator = acceleratorFromCombo(event);
  if (!accelerator) return;
  if (!armed.value) {
    pendingAccelerator = accelerator;
    return;
  }
  void commit(recording.value, accelerator);
}

async function startRecord(id: HotkeyId, event: MouseEvent) {
  error.value = null;
  pendingAccelerator = null;
  recording.value = id;
  isRecordingHotkey.value = true;
  armed.value = false;
  attachListeners();
  (event.currentTarget as HTMLButtonElement | null)?.focus();
  try {
    await commands.hotkeysSuspend();
    if (recording.value !== id) return;
    armed.value = true;
    if (pendingAccelerator) {
      const accelerator = pendingAccelerator;
      pendingAccelerator = null;
      await commit(id, accelerator);
    }
  } catch (err) {
    recording.value = null;
    isRecordingHotkey.value = false;
    armed.value = false;
    detachListeners();
    error.value = String(err);
  }
}

async function stopRecord() {
  if (recording.value === null) return;
  recording.value = null;
  isRecordingHotkey.value = false;
  armed.value = false;
  pendingAccelerator = null;
  detachListeners();
  try {
    await commands.hotkeysResume();
  } catch (err) {
    error.value = String(err);
  }
}

async function commit(id: HotkeyId, accelerator: string) {
  recording.value = null;
  isRecordingHotkey.value = false;
  armed.value = false;
  pendingAccelerator = null;
  detachListeners();
  try {
    emit('updated', await unwrap(commands.hotkeysSet(id, accelerator)));
    error.value = null;
  } catch (err) {
    error.value = String(err);
    try {
      await commands.hotkeysResume();
    } catch {
      // 恢复注册失败时保持错误提示。
    }
  }
}

async function reset() {
  recording.value = null;
  isRecordingHotkey.value = false;
  armed.value = false;
  pendingAccelerator = null;
  detachListeners();
  try {
    emit('updated', await unwrap(commands.hotkeysReset()));
    error.value = null;
  } catch (err) {
    error.value = String(err);
  }
}

onBeforeUnmount(() => {
  if (recording.value === null) return;
  recording.value = null;
  isRecordingHotkey.value = false;
  armed.value = false;
  pendingAccelerator = null;
  detachListeners();
  void commands.hotkeysResume();
});
</script>

<template>
  <div class="hotkeys">
    <div class="pane-header">
      <span>快捷键</span>
      <button class="action subtle" type="button" @click="reset">恢复默认</button>
    </div>
    <p class="muted settings-note">点击组合键，再按下新的快捷键。Esc 取消。</p>
    <div
      v-for="bind in snapshot.bindings"
      :key="bind.id"
      class="hotkey-row"
    >
      <span class="hotkey-title">{{ bind.title }}</span>
      <button
        class="hotkey-bind"
        :class="{ recording: recording === bind.id, warn: !bind.registered }"
        type="button"
        tabindex="0"
        :aria-label="`修改${bind.title}快捷键，当前 ${bind.display}`"
        :aria-pressed="recording === bind.id"
        @click="startRecord(bind.id, $event)"
      >
        {{ recording === bind.id ? '按下组合键…' : bind.display }}
      </button>
    </div>
    <p v-if="error" class="hotkey-warn">{{ error }}</p>
    <p v-else-if="unregistered.length" class="hotkey-warn">
      {{ unregistered.map((bind) => bind.display).join('、') }}
      未能注册，可能已被其他程序占用。
    </p>
  </div>
</template>
