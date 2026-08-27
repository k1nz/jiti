<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { IconAlertCircle, IconDownload, IconNotebook, IconSparkles } from '@tabler/icons-vue';
import { commands } from '../../ipc/bindings';
import type { EngineErrorPayload, MistakeFilter, MistakeTimeRange } from '../../ipc/bindings';
import { renderReviewMarkdown } from '../../markdown';
import { useMistakesStore } from '../../stores/mistakes';
import MistakeCard from './MistakeCard.vue';

const TYPE_CHIPS: ReadonlyArray<{ value: string | null; label: string }> = [
  { value: null, label: '全部类型' },
  { value: 'grammar', label: '语法' },
  { value: 'spelling', label: '拼写' },
  { value: 'punctuation', label: '标点' },
  { value: 'word_choice', label: '用词' },
  { value: 'style', label: '风格' },
];

const STATUS_CHIPS: ReadonlyArray<{ value: string | null; label: string }> = [
  { value: null, label: '全部状态' },
  { value: 'open', label: '未掌握' },
  { value: 'learned', label: '已掌握' },
  { value: 'archived', label: '已归档' },
];

const TIME_CHIPS: ReadonlyArray<{ value: MistakeTimeRange; label: string }> = [
  { value: 'sevenDays', label: '7 天' },
  { value: 'thirtyDays', label: '30 天' },
  { value: 'all', label: '全部时间' },
];

const mistakes = useMistakesStore();

function unwrap<T>(promise: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: unknown }>) {
  return promise.then((result) => {
    if (result.status === 'ok') return result.data;
    throw result.error;
  });
}

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
      <span>错题本</span>
      <span class="spacer"></span>
      <button
        class="action subtle"
        type="button"
        :disabled="mistakes.exporting || busy"
        @click="exportMarkdown"
      >
        <IconDownload :size="14" :stroke-width="1.75" />
        导出
      </button>
      <button
        class="action"
        type="button"
        :disabled="mistakes.reviewStatus === 'loading' || busy || mistakes.empty"
        @click="runReview"
      >
        <IconSparkles :size="14" :stroke-width="1.75" />
        AI 总结
      </button>
    </div>

    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in TYPE_CHIPS"
        :key="`type-${chip.label}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.errorType ?? null) === chip.value }"
        @click="onType(chip.value)"
      >
        {{ chip.label }}
      </button>
    </div>
    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in STATUS_CHIPS"
        :key="`status-${chip.label}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.status ?? null) === chip.value }"
        @click="onStatus(chip.value)"
      >
        {{ chip.label }}
      </button>
    </div>
    <div class="chip-row" data-tauri-drag-region="false">
      <button
        v-for="chip in TIME_CHIPS"
        :key="`time-${chip.label}`"
        class="chip"
        type="button"
        :class="{ on: (mistakes.filter.timeRange ?? 'all') === chip.value }"
        @click="onTime(chip.value)"
      >
        {{ chip.label }}
      </button>
    </div>

    <div v-if="mistakes.reviewStatus === 'loading'" class="state-box compact" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>正在总结</span>
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
        <span>{{ mistakes.reviewResult.analyzedCount }} 条</span>
        <span>{{ mistakes.reviewResult.engine }}</span>
        <span>{{ mistakes.reviewResult.durationMs }} ms</span>
      </div>
    </div>

    <div v-if="busy && mistakes.items.length === 0" class="state-box" aria-live="polite">
      <span class="spinner" aria-hidden="true"></span>
      <span>正在加载</span>
    </div>
    <div v-else-if="mistakes.status === 'error'" class="error-box" data-tauri-drag-region="false">
      <div class="error-title">
        <IconAlertCircle :size="14" :stroke-width="1.75" />
        加载失败
      </div>
      <div class="muted">{{ mistakes.error }}</div>
      <button class="action subtle" type="button" @click="reload">重试</button>
    </div>
    <div v-else-if="mistakes.empty" class="state-box">
      <IconNotebook :size="26" :stroke-width="1.5" />
      <span>还没有错题，检查一次就有了</span>
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
