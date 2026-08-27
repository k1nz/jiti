import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import type { AiReviewResult, Mistake, MistakeList } from '../src/ipc/bindings';
import { useMistakesStore } from '../src/stores/mistakes';

function sampleMistake(id: number, status = 'open'): Mistake {
  return {
    id,
    createdAt: '2026-08-27 01:00:00',
    sourceText: `sentence ${id}`,
    fragment: 'go',
    correction: 'goes',
    errorType: 'grammar',
    severity: 'high',
    explanation: '第三人称',
    correctedSentence: 'He goes.',
    suggestions: ['goes'],
    engine: 'LLM',
    sourceApp: null,
    tags: null,
    status,
    meta: null,
    serverId: null,
    syncedAt: null,
  };
}

function sampleList(items: Mistake[]): MistakeList {
  return { items, total: items.length };
}

function sampleReview(): AiReviewResult {
  return {
    summary: '主谓不一致最常见',
    analyzedCount: 3,
    engine: 'LLM',
    durationMs: 40,
  };
}

describe('mistakes store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('并发加载时过期响应不会覆盖最新列表', () => {
    const store = useMistakesStore();
    const stale = store.beginLoad();
    const live = store.beginLoad();
    store.applyList(stale, sampleList([sampleMistake(1)]));
    expect(store.items).toEqual([]);
    expect(store.status).toBe('loading');
    store.applyList(live, sampleList([sampleMistake(2)]));
    expect(store.items).toHaveLength(1);
    expect(store.items[0].id).toBe(2);
    expect(store.status).toBe('idle');
  });

  it('过期失败不会把最新加载打成错误态', () => {
    const store = useMistakesStore();
    const stale = store.beginLoad();
    const live = store.beginLoad();
    store.failList(stale, '旧错误');
    expect(store.status).toBe('loading');
    expect(store.error).toBeNull();
    store.applyList(live, sampleList([]));
    expect(store.empty).toBe(true);
  });

  it('筛选变更写回 filter，刷新后可落到空态', () => {
    const store = useMistakesStore();
    const first = store.beginLoad();
    store.applyList(first, sampleList([sampleMistake(1)]));
    store.setFilter({ errorType: 'spelling', status: 'learned', timeRange: 'sevenDays' });
    expect(store.filter.errorType).toBe('spelling');
    expect(store.filter.status).toBe('learned');
    expect(store.filter.timeRange).toBe('sevenDays');
    const second = store.beginLoad();
    store.applyList(second, sampleList([]));
    expect(store.empty).toBe(true);
    expect(store.total).toBe(0);
  });

  it('状态变更与删除只改命中项', () => {
    const store = useMistakesStore();
    const id = store.beginLoad();
    store.applyList(id, sampleList([sampleMistake(1), sampleMistake(2)]));
    store.applyMistake({ ...sampleMistake(1, 'learned') });
    expect(store.items[0].status).toBe('learned');
    expect(store.items[1].status).toBe('open');
    store.removeMistake(2);
    expect(store.items).toHaveLength(1);
    expect(store.total).toBe(1);
  });

  it('错误态与空态可区分', () => {
    const store = useMistakesStore();
    expect(store.empty).toBe(true);
    const id = store.beginLoad();
    store.failList(id, 'db down');
    expect(store.status).toBe('error');
    expect(store.empty).toBe(false);
    expect(store.error).toBe('db down');
  });

  it('AI 总结有独立 loading/done/error，过期结果丢弃', () => {
    const store = useMistakesStore();
    const stale = store.beginReview();
    const live = store.beginReview();
    store.applyReview(stale, sampleReview());
    expect(store.reviewStatus).toBe('loading');
    expect(store.reviewResult).toBeNull();
    store.applyReview(live, sampleReview());
    expect(store.reviewStatus).toBe('done');
    expect(store.reviewResult?.analyzedCount).toBe(3);

    const failed = store.beginReview();
    store.failReview(failed, {
      provider: 'ai_review',
      code: 'bad_request',
      message: '当前筛选没有错题',
      hint: null,
      copyable: 'x',
    });
    expect(store.reviewStatus).toBe('error');
    expect(store.reviewError?.code).toBe('bad_request');
  });
});
