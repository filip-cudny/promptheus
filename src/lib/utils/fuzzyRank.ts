export interface FuzzyMatch {
  score: number;
  indices: number[];
}

const SUBSTRING_SCORE = 100;
const WORD_START_BONUS_SUBSTRING = 10;
const SCATTERED_SCORE_CAP = SUBSTRING_SCORE - 1;
const HIT_SCORE = 1;
const ADJACENT_BONUS = 3;
const WORD_START_BONUS_SCATTERED = 2;

const WORD_CHAR = /[\p{L}\p{N}]/u;

function lowerCodePoints(value: string): string[] {
  return Array.from(value, (c) => c.toLowerCase());
}

function isWordStart(chars: string[], index: number): boolean {
  return index === 0 || !WORD_CHAR.test(chars[index - 1]);
}

function findSubstring(text: string[], query: string[]): number {
  let first = -1;
  for (let at = 0; at + query.length <= text.length; at++) {
    if (!query.every((c, k) => text[at + k] === c)) continue;
    if (isWordStart(text, at)) return at;
    if (first < 0) first = at;
  }
  return first;
}

export function fuzzyMatchIndices(query: string, text: string): FuzzyMatch | null {
  const trimmed = query.trim();
  if (!trimmed) return { score: 0, indices: [] };

  const original = Array.from(text);
  const haystack = lowerCodePoints(text);
  const needle = lowerCodePoints(trimmed);

  const at = findSubstring(haystack, needle);
  if (at >= 0) {
    const bonus = isWordStart(original, at) ? WORD_START_BONUS_SUBSTRING : 0;
    return {
      score: SUBSTRING_SCORE + bonus,
      indices: needle.map((_, k) => at + k),
    };
  }

  const indices: number[] = [];
  let score = 0;
  let from = 0;
  let previous = -2;
  for (const c of needle) {
    if (!c.trim()) continue;
    const hit = haystack.indexOf(c, from);
    if (hit < 0) return null;
    score += hit === previous + 1 ? HIT_SCORE + ADJACENT_BONUS : HIT_SCORE;
    if (isWordStart(original, hit)) score += WORD_START_BONUS_SCATTERED;
    indices.push(hit);
    previous = hit;
    from = hit + 1;
  }
  return { score: Math.min(score, SCATTERED_SCORE_CAP), indices };
}

export function rankByQuery<T>(
  items: readonly T[],
  query: string,
  texts: (item: T) => string[],
): { item: T; matches: (FuzzyMatch | null)[] }[] {
  if (!query.trim()) {
    return items.map((item) => ({
      item,
      matches: texts(item).map(() => ({ score: 0, indices: [] })),
    }));
  }

  const ranked = items.flatMap((item, order) => {
    const matches = texts(item).map((text) => fuzzyMatchIndices(query, text));
    const scores = matches.flatMap((m) => (m ? [m.score] : []));
    return scores.length ? [{ item, matches, order, best: Math.max(...scores) }] : [];
  });
  ranked.sort((a, b) => b.best - a.best || a.order - b.order);
  return ranked.map(({ item, matches }) => ({ item, matches }));
}
