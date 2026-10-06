#set document(title: "Washi 技術レポート", author: "kiwamizamurai")
#set page(
  paper: "a4",
  margin: (x: 2.2cm, y: 2.6cm),
  header: context {
    if counter(page).get().first() > 1 {
      set text(size: 8.5pt, fill: luma(110))
      [Washi 技術レポート #h(1fr) 2026-10-06]
      v(-4pt)
      line(length: 100%, stroke: 0.4pt + luma(200))
    }
  },
  footer: context align(center, text(size: 9pt, fill: luma(110))[
    #counter(page).display("1 / 1", both: true)
  ]),
)
#set text(font: ("Hiragino Mincho ProN", "Noto Serif CJK JP", "Libertinus Serif"), lang: "ja", size: 10.5pt)
#set par(justify: true, leading: 0.85em)
#set heading(numbering: "1.1")
#set math.equation(numbering: "(1)")
#show heading: set block(above: 1.4em, below: 0.8em)
#show link: set text(fill: rgb("#b5483a"))
#show raw.where(block: true): block.with(fill: luma(247), inset: 9pt, radius: 3pt, width: 100%)

#let accent = rgb("#b5483a")
#let callout(title: "メモ", body) = block(
  width: 100%,
  inset: 10pt,
  radius: 3pt,
  fill: rgb("#f6efe6"),
  stroke: (left: 3pt + accent),
)[#text(weight: "bold", fill: accent)[#title] \ #body]

#align(center)[
  #v(1cm)
  #text(size: 24pt, weight: "bold")[Washi 技術レポート]
  #v(2pt)
  #text(size: 12pt, fill: luma(100))[Typst・Markdown・LaTeX を静かに読む]
  #v(6pt)
  kiwamizamurai · 2026-10-06
]

#v(0.6cm)
#outline(title: [目次], depth: 2, indent: 1.2em)
#pagebreak()

= 概要

Washi は、ローカルの文書を *保存するたびに即座に再描画* する軽量ビューアです。
Markdown は HTML として、Typst と LaTeX は PDF として表示します @knuth1984 @madje2023。
本書は Typst の機能（見出し番号、相互参照、表、数式、図、脚注、段組み、文献）を一通り使った例です。

#callout(title: "ポイント")[
  変換は *Renderer* という共通インターフェースの背後に隠されており、形式の追加は実装を 1 つ足して登録するだけで済みます。
]

= アーキテクチャ <sec:arch>

描画の流れを @fig:pipeline に、対応形式を @tbl:formats に示します。

#figure(
  grid(
    columns: (1fr, auto, 1fr, auto, 1fr),
    align: horizon + center,
    gutter: 8pt,
    rect(inset: 10pt, radius: 4pt, fill: luma(245), stroke: 0.6pt + luma(160))[ソース \ #text(size: 8pt)[.md / .typ / .tex]],
    text(fill: accent, size: 14pt)[→],
    rect(inset: 10pt, radius: 4pt, fill: rgb("#f6efe6"), stroke: 0.6pt + accent)[*Renderer*],
    text(fill: accent, size: 14pt)[→],
    rect(inset: 10pt, radius: 4pt, fill: luma(245), stroke: 0.6pt + luma(160))[HTML / PDF],
  ),
  caption: [描画パイプライン],
) <fig:pipeline>

#figure(
  table(
    columns: (auto, 1fr, auto),
    align: (left, left, right),
    stroke: (x: none, y: 0.5pt + luma(190)),
    inset: 7pt,
    table.header([*形式*], [*描画経路*], [*目安*]),
    [Markdown], [comrak → HTML → WebView (KaTeX / Mermaid)], [即時],
    [Typst], [typst crate → PDF → pdf.js], [~50 ms],
    [LaTeX], [tectonic または latexmk → PDF → pdf.js], [~1 s],
    [Mermaid], [フェンス付き Markdown として描画], [即時],
  ),
  caption: [対応形式と描画経路],
) <tbl:formats>

== 設計の方針

- 各形式は `Renderer` トレイトを実装し、拡張子で引ける。
- 外部コマンド（TeX エンジン）は `TexEngine` の背後に置き、テストでは差し替える。
- 画面側は `View` インターフェースで、出力の種類ごとに表示を切り替える。

== 数式

積分と総和の基本公式:
$ integral_0^1 x^2 dif x = 1/3, quad sum_(k=1)^n k = (n(n+1))/2 $ <eq:basic>

行列式と逆行列（@eq:basic の続き）:
$ mat(a, b; c, d)^(-1) = 1/(a d - b c) mat(d, -b; -c, a) $

Fourier 変換:
$ hat(f)(xi) = integral_(-oo)^(oo) f(x) e^(-2 pi i x xi) dif x $

= 実装メモ

== コード例

```rust
pub trait Renderer: Sync {
    fn extensions(&self) -> &'static [&'static str];
    fn render(&self, path: &Path) -> Result<Output, String>;
}
```

== 段組みと脚注

#columns(2, gutter: 16pt)[
  #lorem(70)
  #footnote[段組みの中でも脚注は正しくページ下部に置かれます。]

  #lorem(60)
]

#v(0.4cm)
詳しくは #link("https://typst.app/docs")[Typst ドキュメント] を参照してください（@sec:arch も参照）。

#pagebreak()

= 付録

== 長い本文

#lorem(180)

#lorem(150)

#bibliography("refs.bib", title: [参考文献], style: "ieee")
