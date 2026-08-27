import { describe, expect, it } from 'vitest';
import { applyTheme, resolveLocale, themeDatasetValue } from '../src/appearance';
import enUS from '../src/i18n/en-US.json';
import zhCN from '../src/i18n/zh-CN.json';

function leafKeys(value: unknown, prefix = ''): string[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return prefix ? [prefix] : [];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    leafKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe('resolveLocale', () => {
  it('手动选择优先生效', () => {
    expect(resolveLocale('en-US', 'zh-CN')).toBe('en-US');
    expect(resolveLocale('zh-CN', 'en-US')).toBe('zh-CN');
  });

  it('跟随系统时按语言前缀解析到 zh-CN 或 en-US', () => {
    expect(resolveLocale('system', 'zh-Hans-CN')).toBe('zh-CN');
    expect(resolveLocale('system', 'en-GB')).toBe('en-US');
    expect(resolveLocale('system', 'ja-JP')).toBe('en-US');
  });
});

describe('themeDatasetValue', () => {
  it('system 不写 data-theme，浅/深写入对应值', () => {
    expect(themeDatasetValue('system')).toBeNull();
    expect(themeDatasetValue('light')).toBe('light');
    expect(themeDatasetValue('dark')).toBe('dark');
  });
});

describe('applyTheme', () => {
  it('在无 document 的 node 环境是空操作', () => {
    expect(() => applyTheme('dark')).not.toThrow();
  });
});

describe('i18n catalogs', () => {
  it('中英文消息键严格同构', () => {
    expect(leafKeys(enUS).sort()).toEqual(leafKeys(zhCN).sort());
  });
});
