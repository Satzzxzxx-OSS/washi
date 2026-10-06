/** ファイルの種類ごとの、編集まわりの決まり。 */

export type Kind = "markdown" | "typst" | "latex" | "mermaid";

const KINDS: Record<string, Kind> = {
  md: "markdown",
  markdown: "markdown",
  mdown: "markdown",
  typ: "typst",
  tex: "latex",
  latex: "latex",
  mmd: "mermaid",
  mermaid: "mermaid",
};

/** 編集できる種類。PDF などは `null` */
export function kindOf(path: string): Kind | null {
  const dot = path.lastIndexOf(".");
  if (dot < 0 || path.lastIndexOf("/") > dot) return null;
  return KINDS[path.slice(dot + 1).toLowerCase()] ?? null;
}

export interface RenderPolicy {
  strategy: "throttle" | "debounce";
  delay: number;
}

/**
 * プレビューを描き直す頻度（最初の値。使ってみて調整する）。
 * Typst は描画を打ち切れないので、少し間を置く。LaTeX は外部のコマンドで重いので、入力が止まってから。
 */
export function renderPolicy(kind: Kind): RenderPolicy {
  switch (kind) {
    case "markdown":
    case "mermaid":
      return { strategy: "throttle", delay: 150 };
    case "typst":
      return { strategy: "throttle", delay: 400 };
    case "latex":
      return { strategy: "debounce", delay: 1200 };
  }
}

/** ウィンドウのタイトルに付ける、未保存の印 */
export const DIRTY_MARK = "● ";
