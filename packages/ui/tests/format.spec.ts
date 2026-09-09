import { describe, expect, it } from 'vitest';
import { isGrammarSuccessHistory } from '../src/format';

function entry(overrides: Partial<Parameters<typeof isGrammarSuccessHistory>[0]> = {}) {
  return {
    kind: 'grammar',
    input: 'Hello.',
    output: 'Hello.',
    meta: null,
    ...overrides,
  };
}

describe('isGrammarSuccessHistory', () => {
  it('recognizes grammar results with no errors from metadata', () => {
    expect(isGrammarSuccessHistory(entry({ output: 'Hello rewritten.', meta: '{"errors":[]}' }))).toBe(true);
  });

  it('keeps grammar corrections visible when metadata contains errors', () => {
    expect(isGrammarSuccessHistory(entry({ meta: '{"errors":[{"fragment":"go"}]}' }))).toBe(false);
  });

  it('supports legacy rows without metadata', () => {
    expect(isGrammarSuccessHistory(entry())).toBe(true);
    expect(isGrammarSuccessHistory(entry({ output: 'Hello, world.' }))).toBe(false);
  });

  it('does not classify translation rows as grammar success', () => {
    expect(isGrammarSuccessHistory(entry({ kind: 'translate' }))).toBe(false);
  });
});
