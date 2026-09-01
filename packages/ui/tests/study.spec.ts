import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { answersMatch, normalizeAnswer } from '../src/study/answer';

const uiRoot = fileURLToPath(new URL('..', import.meta.url));

function walk(dir: string, files: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, files);
    else files.push(full);
  }
  return files;
}

describe('study answer matching', () => {
  it('ignores case and extra spaces', () => {
    expect(normalizeAnswer('  On  The  Other  Hand ')).toBe('on the other hand');
    expect(answersMatch('goes', 'Goes')).toBe(true);
    expect(answersMatch('goes', 'go')).toBe(false);
  });
});

describe('study window isolation', () => {
  it('vite has a separate study entry next to settings', () => {
    const cfg = readFileSync(join(uiRoot, 'vite.config.ts'), 'utf8');
    expect(cfg).toContain('study.html');
    expect(cfg).toContain('settings.html');
    expect(cfg).toContain('index.html');
  });

  it('panel sources never import study/', () => {
    const panelDir = join(uiRoot, 'src/panel');
    const hits = walk(panelDir).filter((file) => {
      if (!/\.(ts|vue)$/.test(file)) return false;
      const text = readFileSync(file, 'utf8');
      return text.includes("from '../study") || text.includes("from '../../study") || text.includes("from './study");
    });
    expect(hits.map((file) => relative(uiRoot, file))).toEqual([]);
  });
});
