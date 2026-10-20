<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { IconBinaryTree, IconPlayerPlay } from '@tabler/icons-vue';
import type { SyntaxProfile } from '../../ipc/bindings';
import { useSyntaxStore } from '../../stores/syntax';
import ClipBookmark from './ClipBookmark.vue';

const props = defineProps<{
  input: string;
  hint: string;
  saved?: boolean;
}>();

const emit = defineEmits<{
  analyze: [];
  saveClip: [text: string];
}>();

const { t } = useI18n();
const syntax = useSyntaxStore();

const busy = computed(() => syntax.status === 'loading' || syntax.status === 'streaming');
const canAnalyze = computed(() => props.input.trim().length > 0 && !busy.value);
const showSpinner = computed(() => syntax.spinnerVisible && !syntax.hasContent);
const showResults = computed(
  () => syntax.hasContent || syntax.status === 'done' || syntax.status === 'streaming',
);
const statusLabel = computed(() => {
  if (syntax.retrying) return t('syntax.retrying');
  if (busy.value) return t('syntax.analyzing');
  return '';
});

const chips = computed(() => chipsFor(syntax.profile));

function chipsFor(profile: SyntaxProfile | null): string[] {
  if (!profile) return [];
  const items = [
    t(`syntax.language.${profile.language}`),
    t(`syntax.sentence.${profile.sentenceType}`),
  ];
  if (profile.tense) items.push(t(`syntax.tense.${profile.tense}`));
  if (profile.aspect) items.push(t(`syntax.aspect.${profile.aspect}`));
  if (profile.voice) items.push(t(`syntax.voice.${profile.voice}`));
  return items;
}
</script>

<template>
  <section class="syntax-view">
    <div class="toolbar">
      <button class="action" type="button" :disabled="!canAnalyze" @click="emit('analyze')">
        <IconPlayerPlay :size="14" :stroke-width="1.75" />
        {{ t('syntax.action') }}
      </button>
      <span v-if="statusLabel" class="grammar-retry" aria-live="polite">{{ statusLabel }}</span>
      <span class="spacer"></span>
      <ClipBookmark
        v-if="props.input.trim()"
        :saved="saved"
        :label="t('clips.saveSentence')"
        @click="emit('saveClip', props.input)"
      />
    </div>

    <div v-if="showSpinner" class="state-box" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>{{ statusLabel }}</span>
    </div>

    <div
      v-else-if="syntax.status === 'error' && syntax.error"
      class="error-box"
      data-tauri-drag-region="false"
      aria-live="assertive"
    >
      <div class="error-title">{{ syntax.error.code }} · {{ syntax.error.message }}</div>
      <div v-if="syntax.error.hint" class="muted">{{ syntax.error.hint }}</div>
      <code class="copyable">{{ syntax.error.copyable }}</code>
    </div>

    <div v-else-if="showResults" class="grammar-results" data-tauri-drag-region="false" aria-live="polite">
      <p v-if="syntax.summary" class="grammar-overall">{{ syntax.summary }}</p>
      <ul v-if="chips.length" class="syntax-chips">
        <li v-for="chip in chips" :key="chip" class="syntax-chip">{{ chip }}</li>
      </ul>
      <template v-if="syntax.constituents.length">
        <h3 class="syntax-heading">{{ t('syntax.constituents') }}</h3>
        <ul class="grammar-cards">
          <li v-for="(item, index) in syntax.constituents" :key="`c-${item.role}-${index}`">
            <article class="grammar-card">
              <span class="grammar-card-type">{{ t(`syntax.role.${item.role}`) }}</span>
              <p class="syntax-span">{{ item.text }}</p>
              <p class="grammar-explain">{{ item.note }}</p>
            </article>
          </li>
        </ul>
      </template>
      <template v-if="syntax.clauses.length">
        <h3 class="syntax-heading">{{ t('syntax.clauses') }}</h3>
        <ul class="grammar-cards">
          <li v-for="(item, index) in syntax.clauses" :key="`k-${item.kind}-${index}`">
            <article class="grammar-card">
              <span class="grammar-card-type">{{ t(`syntax.clause.${item.kind}`) }}</span>
              <p class="syntax-span">{{ item.text }}</p>
              <p class="grammar-explain">{{ item.note }}</p>
            </article>
          </li>
        </ul>
      </template>
      <div v-if="syntax.result" class="meta">
        <span>{{ syntax.result.engine }}</span>
        <span>{{ syntax.result.durationMs }} ms</span>
      </div>
    </div>

    <div v-else class="state-box">
      <IconBinaryTree :size="26" :stroke-width="1.5" />
      <span>{{ hint }}</span>
    </div>
  </section>
</template>
