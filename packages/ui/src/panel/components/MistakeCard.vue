<script setup lang="ts">
export type CollectState = 'pending' | 'collected' | 'idle' | 'hidden';

const TYPE_LABEL: Record<string, string> = {
  grammar: '语法',
  spelling: '拼写',
  punctuation: '标点',
  word_choice: '用词',
  style: '风格',
};

const STATUS_LABEL: Record<string, string> = {
  open: '未掌握',
  learned: '已掌握',
  archived: '已归档',
};

const props = defineProps<{
  sourceText: string;
  fragment: string;
  correction: string;
  errorType: string | null;
  severity: string;
  explanation: string | null;
  status: string;
  createdAt: string;
}>();

const emit = defineEmits<{
  learn: [];
  archive: [];
  restore: [];
  remove: [];
}>();

function formatTime(value: string) {
  const date = new Date(value.replace(' ', 'T') + 'Z');
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN', { hour12: false });
}
</script>

<template>
  <article
    class="grammar-card mistake-card"
    :class="`sev-${props.severity}`"
  >
    <div class="mistake-card-head">
      <span class="grammar-card-type">{{ TYPE_LABEL[props.errorType ?? ''] ?? '其他' }}</span>
      <span class="mistake-status">{{ STATUS_LABEL[props.status] ?? props.status }}</span>
      <span class="mistake-time">{{ formatTime(props.createdAt) }}</span>
    </div>
    <p class="mistake-source">{{ props.sourceText }}</p>
    <div class="grammar-card-pair">
      <span class="grammar-from">{{ props.fragment }}</span>
      <span class="grammar-arrow" aria-hidden="true">→</span>
      <span class="grammar-to">{{ props.correction }}</span>
    </div>
    <p v-if="props.explanation" class="grammar-explain">{{ props.explanation }}</p>
    <div class="mistake-actions">
      <button
        v-if="props.status === 'open'"
        class="action subtle"
        type="button"
        @click="emit('learn')"
      >
        掌握
      </button>
      <button
        v-if="props.status !== 'archived'"
        class="action subtle"
        type="button"
        @click="emit('archive')"
      >
        归档
      </button>
      <button
        v-if="props.status !== 'open'"
        class="action subtle"
        type="button"
        @click="emit('restore')"
      >
        恢复
      </button>
      <button class="action subtle danger-text" type="button" @click="emit('remove')">删除</button>
    </div>
  </article>
</template>
