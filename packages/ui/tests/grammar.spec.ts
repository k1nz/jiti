import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { useGrammarStore } from '../src/stores/grammar';
import type { GrammarError, GrammarResult_Serialize } from '../src/ipc/bindings';

function sampleError(fragment = 'go'): GrammarError {
  return {
    offset: 3,
    length: 2,
    fragment,
    correction: 'goes',
    type: 'grammar',
    severity: 'high',
    explanation: '第三人称单数',
    suggestions: ['goes'],
  };
}

function sampleResult(): GrammarResult_Serialize {
  return {
    engine: 'LLM',
    input: 'He go to school.',
    errors: [sampleError()],
    correctedText: 'He goes to school.',
    overall: '主谓不一致',
    durationMs: 12,
  };
}

describe('grammar store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('从 idle 进入 loading，再随语义事件变成 streaming 并累加卡片', () => {
    const store = useGrammarStore();
    const id = store.begin();
    expect(store.status).toBe('loading');
    store.applyProgress(id, { kind: 'started', engine: 'LLM' });
    store.applyProgress(id, { kind: 'overall', text: '主谓不一致' });
    expect(store.status).toBe('streaming');
    expect(store.overall).toBe('主谓不一致');
    store.applyProgress(id, { kind: 'correctedText', text: 'He goes to school.' });
    store.applyProgress(id, { kind: 'error', error: sampleError() });
    expect(store.errors).toHaveLength(1);
    store.applyProgress(id, { kind: 'finished' });
    store.finish(id, sampleResult());
    expect(store.status).toBe('done');
    expect(store.result?.correctedText).toBe('He goes to school.');
  });

  it('retrying 会清空临时总评、改写和卡片', () => {
    const store = useGrammarStore();
    const id = store.begin();
    store.applyProgress(id, { kind: 'overall', text: '旧总评' });
    store.applyProgress(id, { kind: 'error', error: sampleError() });
    store.applyProgress(id, { kind: 'retrying', reason: '正在重新解析' });
    expect(store.retrying).toBe(true);
    expect(store.overall).toBeNull();
    expect(store.errors).toEqual([]);
    expect(store.status).toBe('loading');
  });

  it('过期响应不会覆盖最新一次检查', () => {
    const store = useGrammarStore();
    const stale = store.begin();
    const live = store.begin();
    store.applyProgress(stale, { kind: 'overall', text: '过期' });
    expect(store.overall).toBeNull();
    store.fail(stale, {
      provider: 'LLM',
      code: 'network',
      message: '过期失败',
      hint: null,
      copyable: 'x',
    });
    expect(store.status).toBe('loading');
    store.finish(live, sampleResult());
    expect(store.status).toBe('done');
    expect(store.overall).toBe('主谓不一致');
  });

  it('无错误成功态与错误态可区分', () => {
    const store = useGrammarStore();
    const ok = store.begin();
    store.finish(ok, {
      engine: 'LLM',
      input: 'Hello.',
      errors: [],
      correctedText: 'Hello.',
      overall: '没有问题',
      durationMs: 8,
    });
    expect(store.status).toBe('done');
    expect(store.errors).toHaveLength(0);

    const bad = store.begin();
    store.fail(bad, {
      provider: 'LLM',
      code: 'missing_key',
      message: '未配置 LLM API Key',
      hint: '到设置粘贴 Key',
      copyable: '[jiti] missing_key',
    });
    expect(store.status).toBe('error');
    expect(store.error?.code).toBe('missing_key');
  });
});
