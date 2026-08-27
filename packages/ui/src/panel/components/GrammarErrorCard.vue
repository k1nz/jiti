<script setup lang="ts">
import type { GrammarError } from '../../ipc/bindings';

const TYPE_LABEL: Record<GrammarError['type'], string> = {
  grammar: '语法',
  spelling: '拼写',
  punctuation: '标点',
  word_choice: '用词',
  style: '风格',
};

export type CollectState = 'pending' | 'collected' | 'idle' | 'hidden';

defineProps<{
  error: GrammarError;
  collectState?: CollectState;
}>();

const emit = defineEmits<{
  collect: [];
  uncollect: [];
}>();
</script>

<template>
  <article
    class="grammar-card"
    :class="`sev-${error.severity}`"
    :aria-label="`${TYPE_LABEL[error.type]} · ${error.severity}`"
  >
    <span class="grammar-card-type">{{ TYPE_LABEL[error.type] }}</span>
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
        待完成
      </button>
      <template v-else-if="collectState === 'collected'">
        <span class="collect-on">已收录</span>
        <button class="action subtle" type="button" @click="emit('uncollect')">取消收录</button>
      </template>
      <button
        v-else
        class="action subtle"
        type="button"
        @click="emit('collect')"
      >
        收录错题
      </button>
    </div>
  </article>
</template>
