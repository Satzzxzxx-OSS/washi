/** 検索の対象になっている、いま見えている文書の文字列と、選択の位置。 */
export interface SearchSource {
  text(): string;
  /** いま選択している文字列の始まりの、`text()` の中での位置。文書の外や分からなければ `null` */
  selectionStart(): number | null;
  /** `text()` の `start` から `length` 文字を選択して、見える位置までスクロールする */
  select(start: number, length: number): boolean;
  /** まだ描かれていない部分があるか（PDF は近くのページしか描かない） */
  partial(): boolean;
}

/** 描画されない（検索にもかからない）要素 */
const SKIPPED = new Set(["BUTTON", "SCRIPT", "STYLE", "TITLE", "DESC"]);

interface Piece {
  node: Text;
  start: number;
}

export function domSearchSource(root: HTMLElement): SearchSource {
  /** 隠れている部分と、ボタンや KaTeX の読み上げ用の複製を除いて、文字列を並べる */
  const collect = () => {
    const pieces: Piece[] = [];
    let text = "";
    const walk = (node: Node) => {
      for (const child of node.childNodes) {
        // Mermaid の図は SVG なので、HTML 以外の要素も辿る
        if (child instanceof Element) {
          if (child.hasAttribute("hidden") || SKIPPED.has(child.tagName.toUpperCase()) || child.classList.contains("katex-mathml")) continue;
          walk(child);
        } else if (child.nodeType === Node.TEXT_NODE) {
          pieces.push({ node: child as Text, start: text.length });
          text += (child as Text).data;
        }
      }
    };
    walk(root);
    return { text, pieces };
  };

  /** `text()` の位置から、(テキストノード, その中の位置) を求める。`end` は、範囲の終わり（その文字の直後）の位置 */
  const locate = (pieces: Piece[], at: number, end: boolean) =>
    pieces.find(({ node, start }) => (end ? at > start && at <= start + node.length : at >= start && at < start + node.length));

  return {
    text: () => collect().text,
    selectionStart() {
      const selection = getSelection();
      const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
      if (!range || range.collapsed) return null;
      const piece = collect().pieces.find((p) => p.node === range.startContainer);
      return piece ? piece.start + range.startOffset : null;
    },
    select(start, length) {
      const { pieces } = collect();
      const from = locate(pieces, start, false);
      const to = locate(pieces, start + length, true);
      if (!from || !to) return false;
      const range = document.createRange();
      range.setStart(from.node, start - from.start);
      range.setEnd(to.node, start + length - to.start);
      const selection = getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      const target = from.node.parentElement;
      target?.scrollIntoView({ block: "center", behavior: "smooth" });
      return true;
    },
    partial: () => root.querySelector(".textLayer:empty") !== null,
  };
}
