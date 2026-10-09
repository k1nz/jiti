import { describe, expect, it } from 'vitest';
import {
  guessSource,
  guessTarget,
  languageLabel,
  languagePairLabel,
  oppositeLanguage,
  resolveSource,
} from '../src/language';

describe('guessSource', () => {
  it('把纯拉丁短词当成英语，而不是交给引擎乱猜', () => {
    expect(guessSource('Epoch')).toBe('en');
    expect(guessSource('Hello, world')).toBe('en');
  });

  it('把纯汉字当成中文', () => {
    expect(guessSource('你好')).toBe('zh');
    expect(guessSource('纪元')).toBe('zh');
  });

  it('中英混排不臆测源语言', () => {
    expect(guessSource('Hello 世界')).toBeNull();
  });
});

describe('guessTarget / resolveSource', () => {
  it('选中英语时默认译成中文', () => {
    expect(guessTarget('Epoch')).toBe('zh');
    expect(resolveSource('auto', 'Epoch')).toBe('en');
  });

  it('选中中文时默认译成英语', () => {
    expect(guessTarget('你好')).toBe('en');
    expect(resolveSource('auto', '你好')).toBe('zh');
  });

  it('手动指定源语言时不再猜测', () => {
    expect(resolveSource('en', '你好')).toBe('en');
    expect(resolveSource('zh', 'Epoch')).toBe('zh');
  });
});

describe('languageLabel', () => {
  it('把引擎返回的语言码显示成人话', () => {
    expect(languageLabel('sk')).toBe('lang.sk');
    expect(languageLabel('EN')).toBe('lang.en');
    expect(languageLabel('zh-Hans')).toBe('lang.zh');
    expect(languagePairLabel('en', 'zh')).toBe('lang.en → lang.zh');
  });
});

describe('oppositeLanguage', () => {
  it('中文对应英文', () => {
    expect(oppositeLanguage('zh')).toBe('en');
  });

  it('英文对应中文', () => {
    expect(oppositeLanguage('en')).toBe('zh');
  });
});

describe('自动目标语言切换', () => {
  it('纯中文文本应该目标为英文', () => {
    expect(guessTarget('你好世界')).toBe('en');
  });

  it('纯英文文本应该目标为中文', () => {
    expect(guessTarget('Hello World')).toBe('zh');
  });

  it('混合中英文本应该目标为英文', () => {
    expect(guessTarget('Hello 世界')).toBe('en');
    expect(guessTarget('你好 World')).toBe('en');
  });

  it('明确选择中文源时应该目标为英文', () => {
    const source = 'zh';
    expect(oppositeLanguage(source)).toBe('en');
  });

  it('明确选择英文源时应该目标为中文', () => {
    const source = 'en';
    expect(oppositeLanguage(source)).toBe('zh');
  });
});
