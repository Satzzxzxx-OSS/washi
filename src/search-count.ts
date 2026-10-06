/** 検索の件数（「3 / 12」）を数える、DOM に依存しない部分。 */

export interface MatchSummary {
  total: number;
  /** 選択の位置にある一致が、先頭から何番目か（1 始まり）。選択が一致の上に無ければ `null` */
  index: number | null;
}

/** 大文字小文字を区別せず、重ならないように数えた一致の開始位置。大文字小文字の変換で長さが変わる文字があれば `null` */
export function matchStarts(text: string, query: string): number[] | null {
  if (!query) return [];
  const haystack = text.toLowerCase();
  const needle = query.toLowerCase();
  if (haystack.length !== text.length || needle.length !== query.length) return null;
  const starts: number[] = [];
  for (let at = haystack.indexOf(needle); at !== -1; at = haystack.indexOf(needle, at + needle.length)) {
    starts.push(at);
  }
  return starts;
}

/** `selectionStart` は、いま選択されている（`find` が見つけた）文字列の始まりの位置。無ければ `null` */
export function summarize(text: string, query: string, selectionStart: number | null): MatchSummary | null {
  const starts = matchStarts(text, query);
  if (!starts) return null;
  const at = selectionStart === null ? -1 : starts.indexOf(selectionStart);
  return { total: starts.length, index: at === -1 ? null : at + 1 };
}

/** 表示する文字列。`partial` は、まだ描かれていない部分があって、実際はもっと多いかもしれないとき */
export function formatCount(summary: MatchSummary | null, partial: boolean): string {
  if (!summary) return "";
  if (summary.total === 0) return "No results";
  const total = `${summary.total}${partial ? "+" : ""}`;
  return `${summary.index ?? "–"} / ${total}`;
}

/**
 * 次（または前）の一致の開始位置。選択（`from`）が無ければ、先頭（後ろ向きなら末尾）から。端まで来たら反対側へ回る。一致が無ければ `null`
 */
export function stepTo(starts: readonly number[], from: number | null, backwards: boolean): number | null {
  if (starts.length === 0) return null;
  if (from === null) return backwards ? starts[starts.length - 1] : starts[0];
  if (backwards) return [...starts].reverse().find((s) => s < from) ?? starts[starts.length - 1];
  return starts.find((s) => s > from) ?? starts[0];
}
