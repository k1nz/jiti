<script setup lang="ts">
import { computed } from 'vue';
import { IconAbc, IconCopy, IconPlayerPlay } from '@tabler/icons-vue';
import { useGrammarStore } from '../../stores/grammar';
import GrammarErrorCard from './GrammarErrorCard.vue';

const props = defineProps<{
  input: string;
  hint: string;
  copyLabel: string;
}>();

const emit = defineEmits<{
  check: [];
  copy: [];
}>();

const grammar = useGrammarStore();

const busy = computed(() => grammar.status === 'loading' || grammar.status === 'streaming');
const canCheck = computed(() => props.input.trim().length > 0 && !busy.value);
const showSpinner = computed(() => busy.value && !grammar.hasContent);
const showResults = computed(
  () => grammar.hasContent || grammar.status === 'done' || grammar.status === 'streaming',
);
const statusLabel = computed(() => {
  if (grammar.retrying) return '正在重新解析';
  if (busy.value) return '正在检查';
  return '';
});
</script>

<template>
  <section class="grammar-view">
    <div class="toolbar">
      <button class="action" type="button" :disabled="!canCheck" @click="emit('check')">
        <IconPlayerPlay :size="14" :stroke-width="1.75" />
        检查
      </button>
      <span v-if="statusLabel" class="grammar-retry" aria-live="polite">{{ statusLabel }}</span>
      <span class="spacer"></span>
      <button
        v-if="grammar.correctedText"
        class="icon-btn"
        type="button"
        aria-label="复制改写"
        title="复制改写"
        @click="emit('copy')"
      >
        <IconCopy :size="15" :stroke-width="1.75" />
      </button>
      <span v-if="copyLabel" class="copy-label">{{ copyLabel }}</span>
    </div>

    <div v-if="showSpinner" class="state-box" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>{{ statusLabel }}</span>
    </div>

    <div
      v-else-if="grammar.status === 'error' && grammar.error"
      class="error-box"
      data-tauri-drag-region="false"
      aria-live="assertive"
    >
      <div class="error-title">{{ grammar.error.code }} · {{ grammar.error.message }}</div>
      <div v-if="grammar.error.hint" class="muted">{{ grammar.error.hint }}</div>
      <code class="copyable">{{ grammar.error.copyable }}</code>
    </div>

    <div v-else-if="showResults" class="grammar-results" data-tauri-drag-region="false" aria-live="polite">
      <p v-if="grammar.overall" class="grammar-overall">{{ grammar.overall }}</p>
      <p v-if="grammar.correctedText" class="grammar-corrected">{{ grammar.correctedText }}</p>
      <p
        v-if="!busy && grammar.status === 'done' && grammar.errors.length === 0"
        class="grammar-success"
      >
        未发现语法问题
      </p>
      <ul v-if="grammar.errors.length" class="grammar-cards">
        <li v-for="(item, index) in grammar.errors" :key="`${item.fragment}-${index}`">
          <GrammarErrorCard :error="item" />
        </li>
      </ul>
      <div v-if="grammar.result" class="meta">
        <span>{{ grammar.result.engine }}</span>
        <span>{{ grammar.result.durationMs }} ms</span>
      </div>
    </div>

    <div v-else class="state-box">
      <IconAbc :size="26" :stroke-width="1.5" />
      <span>{{ hint }}</span>
    </div>
  </section>
</template>
