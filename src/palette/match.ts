/** あいまい検索。DOM に依存しない。 */

export interface Match {
  /** 大きいほど、よく合っている */
  score: number;
  /** 一致した文字の位置（強調表示に使う） */
  indices: number[];
}

const isWordStart = (text: string, i: number) => i === 0 || !/[\p{L}\p{N}]/u.test(text[i - 1]);

/** 空白で区切った語を、すべて含むものだけに合う（順不同）。一致しなければ `null` */
export function fuzzyMatch(query: string, text: string): Match | null {
  const terms = query.trim().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return { score: 0, indices: [] };
  let score = 0;
  const indices = new Set<number>();
  for (const term of terms) {
    const m = matchTerm(term.toLowerCase(), text.toLowerCase());
    if (!m) return null;
    score += m.score;
    m.indices.forEach((i) => indices.add(i));
  }
  return { score, indices: [...indices].sort((a, b) => a - b) };
}

function matchTerm(term: string, text: string): Match | null {
  const substring = bestSubstring(term, text);
  if (substring !== null) {
    const bonus = (substring === 0 ? 30 : 0) + (isWordStart(text, substring) ? 20 : 0);
    return {
      score: 100 + bonus - Math.min(substring, 50) - Math.max(0, text.length - term.length) * 0.1,
      indices: Array.from({ length: term.length }, (_, k) => substring + k),
    };
  }
  return subsequence(term, text);
}

/** 語の頭から始まる位置を優先して、最初に現れる位置を返す */
function bestSubstring(term: string, text: string): number | null {
  let first: number | null = null;
  for (let at = text.indexOf(term); at !== -1; at = text.indexOf(term, at + 1)) {
    first ??= at;
    if (isWordStart(text, at)) return at;
  }
  return first;
}

/** 文字が順に現れればよい（飛び飛びでもよい）一致。連続や語の頭を高く評価する */
function subsequence(term: string, text: string): Match | null {
  const indices: number[] = [];
  let score = 0;
  let from = 0;
  for (const ch of term) {
    let at = text.indexOf(ch, from);
    if (at === -1) return null;
    // 近くに語の頭があれば、そちらを使う
    for (let j = at; j < Math.min(text.length, at + 8); j++) {
      if (text[j] === ch && isWordStart(text, j)) {
        at = j;
        break;
      }
    }
    const previous = indices[indices.length - 1];
    score += 1 + (previous !== undefined && at === previous + 1 ? 3 : 0) + (isWordStart(text, at) ? 4 : 0);
    indices.push(at);
    from = at + 1;
  }
  return { score: Math.min(score, 60), indices };
}
