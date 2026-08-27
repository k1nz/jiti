import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  EngineErrorPayload,
  GrammarCheckOutcome_Serialize,
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
  const mistakeIds = ref<(number | null)[]>([]);

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
    mistakeIds.value = [];
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
        mistakeIds.value = [];
        status.value = 'loading';
        break;
      case 'finished':
        retrying.value = false;
        if (status.value !== 'error') status.value = 'done';
        break;
    }
  }

  function finish(id: number, outcome: GrammarCheckOutcome_Serialize | GrammarResult_Serialize) {
    if (!isLive(id)) return;
    const finalResult = 'result' in outcome ? outcome.result : outcome;
    const ids = 'mistakeIds' in outcome ? outcome.mistakeIds : [];
    result.value = finalResult;
    overall.value = finalResult.overall ?? overall.value;
    correctedText.value = finalResult.correctedText ?? correctedText.value;
    errors.value = finalResult.errors;
    mistakeIds.value = alignMistakeIds(ids, finalResult.errors.length);
    retrying.value = false;
    status.value = 'done';
  }

  function setMistakeId(index: number, id: number | null) {
    const next = [...mistakeIds.value];
    while (next.length <= index) next.push(null);
    next[index] = id;
    mistakeIds.value = next;
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
    mistakeIds,
    hasContent,
    begin,
    applyProgress,
    finish,
    fail,
    reset,
    isLive,
    setMistakeId,
  };
});

export function alignMistakeIds(ids: (number | null)[] | undefined, n: number): (number | null)[] {
  return Array.from({ length: n }, (_, i) => ids?.[i] ?? null);
}
