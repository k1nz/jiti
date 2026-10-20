import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import type { SyntaxResult_Serialize } from '../src/ipc/bindings';
import { useSyntaxStore } from '../src/stores/syntax';

function sampleResult(): SyntaxResult_Serialize {
  return {
    engine: 'LLM',
    input: 'The book that I bought was expensive.',
    summary: '含定语从句的过去时陈述句',
    profile: {
      language: 'en',
      sentenceType: 'declarative',
      tense: 'past',
      aspect: 'simple',
      voice: 'active',
    },
    constituents: [
      {
        offset: 0,
        length: 8,
        role: 'subject',
        text: 'The book',
        note: '主语',
      },
    ],
    clauses: [
      {
        offset: 9,
        length: 13,
        kind: 'relative',
        text: 'that I bought',
        note: '修饰 book',
      },
    ],
    durationMs: 12,
  };
}

describe('syntax store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('从 idle 进入 loading，再随语义事件变成 streaming 并累加卡片', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    expect(store.status).toBe('loading');
    store.applyProgress(id, { kind: 'started', engine: 'LLM' });
    store.applyProgress(id, { kind: 'summary', text: '含定语从句的过去时陈述句' });
    expect(store.status).toBe('streaming');
    expect(store.summary).toBe('含定语从句的过去时陈述句');
    store.applyProgress(id, {
      kind: 'profile',
      profile: {
        language: 'en',
        sentenceType: 'declarative',
        tense: 'past',
        aspect: 'simple',
        voice: 'active',
      },
    });
    store.applyProgress(id, {
      kind: 'constituent',
      constituent: sampleResult().constituents[0],
    });
    store.applyProgress(id, { kind: 'clause', clause: sampleResult().clauses[0] });
    expect(store.constituents).toHaveLength(1);
    expect(store.clauses).toHaveLength(1);
    store.applyProgress(id, { kind: 'finished' });
    store.finish(id, sampleResult());
    expect(store.status).toBe('done');
    expect(store.profile?.tense).toBe('past');
    expect(store.result?.durationMs).toBe(12);
  });

  it('retrying 会清空总评、句型和卡片', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    store.applyProgress(id, { kind: 'summary', text: '旧总评' });
    store.applyProgress(id, {
      kind: 'constituent',
      constituent: sampleResult().constituents[0],
    });
    store.applyProgress(id, { kind: 'retrying', reason: '正在重新解析' });
    expect(store.retrying).toBe(true);
    expect(store.summary).toBeNull();
    expect(store.profile).toBeNull();
    expect(store.constituents).toEqual([]);
    expect(store.clauses).toEqual([]);
    expect(store.status).toBe('loading');
  });

  it('过期响应不会覆盖最新一次分析', () => {
    const store = useSyntaxStore();
    const stale = store.begin();
    const live = store.begin();
    store.applyProgress(stale, { kind: 'summary', text: '过期' });
    expect(store.summary).toBeNull();
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
    expect(store.summary).toBe('含定语从句的过去时陈述句');
  });

  it('中文句型可以没有时态和语态', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    store.finish(id, {
      engine: 'LLM',
      input: '我昨天买的书很贵。',
      summary: '带定语的陈述句',
      profile: {
        language: 'zh',
        sentenceType: 'declarative',
      },
      constituents: [],
      clauses: [],
      durationMs: 8,
    });
    expect(store.profile?.language).toBe('zh');
    expect(store.profile?.tense).toBeUndefined();
    expect(store.profile?.voice).toBeUndefined();
    expect(store.status).toBe('done');
  });
});
