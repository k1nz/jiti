export function normalizeAnswer(text: string): string {
  return text.trim().split(/\s+/).filter(Boolean).join(' ').toLowerCase();
}

export function answersMatch(expected: string, got: string): boolean {
  return normalizeAnswer(expected) === normalizeAnswer(got);
}
