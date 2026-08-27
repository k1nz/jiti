<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { formatTime } from '../../format';

export type CollectState = 'pending' | 'collected' | 'idle' | 'hidden';

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

const { t, locale } = useI18n();

function typeLabel(value: string | null) {
  if (!value) return t('errorType.other');
  const key = `errorType.${value}`;
  return t(key) === key ? t('errorType.other') : t(key);
}

function statusLabel(value: string) {
  const key = `status.${value}`;
  return t(key) === key ? value : t(key);
}
</script>

<template>
  <article
    class="grammar-card mistake-card"
    :class="`sev-${props.severity}`"
  >
    <div class="mistake-card-head">
      <span class="grammar-card-type">{{ typeLabel(props.errorType) }}</span>
      <span class="mistake-status">{{ statusLabel(props.status) }}</span>
      <span class="mistake-time">{{ formatTime(props.createdAt, locale) }}</span>
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
        {{ t('mistakes.learn') }}
      </button>
      <button
        v-if="props.status !== 'archived'"
        class="action subtle"
        type="button"
        @click="emit('archive')"
      >
        {{ t('mistakes.archive') }}
      </button>
      <button
        v-if="props.status !== 'open'"
        class="action subtle"
        type="button"
        @click="emit('restore')"
      >
        {{ t('mistakes.restore') }}
      </button>
      <button class="action subtle danger-text" type="button" @click="emit('remove')">{{ t('mistakes.remove') }}</button>
    </div>
  </article>
</template>
