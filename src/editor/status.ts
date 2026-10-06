import type { Diagnostic, Severity } from "../api";

/** 文字数（Unicode のコードポイントの数。サロゲートペアは 1 文字） */
export function countChars(text: string): number {
  let count = text.length;
  for (let i = 0; i < text.length - 1; i++) {
    const high = text.charCodeAt(i);
    if (high >= 0xd800 && high <= 0xdbff) {
      const low = text.charCodeAt(i + 1);
      if (low >= 0xdc00 && low <= 0xdfff) {
        count--;
        i++;
      }
    }
  }
  return count;
}

export interface DiagnosticCounts {
  errors: number;
  warnings: number;
}

export function summarize(list: readonly Diagnostic[]): DiagnosticCounts {
  let errors = 0;
  let warnings = 0;
  for (const d of list) {
    if (d.severity === "error") errors++;
    else warnings++;
  }
  return { errors, warnings };
}

export interface Cursor {
  line: number;
  column: number;
}

/** カーソルより後ろにある、同じ重大度の最初の診断。末尾まで来たら先頭に戻る。無ければ `null` */
export function nextDiagnostic(list: readonly Diagnostic[], severity: Severity, cursor: Cursor): Diagnostic | null {
  const same = list
    .filter((d) => d.severity === severity)
    .sort((a, b) => a.line - b.line || a.column - b.column);
  if (same.length === 0) return null;
  return same.find((d) => d.line > cursor.line || (d.line === cursor.line && d.column > cursor.column)) ?? same[0];
}
