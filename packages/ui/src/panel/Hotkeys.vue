<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { commands, type HotkeyId, type HotkeysSnapshot } from '../ipc/bindings';
import { acceleratorFromCombo } from '../hotkeys';

const props = defineProps<{
  snapshot: HotkeysSnapshot;
}>();

const emit = defineEmits<{
  updated: [snapshot: HotkeysSnapshot];
}>();

const recording = ref<HotkeyId | null>(null);
const error = ref<string | null>(null);

function unwrap<T>(promise: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: unknown }>) {
  return promise.then((result) => {
    if (result.status === 'ok') return result.data;
    throw result.error;
  });
}

const unregistered = computed(() => props.snapshot.bindings.filter((bind) => !bind.registered));

async function startRecord(id: HotkeyId) {
  error.value = null;
  recording.value = id;
  try {
    await commands.hotkeysSuspend();
  } catch (err) {
    recording.value = null;
    error.value = String(err);
  }
}

async function stopRecord() {
  if (recording.value === null) return;
  recording.value = null;
  try {
    await commands.hotkeysResume();
  } catch (err) {
    error.value = String(err);
  }
}

async function onBindKeydown(id: HotkeyId, event: KeyboardEvent) {
  if (recording.value !== id) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === 'Escape') {
    await stopRecord();
    return;
  }
  const accelerator = acceleratorFromCombo(event);
  if (!accelerator) return;
  recording.value = null;
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
        :aria-label="`修改${bind.title}快捷键，当前 ${bind.display}`"
        :aria-pressed="recording === bind.id"
        @click="startRecord(bind.id)"
        @keydown="onBindKeydown(bind.id, $event)"
        @blur="stopRecord"
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
