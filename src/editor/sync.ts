export interface SourcePoint {
  line: number;
  column: number;
}

export function parseSourcepos(value: string | undefined | null): SourcePoint | null {
  const m = /^(\d+):(\d+)-\d+:\d+$/.exec(value ?? "");
  return m ? { line: Number(m[1]), column: Number(m[2]) } : null;
}

export function sourceAt(target: Pick<Element, "closest"> | null): SourcePoint | null {
  return parseSourcepos(target?.closest<HTMLElement>("[data-sourcepos]")?.dataset.sourcepos);
}

export function elementForLine(root: Pick<ParentNode, "querySelectorAll">, line: number): HTMLElement | null {
  let found: HTMLElement | null = null;
  for (const el of root.querySelectorAll<HTMLElement>("[data-sourcepos]")) {
    const at = parseSourcepos(el.dataset.sourcepos);
    if (!at || at.line > line) break;
    found = el;
  }
  return found;
}
