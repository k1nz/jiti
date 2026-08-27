<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { IconAlertCircle, IconDownload, IconNotebook, IconSparkles } from '@tabler/icons-vue';
import { commands } from '../../ipc/bindings';
import type { EngineErrorPayload, MistakeFilter, MistakeTimeRange } from '../../ipc/bindings';
import { unwrap } from '../../ipc/unwrap';
import { renderReviewMarkdown } from '../../markdown';
import { useMistakesStore } from '../../stores/mistakes';
import MistakeCard from './MistakeCard.vue';

const TYPE_CHIPS: ReadonlyArray<{ value: string | null; labelKey: string }> = [
  { value: null, labelKey: 'mistakes.allTypes' },
  { value: 'grammar', labelKey: 'errorType.grammar' },
  { value: 'spelling', labelKey: 'errorType.spelling' },
  { value: 'punctuation', labelKey: 'errorType.punctuation' },
  { value: 'word_choice', labelKey: 'errorType.word_choice' },
  { value: 'style', labelKey: 'errorType.style' },
];

const STATUS_CHIPS: ReadonlyArray<{ value: string | null; labelKey: string }> = [
  { value: null, labelKey: 'mistakes.allStatus' },
  { value: 'open', labelKey: 'status.open' },
  { value: 'learned', labelKey: 'status.learned' },
  { value: 'archived', labelKey: 'status.archived' },
];

const TIME_CHIPS: ReadonlyArray<{ value: MistakeTimeRange; labelKey: string }> = [
  { value: 'sevenDays', labelKey: 'mistakes.sevenDays' },
  { value: 'thirtyDays', labelKey: 'mistakes.thirtyDays' },
  { value: 'all', labelKey: 'mistakes.allTime' },
];

const { t } = useI18n();
const mistakes = useMistakesStore();

function currentFilter(): MistakeFilter {
  return { ...mistakes.filter };
}

async function reload() {
  const id = mistakes.beginLoad();
  try {
    const list = await unwrap(commands.mistakesList(currentFilter()));
    mistakes.applyList(id, list);
  } catch (err) {
    mistakes.failList(id, String(err));
  }
}

async function onType(value: string | null) {
  mistakes.setFilter({ errorType: value });
  mistakes.clearReview();
  await reload();
}

async function onStatus(value: string | null) {
  mistakes.setFilter({ status: value });
  mistakes.clearReview();
  await reload();
}

async function onTime(value: MistakeTimeRange) {
  mistakes.setFilter({ timeRange: value });
  mistakes.clearReview();
  await reload();
}

async function patchStatus(id: number, status: string) {
  try {
    const updated = await unwrap(commands.mistakesUpdate(id, { status }));
    mistakes.applyMistake(updated);
  } catch (err) {
    mistakes.failList(mistakes.listId, String(err));
  }
}

async function removeItem(id: number) {
  try {
    await unwrap(commands.mistakesDelete(id));
    mistakes.removeMistake(id);
  } catch (err) {
    mistakes.failList(mistakes.listId, String(err));
  }
}

async function exportMarkdown() {
  mistakes.setExporting(true);
  try {
    await unwrap(commands.mistakesExport(currentFilter()));
  } catch (err) {
    mistakes.failList(mistakes.listId, String(err));
  } finally {
    mistakes.setExporting(false);
  }
}

async function runReview() {
  const id = mistakes.beginReview();
  try {
    const result = await unwrap(commands.mistakesAiReview(currentFilter()));
    mistakes.applyReview(id, result);
  } catch (err) {
    const payload = err as EngineErrorPayload;
    if (payload && typeof payload === 'object' && 'code' in payload) {
      mistakes.failReview(id, payload);
    } else {
      mistakes.failReview(id, {
        provider: 'ai_review',
        code: 'invalid_config',
        message: String(err),
        hint: null,
        copyable: `[jiti] ai_review ${String(err)}`,
      });
    }
  }
}

const busy = computed(() => mistakes.status === 'loading');
const reviewHtml = computed(() =>
  mistakes.reviewResult ? renderReviewMarkdown(mistakes.reviewResult.summary) : '',
);

onMounted(() => {
  void reload();
});
</script>

<template>
  <section class="mistakes-view">
    <div class="pane-header">
      <span>{{ t('mistakes.title') }}</span>
      <span class="spacer"></span>
      <button
        class="action subtle"
        type="button"
        :disabled="mistakes.exporting || busy"
        @click="exportMarkdown"
      >
        <IconDownload :size="14" :stroke-width="1.75" />
        {{ t('mistakes.export') }}
      </button>
      <button
        class="action"
        type="button"
        :disabled="mistakes.reviewStatus === 'loading' || busy || mistakes.empty"
        @click="runReview"
      >
        <IconSparkles :size="14" :stroke-width="1.75" />
        {{ t('mistakes.review') }}
      </button>
    </div>

    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in TYPE_CHIPS"
        :key="`type-${chip.labelKey}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.errorType ?? null) === chip.value }"
        @click="onType(chip.value)"
      >
        {{ t(chip.labelKey) }}
      </button>
    </div>
    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in STATUS_CHIPS"
        :key="`status-${chip.labelKey}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.status ?? null) === chip.value }"
        @click="onStatus(chip.value)"
      >
        {{ t(chip.labelKey) }}
      </button>
    </div>
    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in TIME_CHIPS"
        :key="`time-${chip.labelKey}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.timeRange ?? 'all') === chip.value }"
        @click="onTime(chip.value)"
      >
        {{ t(chip.labelKey) }}
      </button>
    </div>

    <div v-if="mistakes.reviewStatus === 'loading'" class="state-box compact" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>{{ t('mistakes.reviewing') }}</span>
    </div>
    <div
      v-else-if="mistakes.reviewStatus === 'error' && mistakes.reviewError"
      class="error-box"
      data-tauri-drag-region="false"
      aria-live="assertive"
    >
      <div class="error-title">{{ mistakes.reviewError.code }} · {{ mistakes.reviewError.message }}</div>
      <div v-if="mistakes.reviewError.hint" class="muted">{{ mistakes.reviewError.hint }}</div>
    </div>
    <div v-else-if="mistakes.reviewResult" class="review-box" data-tauri-drag-region="false">
      <div class="review-summary markdown-body" v-html="reviewHtml"></div>
      <div class="meta">
        <span>{{ t('mistakes.count', { n: mistakes.reviewResult.analyzedCount }) }}</span>
        <span>{{ mistakes.reviewResult.engine }}</span>
        <span>{{ mistakes.reviewResult.durationMs }} ms</span>
      </div>
    </div>

    <div v-if="busy && mistakes.items.length === 0" class="state-box" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>{{ t('mistakes.loading') }}</span>
    </div>
    <div v-else-if="mistakes.status === 'error'" class="error-box" data-tauri-drag-region="false">
      <div class="error-title">
        <IconAlertCircle :size="14" :stroke-width="1.75" />
        {{ t('mistakes.loadFail') }}
      </div>
      <div class="muted">{{ mistakes.error }}</div>
      <button class="action subtle" type="button" @click="reload">{{ t('mistakes.retry') }}</button>
    </div>
    <div v-else-if="mistakes.empty" class="state-box">
      <IconNotebook :size="26" :stroke-width="1.5" />
      <span>{{ t('mistakes.empty') }}</span>
    </div>
    <ul v-else class="mistake-list">
      <li v-for="item in mistakes.items" :key="item.id">
        <MistakeCard
          :source-text="item.sourceText"
          :fragment="item.fragment"
          :correction="item.correction"
          :error-type="item.errorType"
          :severity="item.severity"
          :explanation="item.explanation"
          :status="item.status"
          :created-at="item.createdAt"
          @learn="patchStatus(item.id, 'learned')"
          @archive="patchStatus(item.id, 'archived')"
          @restore="patchStatus(item.id, 'open')"
          @remove="removeItem(item.id)"
        />
      </li>
    </ul>
  </section>
</template>
