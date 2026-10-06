/** ソースの行と、プレビューの位置の対応。 */

export interface SourcePoint {
  line: number;
  column: number;
}

/** comrak の `data-sourcepos="行:列-行:列"` の、始まりの位置 */
export function parseSourcepos(value: string | undefined | null): SourcePoint | null {
  const m = /^(\d+):(\d+)-\d+:\d+$/.exec(value ?? "");
  return m ? { line: Number(m[1]), column: Number(m[2]) } : null;
}

/** クリックした要素から、いちばん近い位置情報つきの祖先をたどって、ソースの位置を返す */
export function sourceAt(target: Pick<Element, "closest"> | null): SourcePoint | null {
  return parseSourcepos(target?.closest<HTMLElement>("[data-sourcepos]")?.dataset.sourcepos);
}

/** `line` 以前に始まる要素のうち、文書の順で最後のもの（そこに居る要素のうち、いちばん細かいもの） */
export function elementForLine(root: Pick<ParentNode, "querySelectorAll">, line: number): HTMLElement | null {
  let found: HTMLElement | null = null;
  for (const el of root.querySelectorAll<HTMLElement>("[data-sourcepos]")) {
    const at = parseSourcepos(el.dataset.sourcepos);
    if (!at || at.line > line) break;
    found = el;
  }
  return found;
}
