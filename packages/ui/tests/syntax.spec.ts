import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import type { SyntaxResult_Serialize } from '../src/ipc/bindings';
import { useSyntaxStore } from '../src/stores/syntax';

function sampleResult(): SyntaxResult_Serialize {
  return {
    engine: 'LLM',
    input: 'and everything it ships targets Apple',
    chunks: [
      {
        offset: 15,
        length: 8,
        text: 'it ships',
        gloss: '它发布的所有东西',
        notes: ['ship 在科技语境里常指发布、出货'],
      },
    ],
    readings: [
      { text: '并且它发布的所有产品都是针对苹果的。', tone: 'literal' },
      { text: '而且它推出的所有东西，都是冲着苹果来的。', tone: 'natural' },
    ],
    structures: [
      {
        offset: 4,
        length: 19,
        role: 'subject',
        text: 'everything it ships',
        label: '名词性短语，作主语',
        note: '省略了 that',
      },
    ],
    skeletons: [{ role: 'subject', text: 'everything (that) it ships' }],
    tips: ['关系代词 that 被省略了。'],
    durationMs: 12,
  };
}

describe('syntax store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('按模块顺序累加拆解、整句、结构和要点', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    expect(store.status).toBe('loading');
    const sample = sampleResult();
    store.applyProgress(id, { kind: 'started', engine: 'LLM' });
    store.applyProgress(id, { kind: 'chunk', chunk: sample.chunks[0] });
    expect(store.status).toBe('streaming');
    store.applyProgress(id, { kind: 'reading', reading: sample.readings[0] });
    store.applyProgress(id, { kind: 'reading', reading: sample.readings[1] });
    store.applyProgress(id, { kind: 'structure', structure: sample.structures[0] });
    store.applyProgress(id, { kind: 'skeleton', skeleton: sample.skeletons[0] });
    store.applyProgress(id, { kind: 'tip', text: sample.tips[0] });
    expect(store.chunks).toHaveLength(1);
    expect(store.readings.map((item) => item.tone)).toEqual(['literal', 'natural']);
    expect(store.tips).toEqual(['关系代词 that 被省略了。']);
    store.applyProgress(id, { kind: 'finished' });
    store.finish(id, sample);
    expect(store.status).toBe('done');
    expect(store.result?.durationMs).toBe(12);
  });

  it('retrying 会清空已经出现的模块', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    store.applyProgress(id, { kind: 'chunk', chunk: sampleResult().chunks[0] });
    store.applyProgress(id, { kind: 'tip', text: '旧要点' });
    store.applyProgress(id, { kind: 'retrying', reason: '正在重新解析' });
    expect(store.retrying).toBe(true);
    expect(store.chunks).toEqual([]);
    expect(store.readings).toEqual([]);
    expect(store.structures).toEqual([]);
    expect(store.skeletons).toEqual([]);
    expect(store.tips).toEqual([]);
    expect(store.status).toBe('loading');
  });

  it('过期响应不会覆盖最新一次分析', () => {
    const store = useSyntaxStore();
    const stale = store.begin();
    const live = store.begin();
    store.applyProgress(stale, { kind: 'reading', reading: sampleResult().readings[0] });
    expect(store.readings).toEqual([]);
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
    expect(store.readings[0]?.text).toBe('并且它发布的所有产品都是针对苹果的。');
  });

  it('简单句可以没有句型骨架和理解要点', () => {
    const store = useSyntaxStore();
    const id = store.begin();
    store.finish(id, {
      engine: 'LLM',
      input: '我昨天买的书很贵。',
      chunks: [{ text: '书', gloss: '书', notes: [] }],
      readings: [{ text: '我昨天买的书很贵。', tone: 'literal' }],
      structures: [{ role: 'subject', text: '书', label: '主语' }],
      skeletons: [],
      tips: [],
      durationMs: 8,
    });
    expect(store.skeletons).toEqual([]);
    expect(store.tips).toEqual([]);
    expect(store.structures[0]?.note).toBeUndefined();
    expect(store.status).toBe('done');
  });
});
