import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  EngineErrorPayload,
  SyntaxChunk,
  SyntaxProgressEvent_Deserialize,
  SyntaxReading,
  SyntaxResult_Serialize,
  SyntaxSkeleton,
  SyntaxStructure,
} from '../ipc/bindings';
import { LOADING_SPINNER_DELAY_MS, shouldShowDelayedSpinner } from '../loading';

export type SyntaxStatus = 'idle' | 'loading' | 'streaming' | 'done' | 'error';

export const useSyntaxStore = defineStore('syntax', () => {
  const status = ref<SyntaxStatus>('idle');
  const runId = ref(0);
  const chunks = ref<SyntaxChunk[]>([]);
  const readings = ref<SyntaxReading[]>([]);
  const structures = ref<SyntaxStructure[]>([]);
  const skeletons = ref<SyntaxSkeleton[]>([]);
  const tips = ref<string[]>([]);
  const result = ref<SyntaxResult_Serialize | null>(null);
  const error = ref<EngineErrorPayload | null>(null);
  const retrying = ref(false);
  const spinnerVisible = ref(false);
  let spinnerTimer: ReturnType<typeof setTimeout> | undefined;

  const hasContent = computed(
    () =>
      chunks.value.length > 0 ||
      readings.value.length > 0 ||
      structures.value.length > 0 ||
      skeletons.value.length > 0 ||
      tips.value.length > 0,
  );

  function isLive(id: number) {
    return id === runId.value;
  }

  function clearSpinnerTimer() {
    if (spinnerTimer !== undefined) {
      clearTimeout(spinnerTimer);
      spinnerTimer = undefined;
    }
  }

  function armSpinner(id: number) {
    clearSpinnerTimer();
    spinnerVisible.value = false;
    spinnerTimer = setTimeout(() => {
      if (!isLive(id)) return;
      spinnerVisible.value = shouldShowDelayedSpinner({
        busy: status.value === 'loading' || status.value === 'streaming',
        hasContent: hasContent.value,
        elapsedMs: LOADING_SPINNER_DELAY_MS,
      });
    }, LOADING_SPINNER_DELAY_MS);
  }

  function clearDraft() {
    chunks.value = [];
    readings.value = [];
    structures.value = [];
    skeletons.value = [];
    tips.value = [];
    result.value = null;
  }

  function begin(): number {
    const id = runId.value + 1;
    runId.value = id;
    status.value = 'loading';
    retrying.value = false;
    error.value = null;
    clearDraft();
    armSpinner(id);
    return id;
  }

  function applyProgress(id: number, event: SyntaxProgressEvent_Deserialize) {
    if (!isLive(id)) return;
    switch (event.kind) {
      case 'started':
        status.value = 'loading';
        break;
      case 'chunk':
        status.value = 'streaming';
        chunks.value = [...chunks.value, event.chunk];
        if (hasContent.value) spinnerVisible.value = false;
        break;
      case 'reading':
        status.value = 'streaming';
        readings.value = [...readings.value, event.reading];
        if (hasContent.value) spinnerVisible.value = false;
        break;
      case 'structure':
        status.value = 'streaming';
        structures.value = [...structures.value, event.structure];
        if (hasContent.value) spinnerVisible.value = false;
        break;
      case 'skeleton':
        status.value = 'streaming';
        skeletons.value = [...skeletons.value, event.skeleton];
        if (hasContent.value) spinnerVisible.value = false;
        break;
      case 'tip':
        status.value = 'streaming';
        tips.value = [...tips.value, event.text];
        if (hasContent.value) spinnerVisible.value = false;
        break;
      case 'retrying':
        retrying.value = true;
        clearDraft();
        status.value = 'loading';
        armSpinner(id);
        break;
      case 'finished':
        retrying.value = false;
        spinnerVisible.value = false;
        clearSpinnerTimer();
        if (status.value !== 'error') status.value = 'done';
        break;
    }
  }

  function finish(id: number, finalResult: SyntaxResult_Serialize) {
    if (!isLive(id)) return;
    result.value = finalResult;
    chunks.value = finalResult.chunks;
    readings.value = finalResult.readings;
    structures.value = finalResult.structures;
    skeletons.value = finalResult.skeletons;
    tips.value = finalResult.tips;
    retrying.value = false;
    spinnerVisible.value = false;
    clearSpinnerTimer();
    status.value = 'done';
  }

  function fail(id: number, payload: EngineErrorPayload) {
    if (!isLive(id)) return;
    error.value = payload;
    retrying.value = false;
    spinnerVisible.value = false;
    clearSpinnerTimer();
    status.value = 'error';
  }

  function reset() {
    begin();
    status.value = 'idle';
    spinnerVisible.value = false;
    clearSpinnerTimer();
  }

  return {
    status,
    runId,
    chunks,
    readings,
    structures,
    skeletons,
    tips,
    result,
    error,
    retrying,
    hasContent,
    spinnerVisible,
    begin,
    applyProgress,
    finish,
    fail,
    reset,
    isLive,
  };
});
