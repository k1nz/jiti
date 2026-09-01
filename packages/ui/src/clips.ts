import { guessSource, hasCjk, hasLatin } from './language';

export type ClipPair = { text: string; note: string | null };
export type ClipDisplay = { en: string; zh: string };

export function collapseClipText(text: string): string {
  return text.trim().split(/\s+/).filter(Boolean).join(' ');
}

export function normalizeClipText(text: string): string {
  return collapseClipText(text).toLowerCase();
}

/** 英语进 text（默写对象），中文释义进 note。写反会互换。 */
export function pairClip(text: string, note?: string | null): ClipPair {
  const left = collapseClipText(text);
  const right = collapseClipText(note ?? '');
  if (!right || normalizeClipText(right) === normalizeClipText(left)) {
    return { text: left, note: null };
  }
  const leftSrc = guessSource(left);
  const rightSrc = guessSource(right);
  if (leftSrc === 'zh' && rightSrc === 'en') return { text: right, note: left };
  if (leftSrc === 'en' && rightSrc === 'zh') return { text: left, note: right };
  if (hasCjk(left) && hasLatin(right) && !hasCjk(right)) return { text: right, note: left };
  if (hasLatin(left) && hasCjk(right)) return { text: left, note: right };
  return { text: left, note: right };
}

/** 词库卡片：拉丁行 + 汉字行，兼容旧数据把中文写在 text 里。 */
export function displayClip(text: string, note?: string | null): ClipDisplay {
  const paired = pairClip(text, note);
  const primary = guessSource(paired.text);
  if (primary === 'zh') {
    const gloss = paired.note ?? '';
    return {
      en: guessSource(gloss) === 'en' ? gloss : '',
      zh: paired.text,
    };
  }
  return { en: paired.text, zh: paired.note ?? '' };
}

export function isClipMatch(
  haystack: Set<string>,
  ...parts: Array<string | null | undefined>
): boolean {
  return parts.some((part) => {
    const n = normalizeClipText(part ?? '');
    return n.length > 0 && haystack.has(n);
  });
}

export function glossFromEnrichment(
  enrichment?: {
    mnemonicZh?: string | null;
    senses?: Array<{ translation?: string | null }>;
  } | null,
): string {
  const fromSense = enrichment?.senses
    ?.map((sense) => sense.translation?.trim())
    .find(Boolean);
  if (fromSense) return fromSense;
  return enrichment?.mnemonicZh?.trim() || '';
}

/** 默写提示：有中文释义就出中文；否则只露英语首字母。 */
export function maskDictation(text: string): string {
  return collapseClipText(text)
    .split(/\s+/)
    .filter(Boolean)
    .map((word) =>
      [...word]
        .map((ch, i) => (i === 0 || !/[A-Za-z]/.test(ch) ? ch : '_'))
        .join(''),
    )
    .join(' ');
}

export function dictationCue(text: string, note?: string | null): string {
  const { en, zh } = displayClip(text, note);
  if (zh) return zh;
  if (en) return maskDictation(en);
  return maskDictation(text);
}
