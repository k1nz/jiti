import { describe, expect, it } from 'vitest';
import {
  guessSource,
  guessTarget,
  languageLabel,
  languagePairLabel,
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
    expect(languageLabel('sk')).toBe('斯洛伐克语');
    expect(languageLabel('EN')).toBe('英语');
    expect(languageLabel('zh-Hans')).toBe('中文');
    expect(languagePairLabel('en', 'zh')).toBe('英语 → 中文');
  });
});
