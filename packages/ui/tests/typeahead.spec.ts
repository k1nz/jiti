import { describe, expect, it } from 'vitest';
import { TypeaheadBuffer, typeaheadIndex } from '../src/typeahead';

describe('typeahead', () => {
  const labels = ['Apple pie', 'banana', 'apricot'];

  it('跳到以当前缓冲为前缀的下一项', () => {
    expect(typeaheadIndex(labels, 'a', 0)).toBe(0);
    expect(typeaheadIndex(labels, 'ap', 1)).toBe(2);
    expect(typeaheadIndex(labels, 'b', 0)).toBe(1);
  });

  it('超时后重新开始缓冲', () => {
    const buf = new TypeaheadBuffer();
    expect(buf.push('a', 0)).toBe('a');
    expect(buf.push('p', 100)).toBe('ap');
    expect(buf.push('b', 1000)).toBe('b');
  });
});
