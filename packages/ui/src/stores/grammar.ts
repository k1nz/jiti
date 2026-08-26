import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  EngineErrorPayload,
  GrammarError,
  GrammarProgressEvent_Deserialize,
  GrammarResult_Serialize,
} from '../ipc/bindings';

export type GrammarStatus = 'idle' | 'loading' | 'streaming' | 'done' | 'error';

export const useGrammarStore = defineStore('grammar', () => {
  const status = ref<GrammarStatus>('idle');
  const runId = ref(0);
  const overall = ref<string | null>(null);
  const correctedText = ref<string | null>(null);
  const errors = ref<GrammarError[]>([]);
  const result = ref<GrammarResult_Serialize | null>(null);
  const error = ref<EngineErrorPayload | null>(null);
  const retrying = ref(false);

  const hasContent = computed(
    () => overall.value !== null || correctedText.value !== null || errors.value.length > 0,
  );

  function isLive(id: number) {
    return id === runId.value;
  }

  function begin(): number {
    const id = runId.value + 1;
    runId.value = id;
    status.value = 'loading';
    retrying.value = false;
    overall.value = null;
    correctedText.value = null;
    errors.value = [];
    result.value = null;
    error.value = null;
    return id;
  }

  function applyProgress(id: number, event: GrammarProgressEvent_Deserialize) {
    if (!isLive(id)) return;
    switch (event.kind) {
      case 'started':
        status.value = 'loading';
        break;
      case 'overall':
        status.value = 'streaming';
        overall.value = event.text;
        break;
      case 'correctedText':
        status.value = 'streaming';
        correctedText.value = event.text;
        break;
      case 'error':
        status.value = 'streaming';
        errors.value = [...errors.value, event.error];
        break;
      case 'retrying':
        retrying.value = true;
        overall.value = null;
        correctedText.value = null;
        errors.value = [];
        result.value = null;
        status.value = 'loading';
        break;
      case 'finished':
        retrying.value = false;
        if (status.value !== 'error') status.value = 'done';
        break;
    }
  }

  function finish(id: number, finalResult: GrammarResult_Serialize) {
    if (!isLive(id)) return;
    result.value = finalResult;
    overall.value = finalResult.overall ?? overall.value;
    correctedText.value = finalResult.correctedText ?? correctedText.value;
    errors.value = finalResult.errors;
    retrying.value = false;
    status.value = 'done';
  }

  function fail(id: number, payload: EngineErrorPayload) {
    if (!isLive(id)) return;
    error.value = payload;
    retrying.value = false;
    status.value = 'error';
  }

  function reset() {
    begin();
    status.value = 'idle';
  }

  return {
    status,
    runId,
    overall,
    correctedText,
    errors,
    result,
    error,
    retrying,
    hasContent,
    begin,
    applyProgress,
    finish,
    fail,
    reset,
    isLive,
  };
});
