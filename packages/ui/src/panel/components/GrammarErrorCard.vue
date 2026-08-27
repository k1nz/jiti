<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import type { GrammarError } from '../../ipc/bindings';

export type CollectState = 'pending' | 'collected' | 'idle' | 'hidden';

defineProps<{
  error: GrammarError;
  collectState?: CollectState;
}>();

const emit = defineEmits<{
  collect: [];
  uncollect: [];
}>();

const { t } = useI18n();
</script>

<template>
  <article
    class="grammar-card"
    :class="`sev-${error.severity}`"
    :aria-label="`${t(`errorType.${error.type}`)} · ${error.severity}`"
  >
    <span class="grammar-card-type">{{ t(`errorType.${error.type}`) }}</span>
    <div class="grammar-card-pair">
      <span class="grammar-from">{{ error.fragment }}</span>
      <span class="grammar-arrow" aria-hidden="true">→</span>
      <span class="grammar-to">{{ error.correction }}</span>
    </div>
    <p class="grammar-explain">{{ error.explanation }}</p>
    <ul v-if="error.suggestions.length" class="grammar-suggestions">
      <li v-for="(item, index) in error.suggestions" :key="`${item}-${index}`">{{ item }}</li>
    </ul>
    <div v-if="collectState && collectState !== 'hidden'" class="grammar-collect">
      <button
        v-if="collectState === 'pending'"
        class="action subtle"
        type="button"
        disabled
      >
        {{ t('mistakes.pending') }}
      </button>
      <template v-else-if="collectState === 'collected'">
        <span class="collect-on">{{ t('mistakes.collected') }}</span>
        <button class="action subtle" type="button" @click="emit('uncollect')">{{ t('mistakes.uncollect') }}</button>
      </template>
      <button
        v-else
        class="action subtle"
        type="button"
        @click="emit('collect')"
      >
        {{ t('mistakes.collect') }}
      </button>
    </div>
  </article>
</template>
