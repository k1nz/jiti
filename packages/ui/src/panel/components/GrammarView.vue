<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { IconAbc, IconCopy, IconPlayerPlay } from '@tabler/icons-vue';
import { commands } from '../../ipc/bindings';
import type { GrammarError, NewMistake } from '../../ipc/bindings';
import { unwrap } from '../../ipc/unwrap';
import { useGrammarStore } from '../../stores/grammar';
import { useMistakesStore } from '../../stores/mistakes';
import GrammarErrorCard, { type CollectState } from './GrammarErrorCard.vue';

const props = defineProps<{
  input: string;
  hint: string;
  copyLabel: string;
}>();

const emit = defineEmits<{
  check: [];
  copy: [];
}>();

const { t } = useI18n();
const grammar = useGrammarStore();
const mistakes = useMistakesStore();

function collectState(index: number): CollectState {
  if (grammar.status === 'loading' || grammar.status === 'streaming') {
    return mistakes.preferences.autoCollect !== false ? 'pending' : 'hidden';
  }
  if (grammar.mistakeIds[index] != null) return 'collected';
  return 'idle';
}

function toNewMistake(error: GrammarError): NewMistake {
  return {
    sourceText: grammar.result?.input ?? '',
    fragment: error.fragment,
    correction: error.correction,
    errorType: error.type,
    severity: error.severity,
    explanation: error.explanation,
    correctedSentence: grammar.correctedText,
    suggestions: error.suggestions,
    engine: grammar.result?.engine ?? null,
    sourceApp: null,
    tags: null,
    status: mistakes.preferences.defaultStatus ?? 'open',
  };
}

async function collect(index: number) {
  const error = grammar.errors[index];
  if (!error) return;
  try {
    const created = await unwrap(commands.mistakesCreate(toNewMistake(error)));
    grammar.setMistakeId(index, created.id);
  } catch {
    /* 收录失败时保持未收录，不打断检查结果 */
  }
}

async function uncollect(index: number) {
  const id = grammar.mistakeIds[index];
  if (id == null) return;
  try {
    await unwrap(commands.mistakesDelete(id));
    grammar.setMistakeId(index, null);
  } catch {
    /* 取消失败时保持已收录 */
  }
}

const busy = computed(() => grammar.status === 'loading' || grammar.status === 'streaming');
const canCheck = computed(() => props.input.trim().length > 0 && !busy.value);
const showSpinner = computed(() => grammar.spinnerVisible && !grammar.hasContent);
const showResults = computed(
  () => grammar.hasContent || grammar.status === 'done' || grammar.status === 'streaming',
);
const statusLabel = computed(() => {
  if (grammar.retrying) return t('grammar.retrying');
  if (busy.value) return t('grammar.checking');
  return '';
});
</script>

<template>
  <section class="grammar-view">
    <div class="toolbar">
      <button class="action" type="button" :disabled="!canCheck" @click="emit('check')">
        <IconPlayerPlay :size="14" :stroke-width="1.75" />
        {{ t('grammar.action') }}
      </button>
      <span v-if="statusLabel" class="grammar-retry" aria-live="polite">{{ statusLabel }}</span>
      <span class="spacer"></span>
      <button
        v-if="grammar.correctedText"
        class="icon-btn"
        type="button"
            :aria-label="t('grammar.copy')"
            :title="t('grammar.copy')"
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
        {{ t('grammar.ok') }}
      </p>
      <ul v-if="grammar.errors.length" class="grammar-cards">
        <li v-for="(item, index) in grammar.errors" :key="`${item.fragment}-${index}`">
          <GrammarErrorCard
            :error="item"
            :collect-state="collectState(index)"
            @collect="collect(index)"
            @uncollect="uncollect(index)"
          />
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
