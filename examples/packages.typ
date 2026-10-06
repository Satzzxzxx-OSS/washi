#import "@preview/cetz:0.5.2": canvas, draw
#import "@preview/fletcher:0.5.8": diagram, node, edge

#set page(width: 15cm, height: auto, margin: 1cm)
#set text(lang: "ja", size: 10pt)

= `@preview` パッケージの例

== CeTZ で描く図

#align(center, canvas({
  import draw: *
  circle((0, 0), radius: 1.2, stroke: rgb("#b5483a"), fill: rgb("#f6efe6"))
  line((-2.2, 0), (2.2, 0), mark: (end: ">"))
  line((0, -1.6), (0, 1.6), mark: (end: ">"))
  content((0, 0), [和紙])
  content((2.4, -0.3), [$x$])
  content((0.3, 1.8), [$y$])
}))

== Fletcher の図式

#align(center, diagram(
  node-stroke: 0.8pt,
  node-inset: 8pt,
  node((0, 0), [ソース]),
  edge("->", [読み込み]),
  node((1, 0), [Renderer]),
  edge("->", [PDF]),
  node((2, 0), [pdf.js]),
  edge((1, 0), (1, 1), "->", [HTML]),
  node((1, 1), [WebView]),
))
