<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { IconBinaryTree, IconPlayerPlay } from '@tabler/icons-vue';
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

    <div v-else-if="showResults" class="syntax-modules" data-tauri-drag-region="false" aria-live="polite">
      <section v-if="syntax.chunks.length" class="syntax-module">
        <h3 class="syntax-heading">{{ t('syntax.chunks') }}</h3>
        <ul class="syntax-chunks">
          <li v-for="(item, index) in syntax.chunks" :key="`chunk-${index}`" class="syntax-chunk">
            <p class="syntax-chunk-line">
              <span class="syntax-quote">{{ item.text }}</span>
              <span class="syntax-gloss">{{ item.gloss }}</span>
            </p>
            <ul v-if="item.notes.length" class="syntax-notes">
              <li v-for="(note, noteIndex) in item.notes" :key="`note-${index}-${noteIndex}`">
                {{ note }}
              </li>
            </ul>
          </li>
        </ul>
      </section>

      <section v-if="syntax.readings.length" class="syntax-module">
        <h3 class="syntax-heading">{{ t('syntax.readings') }}</h3>
        <ol class="syntax-readings">
          <li
            v-for="(item, index) in syntax.readings"
            :key="`reading-${index}`"
            class="syntax-reading"
            :class="{ natural: item.tone === 'natural' }"
          >
            <span class="syntax-tone">{{ t(`syntax.tone.${item.tone}`) }}</span>
            <p>{{ item.text }}</p>
          </li>
        </ol>
      </section>

      <section v-if="syntax.structures.length" class="syntax-module">
        <h3 class="syntax-heading">{{ t('syntax.structure') }}</h3>
        <ul class="grammar-cards">
          <li v-for="(item, index) in syntax.structures" :key="`structure-${index}`">
            <article class="grammar-card">
              <span class="grammar-card-type">{{ t(`syntax.role.${item.role}`) }}</span>
              <p class="syntax-span">{{ item.text }}</p>
              <p class="syntax-label">{{ item.label }}</p>
              <p v-if="item.note" class="grammar-explain">{{ item.note }}</p>
            </article>
          </li>
        </ul>
      </section>

      <section v-if="syntax.skeletons.length" class="syntax-module">
        <h3 class="syntax-heading">{{ t('syntax.skeleton') }}</h3>
        <ol class="syntax-skeleton">
          <li v-for="(item, index) in syntax.skeletons" :key="`skeleton-${index}`">
            <span class="syntax-skeleton-role">{{ t(`syntax.role.${item.role}`) }}</span>
            <span class="syntax-skeleton-text">{{ item.text }}</span>
          </li>
        </ol>
      </section>

      <section v-if="syntax.tips.length" class="syntax-module">
        <h3 class="syntax-heading">{{ t('syntax.tips') }}</h3>
        <ol class="syntax-tips">
          <li v-for="(tip, index) in syntax.tips" :key="`tip-${index}`">{{ tip }}</li>
        </ol>
      </section>

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
