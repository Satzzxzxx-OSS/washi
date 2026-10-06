import type { Diagnostic as CmDiagnostic } from "@codemirror/lint";
import type { Diagnostic } from "../api";

/** CodeMirror の文書のうち、位置の計算に使う部分（テストでは偽物を渡す） */
export interface DocLike {
  readonly lines: number;
  readonly length: number;
  line(n: number): { from: number; to: number; text: string };
}

/** 1 始まりの「コードポイントの列」を、その行の中の UTF-16 の位置にする。行末を越えたら行末 */
export function columnToOffset(lineText: string, column: number): number {
  let units = 0;
  let seen = 0;
  for (const ch of lineText) {
    if (seen >= column - 1) break;
    units += ch.length;
    seen += 1;
  }
  return units;
}

function position(doc: DocLike, line: number, column: number): number {
  const n = Math.min(Math.max(line, 1), doc.lines);
  const l = doc.line(n);
  return Math.min(l.from + columnToOffset(l.text, column), l.to);
}

/**
 * 診断を、CodeMirror の波線にする。開いているファイルの診断だけ（`file` が `null`）。
 * 範囲が空なら 1 文字ぶんに広げ、本文が短くなっていて範囲が外れたら、末尾に収める。
 */
export function toCmDiagnostics(doc: DocLike, list: readonly Diagnostic[]): CmDiagnostic[] {
  return list
    .filter((d) => d.file === null)
    .map((d) => {
      const from = position(doc, d.line, d.column);
      let to = position(doc, d.endLine, d.endColumn);
      if (to <= from) to = Math.min(from + 1, doc.length);
      const hints = d.hints.map((h) => `\nHint: ${h}`).join("");
      return {
        from: Math.min(from, doc.length),
        to: Math.max(to, Math.min(from, doc.length)),
        severity: d.severity,
        message: d.message + hints,
      };
    });
}
