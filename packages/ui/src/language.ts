/** 翻译方向：这是中英工具。短拉丁词交给引擎自动识别时，DeepL 常误判成 sk 等语言并原样返回。 */

export type LangCode = 'zh' | 'en';
export type SourceChoice = 'auto' | LangCode;

const CJK = /[\u3400-\u4dbf\u4e00-\u9fff]/;
const LATIN = /[A-Za-z]/;

const LABELS: Record<string, string> = {
  auto: '自动',
  zh: '中文',
  en: '英语',
  ja: '日语',
  ko: '韩语',
  de: '德语',
  fr: '法语',
  es: '西班牙语',
  sk: '斯洛伐克语',
};

export function hasCjk(text: string): boolean {
  return CJK.test(text);
}

export function hasLatin(text: string): boolean {
  return LATIN.test(text);
}

/** 纯汉字 → 中文；纯拉丁字母 → 英语；中英混排交给引擎。 */
export function guessSource(text: string): LangCode | null {
  const cjk = hasCjk(text);
  const latin = hasLatin(text);
  if (cjk && !latin) return 'zh';
  if (latin && !cjk) return 'en';
  return null;
}

export function guessTarget(text: string): LangCode {
  return hasCjk(text) ? 'en' : 'zh';
}

export function resolveSource(choice: SourceChoice, text: string): string | null {
  if (choice !== 'auto') return choice;
  return guessSource(text);
}

export function languageLabel(code: string | null | undefined): string {
  if (!code) return '';
  const key = code.trim().toLowerCase().split(/[-_]/)[0] ?? code;
  return LABELS[key] ?? code;
}

export function languagePairLabel(
  from: string | null | undefined,
  to: string | null | undefined,
): string {
  const src = languageLabel(from);
  const dst = languageLabel(to);
  if (src && dst) return `${src} → ${dst}`;
  return dst;
}
