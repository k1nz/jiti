/** 翻译方向：这是中英工具。短拉丁词交给引擎自动识别时，DeepL 常误判成 sk 等语言并原样返回。 */

export type LangCode = 'zh' | 'en';
export type SourceChoice = 'auto' | LangCode;

const CJK = /[\u3400-\u4dbf\u4e00-\u9fff]/;
const LATIN = /[A-Za-z]/;

const LABEL_KEYS: Record<string, string> = {
  auto: 'lang.auto',
  zh: 'lang.zh',
  en: 'lang.en',
  ja: 'lang.ja',
  ko: 'lang.ko',
  de: 'lang.de',
  fr: 'lang.fr',
  es: 'lang.es',
  sk: 'lang.sk',
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

export function languageLabelKey(code: string | null | undefined): string | null {
  if (!code) return null;
  const key = code.trim().toLowerCase().split(/[-_]/)[0] ?? code;
  return LABEL_KEYS[key] ?? null;
}

export function languageLabel(
  code: string | null | undefined,
  t: (key: string) => string = (key) => key,
): string {
  const key = languageLabelKey(code);
  if (key) return t(key);
  return code ?? '';
}

export function languagePairLabel(
  from: string | null | undefined,
  to: string | null | undefined,
  t: (key: string) => string = (key) => key,
): string {
  const src = languageLabel(from, t);
  const dst = languageLabel(to, t);
  if (src && dst) return `${src} → ${dst}`;
  return dst;
}
