/** 将 AI 复习摘要中的 Markdown 转成安全 HTML（只产出白名单标签）。 */

const PLACEHOLDER = '\u0000';

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function renderInline(text: string): string {
  const codes: string[] = [];
  const withCodes = text.replace(/`([^`]+)`/g, (_match, code: string) => {
    codes.push(`<code>${escapeHtml(code)}</code>`);
    return `${PLACEHOLDER}${codes.length - 1}${PLACEHOLDER}`;
  });
  let html = escapeHtml(withCodes);
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/__(.+?)__/g, '<strong>$1</strong>');
  html = html.replace(/(^|[^*])\*(?!\s)(.+?)(?<!\s)\*(?!\*)/g, '$1<em>$2</em>');
  html = html.replace(/(^|[^_])_(?!\s)(.+?)(?<!\s)_(?!_)/g, '$1<em>$2</em>');
  html = html.replace(/\u0000(\d+)\u0000/g, (_match, index: string) => codes[Number(index)] ?? '');
  return html;
}

function heading(line: string): string | null {
  const match = /^(#{1,3})\s+(.+)$/.exec(line);
  if (!match) return null;
  const level = match[1].length;
  return `<h${level}>${renderInline(match[2].trim())}</h${level}>`;
}

function bullet(line: string): string | null {
  const match = /^[-*+]\s+(.+)$/.exec(line);
  if (!match) return null;
  return `<li>${renderInline(match[1])}</li>`;
}

function ordered(line: string): string | null {
  const match = /^\d+[.)]\s+(.+)$/.exec(line);
  if (!match) return null;
  return `<li>${renderInline(match[1])}</li>`;
}

function flushParagraph(lines: string[], out: string[]) {
  if (lines.length === 0) return;
  out.push(`<p>${lines.map(renderInline).join('<br>')}</p>`);
  lines.length = 0;
}

function flushList(kind: 'ul' | 'ol' | null, items: string[], out: string[]): null {
  if (kind && items.length) {
    out.push(`<${kind}>${items.join('')}</${kind}>`);
    items.length = 0;
  }
  return null;
}

export function renderReviewMarkdown(source: string): string {
  const lines = source.replace(/\r\n/g, '\n').trimEnd().split('\n');
  const out: string[] = [];
  const paragraph: string[] = [];
  const listItems: string[] = [];
  let listKind: 'ul' | 'ol' | null = null;

  for (const raw of lines) {
    const line = raw.trimEnd();
    const trimmed = line.trim();
    if (!trimmed) {
      listKind = flushList(listKind, listItems, out);
      flushParagraph(paragraph, out);
      continue;
    }

    const head = heading(trimmed);
    if (head) {
      listKind = flushList(listKind, listItems, out);
      flushParagraph(paragraph, out);
      out.push(head);
      continue;
    }

    const li = bullet(trimmed);
    if (li) {
      flushParagraph(paragraph, out);
      if (listKind === 'ol') listKind = flushList(listKind, listItems, out);
      listKind = 'ul';
      listItems.push(li);
      continue;
    }

    const numbered = ordered(trimmed);
    if (numbered) {
      flushParagraph(paragraph, out);
      if (listKind === 'ul') listKind = flushList(listKind, listItems, out);
      listKind = 'ol';
      listItems.push(numbered);
      continue;
    }

    listKind = flushList(listKind, listItems, out);
    paragraph.push(trimmed);
  }

  flushList(listKind, listItems, out);
  flushParagraph(paragraph, out);
  return out.join('');
}
