/** 列表打字跳转（ship-readiness 30）。 */

export function typeaheadIndex(labels: readonly string[], query: string, from = 0): number {
  const q = query.trim().toLowerCase();
  if (!q || labels.length === 0) return from;
  const n = labels.length;
  const start = ((from % n) + n) % n;
  for (let i = 0; i < n; i++) {
    const idx = (start + i) % n;
    if (labels[idx].toLowerCase().startsWith(q)) return idx;
  }
  return from;
}

export class TypeaheadBuffer {
  buf = '';
  last = 0;

  push(ch: string, now = Date.now(), gap = 800): string {
    this.buf = now - this.last > gap ? ch : this.buf + ch;
    this.last = now;
    return this.buf;
  }
}
