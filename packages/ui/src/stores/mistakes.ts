import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  AiReviewResult,
  EngineErrorPayload,
  Mistake,
  MistakeFilter,
  MistakeList,
  MistakePreferences,
} from '../ipc/bindings';

export type MistakesStatus = 'idle' | 'loading' | 'error';
export type ReviewStatus = 'idle' | 'loading' | 'done' | 'error';

export const DEFAULT_MISTAKE_PREFS: MistakePreferences = {
  autoCollect: true,
  defaultStatus: 'open',
};

export const DEFAULT_MISTAKE_FILTER: MistakeFilter = {
  errorType: null,
  status: null,
  timeRange: 'all',
  limit: null,
  offset: null,
};

export const useMistakesStore = defineStore('mistakes', () => {
  const status = ref<MistakesStatus>('idle');
  const listId = ref(0);
  const reviewId = ref(0);
  const items = ref<Mistake[]>([]);
  const total = ref(0);
  const error = ref<string | null>(null);
  const filter = ref<MistakeFilter>({ ...DEFAULT_MISTAKE_FILTER });
  const preferences = ref<MistakePreferences>({ ...DEFAULT_MISTAKE_PREFS });
  const reviewStatus = ref<ReviewStatus>('idle');
  const reviewResult = ref<AiReviewResult | null>(null);
  const reviewError = ref<EngineErrorPayload | null>(null);
  const exporting = ref(false);

  const empty = computed(
    () => status.value !== 'loading' && status.value !== 'error' && items.value.length === 0,
  );

  function isLiveList(id: number) {
    return id === listId.value;
  }

  function isLiveReview(id: number) {
    return id === reviewId.value;
  }

  function beginLoad(): number {
    const id = listId.value + 1;
    listId.value = id;
    status.value = 'loading';
    error.value = null;
    return id;
  }

  function applyList(id: number, list: MistakeList) {
    if (!isLiveList(id)) return;
    items.value = list.items;
    total.value = list.total;
    status.value = 'idle';
  }

  function failList(id: number, message: string) {
    if (!isLiveList(id)) return;
    error.value = message;
    status.value = 'error';
  }

  function setFilter(next: Partial<MistakeFilter>) {
    filter.value = { ...filter.value, ...next };
  }

  function applyMistake(updated: Mistake) {
    items.value = items.value.map((item) => (item.id === updated.id ? updated : item));
  }

  function removeMistake(id: number) {
    items.value = items.value.filter((item) => item.id !== id);
    total.value = Math.max(0, total.value - 1);
  }

  function setPreferences(prefs: MistakePreferences) {
    preferences.value = prefs;
  }

  function beginReview(): number {
    const id = reviewId.value + 1;
    reviewId.value = id;
    reviewStatus.value = 'loading';
    reviewError.value = null;
    return id;
  }

  function applyReview(id: number, result: AiReviewResult) {
    if (!isLiveReview(id)) return;
    reviewResult.value = result;
    reviewStatus.value = 'done';
  }

  function failReview(id: number, payload: EngineErrorPayload) {
    if (!isLiveReview(id)) return;
    reviewError.value = payload;
    reviewStatus.value = 'error';
  }

  function clearReview() {
    reviewId.value += 1;
    reviewStatus.value = 'idle';
    reviewResult.value = null;
    reviewError.value = null;
  }

  function setExporting(value: boolean) {
    exporting.value = value;
  }

  return {
    status,
    listId,
    reviewId,
    items,
    total,
    error,
    filter,
    preferences,
    reviewStatus,
    reviewResult,
    reviewError,
    exporting,
    empty,
    beginLoad,
    applyList,
    failList,
    setFilter,
    applyMistake,
    removeMistake,
    setPreferences,
    beginReview,
    applyReview,
    failReview,
    clearReview,
    setExporting,
    isLiveList,
    isLiveReview,
  };
});
