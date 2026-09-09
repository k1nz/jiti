import type { HistoryEntry } from './ipc/bindings';

export function formatTime(value: string, locale: string) {
  const date = new Date(value.replace(' ', 'T') + 'Z');
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(locale, { hour12: false });
}

/** Whether a grammar history row represents a check with no reported errors. */
export function isGrammarSuccessHistory(
  entry: Pick<HistoryEntry, 'kind' | 'input' | 'output' | 'meta'>,
): boolean {
  if (entry.kind !== 'grammar') return false;

  if (entry.meta) {
    try {
      const parsed: unknown = JSON.parse(entry.meta);
      if (parsed && typeof parsed === 'object' && 'errors' in parsed) {
        const errors = (parsed as { errors?: unknown }).errors;
        if (Array.isArray(errors)) return errors.length === 0;
      }
    } catch {
      // Fall back to the legacy input/output check for malformed metadata.
    }
  }

  return entry.input === entry.output;
}
