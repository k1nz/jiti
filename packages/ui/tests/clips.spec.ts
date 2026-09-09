import { describe, expect, it } from 'vitest';
import {
  dictationCue,
  displayClip,
  glossFromEnrichment,
  isClipMatch,
  maskDictation,
  normalizeClipText,
  pairClip,
} from '../src/clips';

describe('pairClip', () => {
  it('把英语放进 text、中文放进 note', () => {
    expect(pairClip('simply is', '简直是')).toEqual({
      text: 'simply is',
      note: '简直是',
    });
    expect(pairClip('简直是', 'simply is')).toEqual({
      text: 'simply is',
      note: '简直是',
    });
  });

  it('只有一侧时不捏造另一侧', () => {
    expect(pairClip('简直是')).toEqual({ text: '简直是', note: null });
    expect(pairClip('on the other hand', '  ')).toEqual({
      text: 'on the other hand',
      note: null,
    });
  });
});

describe('displayClip', () => {
  it('词库卡片先英语后中文', () => {
    expect(displayClip('simply is', '简直是')).toEqual({
      en: 'simply is',
      zh: '简直是',
    });
  });

  it('旧数据把中文写在 text 里时仍显示为释义', () => {
    expect(displayClip('简直是')).toEqual({ en: '', zh: '简直是' });
    expect(displayClip('简直是', 'simply is')).toEqual({
      en: 'simply is',
      zh: '简直是',
    });
  });
});

describe('clip marks', () => {
  it('用规范化文本判断是否已收藏', () => {
    const marks = new Set([normalizeClipText('Simply  is'), normalizeClipText('简直是')]);
    expect(isClipMatch(marks, 'simply is')).toBe(true);
    expect(isClipMatch(marks, '  简直是 ')).toBe(true);
    expect(isClipMatch(marks, 'other')).toBe(false);
  });

  it('词卡取第一条中文义项', () => {
    expect(
      glossFromEnrichment({
        mnemonicZh: '备用',
        senses: [{ translation: '  简直是  ' }],
      }),
    ).toBe('简直是');
  });
});

describe('dictationCue', () => {
  it('默写时用中文释义当提示，不泄露英语答案', () => {
    expect(dictationCue('simply is', '简直是')).toBe('简直是');
    expect(dictationCue('简直是', 'simply is')).toBe('简直是');
  });

  it('没有释义时只露出首字母', () => {
    expect(maskDictation('on the other hand')).toBe('o_ t__ o____ h___');
    expect(dictationCue('on the other hand')).toBe('o_ t__ o____ h___');
  });
});
