/**
 * Subsequence fuzzy matcher for the Command palette (DESIGN.md §5.1).
 *
 * Scoring rewards the things a person actually types: characters that land on a
 * word boundary, and runs of consecutive characters. Matching is case
 * insensitive and diacritic-insensitive so "verify" finds "Vérifier".
 *
 * Returns null when the query does not match, which lets callers filter without
 * inspecting a sentinel score.
 */

const BOUNDARY = /[\s\-_/\\.[\]]/;

export interface FuzzyMatch {
  /** Higher is better. Only comparable between matches of the same query. */
  readonly score: number;
  /** Indices into the original haystack, for highlighting. */
  readonly indices: readonly number[];
}

function fold(value: string): string {
  return value
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

export function fuzzyMatch(haystack: string, query: string): FuzzyMatch | null {
  const needle = fold(query.trim());
  if (needle.length === 0) return { score: 0, indices: [] };

  const target = fold(haystack);
  const indices: number[] = [];
  let score = 0;
  let cursor = 0;
  let previousIndex = -2;

  for (const char of needle) {
    const found = target.indexOf(char, cursor);
    if (found === -1) return null;

    if (found === previousIndex + 1) {
      // Consecutive characters are the strongest signal of intent.
      score += 8;
    }

    if (found === 0 || BOUNDARY.test(target[found - 1] ?? "")) {
      // Start of a word.
      score += 6;
    }

    if (char >= "0" && char <= "9") {
      // Digits are rarer and more deliberate than letters.
      score += 2;
    }

    score -= Math.min(found - cursor, 6);

    indices.push(found);
    previousIndex = found;
    cursor = found + 1;
  }

  // Shorter haystacks that contain the same query are the better answer.
  score += Math.max(0, 12 - Math.floor(target.length / 6));

  return { score, indices };
}

export interface Ranked<T> {
  readonly item: T;
  readonly score: number;
  readonly indices: readonly number[];
}

/** Filters and orders candidates, dropping anything that does not match. */
export function fuzzyRank<T>(
  items: readonly T[],
  query: string,
  toText: (item: T) => string,
): Ranked<T>[] {
  if (query.trim().length === 0) {
    return items.map((item) => ({ item, score: 0, indices: [] }));
  }

  return items
    .map((item) => {
      const match = fuzzyMatch(toText(item), query);
      return match === null ? undefined : { item, ...match };
    })
    .filter((entry): entry is Ranked<T> => entry !== undefined)
    .sort((a, b) => b.score - a.score);
}
