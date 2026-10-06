#!/usr/bin/env python3
"""Washi（和紙）のロゴを SVG で生成する。

漉きの不揃いな縁をもつ和紙の上に、朱の落款（「和」を白抜き）を押したもの。
紙の繊維、縁の揺らぎ、朱のかすれは乱数（--seed）で決まるので、同じ引数なら同じ絵になる。

    python3 scripts/make_logo.py --font ZenOldMincho-Black.ttf --out docs/assets/logo.svg

必要なもの: fontTools（pip install fonttools）と、文字の形を借りる日本語フォント。
フォントはリポジトリに入れない。文字は SVG のパスとして書き出すので、見る側にフォントは要らない。
Zen Old Mincho（SIL Open Font License 1.1, https://github.com/google/fonts/tree/main/ofl/zenoldmincho）で作る想定。
"""

import argparse
import math
import random
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

SIZE = 1024
PAPER = "#fbf9f4"
PAPER_DEEP = "#ece4d0"
FIBERS = ("#e2d9c2", "#d6ccb2", "#efe8d8", "#cfc4a8")
VERMILION = "#b9442f"
VERMILION_DEEP = "#8f3022"


def closed_path(points):
    """点列を通る滑らかな閉じた曲線（Catmull-Rom を 3 次ベジェに直したもの）"""
    n = len(points)
    d = f"M{points[0][0]:.1f} {points[0][1]:.1f}"
    for i in range(n):
        p0, p1, p2, p3 = points[(i - 1) % n], points[i], points[(i + 1) % n], points[(i + 2) % n]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f"C{c1[0]:.1f} {c1[1]:.1f} {c2[0]:.1f} {c2[1]:.1f} {p2[0]:.1f} {p2[1]:.1f}"
    return d + "Z"


def wobble(rng, amplitude, harmonics=(2, 3, 5, 7, 11, 17)):
    """角度から揺らぎの大きさを返す、なめらかな 1 次元ノイズ"""
    terms = [(amplitude / k**0.7, k, rng.uniform(0, math.tau)) for k in harmonics]
    return lambda t: sum(a * math.sin(k * t + phase) for a, k, phase in terms)


def squircle(cx, cy, half_w, half_h, power, rng, jitter, points=120):
    """角丸の四角（スーパー楕円）の縁を、外向きに揺らした閉じた曲線"""
    noise = wobble(rng, jitter)
    out = []
    for i in range(points):
        t = math.tau * i / points
        c, s = math.cos(t), math.sin(t)
        x = half_w * math.copysign(abs(c) ** (2 / power), c)
        y = half_h * math.copysign(abs(s) ** (2 / power), s)
        length = math.hypot(x, y) or 1
        push = noise(t)
        out.append((cx + x + x / length * push, cy + y + y / length * push))
    return closed_path(out)


def fibers(rng, count, margin):
    """紙の繊維。短くて細い曲線を、向きと濃さをばらして散らす"""
    parts = []
    for _ in range(count):
        x = rng.uniform(margin, SIZE - margin)
        y = rng.uniform(margin, SIZE - margin)
        length = rng.uniform(40, 190)
        angle = rng.gauss(math.radians(-8), math.radians(55))
        dx, dy = math.cos(angle) * length, math.sin(angle) * length
        bend = rng.uniform(-0.28, 0.28) * length
        cx, cy = x + dx / 2 - math.sin(angle) * bend, y + dy / 2 + math.cos(angle) * bend
        width = rng.choice((1.2, 1.6, 2.0, 2.6, 3.4))
        opacity = rng.uniform(0.22, 0.62)
        parts.append(
            f'<path d="M{x:.1f} {y:.1f}Q{cx:.1f} {cy:.1f} {x + dx:.1f} {y + dy:.1f}" '
            f'stroke="{rng.choice(FIBERS)}" stroke-width="{width}" stroke-opacity="{opacity:.2f}"/>'
        )
    return "\n      ".join(parts)


def glyph_path(font_path, char, cx, cy, height):
    """文字の輪郭を、中心 (cx, cy)・高さ `height` に収まる SVG パスとして返す"""
    font = TTFont(font_path)
    glyphs = font.getGlyphSet()
    name = font.getBestCmap().get(ord(char))
    if name is None:
        raise SystemExit(f"フォントに「{char}」がありません: {font_path}")
    bounds = BoundsPen(glyphs)
    glyphs[name].draw(bounds)
    x0, y0, x1, y1 = bounds.bounds
    scale = height / max(y1 - y0, x1 - x0)
    pen = SVGPathPen(glyphs)
    glyphs[name].draw(
        TransformPen(pen, (scale, 0, 0, -scale, cx - scale * (x0 + x1) / 2, cy + scale * (y0 + y1) / 2))
    )
    return pen.getCommands()


def specks(rng, cx, cy, half, count):
    """朱の抜け（インクが乗らなかった小さな点）。縁に近いほど多い"""
    parts = []
    for _ in range(count):
        t = rng.uniform(0, math.tau)
        radius = half * (1 - abs(rng.gauss(0, 0.33)))
        x = cx + math.cos(t) * radius * rng.uniform(0.2, 1.0)
        y = cy + math.sin(t) * radius * rng.uniform(0.2, 1.0)
        parts.append(f'<ellipse cx="{x:.1f}" cy="{y:.1f}" rx="{rng.uniform(1.2, 6):.1f}" ry="{rng.uniform(1.0, 4.5):.1f}" '
                     f'transform="rotate({rng.uniform(0, 180):.0f} {x:.1f} {y:.1f})" fill="#000" fill-opacity="{rng.uniform(0.45, 0.95):.2f}"/>')
    return "\n        ".join(parts)


def build(font_path, char, seed, shown):
    rng = random.Random(seed)
    middle = SIZE / 2

    paper = squircle(middle, middle, 448, 448, 5.2, rng, 5.5)
    seal_cx, seal_cy, seal_half = middle + 6, middle + 14, 246
    seal = squircle(seal_cx, seal_cy, seal_half, seal_half, 7.5, rng, 3.2)
    keyline = squircle(seal_cx, seal_cy, seal_half - 30, seal_half - 30, 7.5, rng, 1.6)
    glyph = glyph_path(font_path, char, seal_cx, seal_cy + 6, seal_half * 1.32)

    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SIZE} {SIZE}" width="{shown}" height="{shown}" role="img" aria-label="Washi">
  <title>Washi 和紙</title>
  <defs>
    <clipPath id="sheet"><path d="{paper}"/></clipPath>
    <radialGradient id="vignette" cx="50%" cy="46%" r="62%">
      <stop offset="0.55" stop-color="{PAPER}" stop-opacity="0"/>
      <stop offset="1" stop-color="{PAPER_DEEP}" stop-opacity="0.85"/>
    </radialGradient>
    <linearGradient id="ink" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{VERMILION}"/>
      <stop offset="1" stop-color="{VERMILION_DEEP}"/>
    </linearGradient>
    <filter id="lift" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="16" stdDeviation="20" flood-color="#2a1d10" flood-opacity="0.22"/>
    </filter>
    <mask id="stamp" maskUnits="userSpaceOnUse" x="0" y="0" width="{SIZE}" height="{SIZE}">
      <rect width="{SIZE}" height="{SIZE}" fill="#fff"/>
      <path d="{glyph}" fill="#000"/>
      <g>
        {specks(rng, seal_cx, seal_cy, seal_half, 70)}
      </g>
    </mask>
  </defs>

  <path d="{paper}" fill="{PAPER}" filter="url(#lift)"/>
  <g clip-path="url(#sheet)">
    <rect width="{SIZE}" height="{SIZE}" fill="url(#vignette)"/>
    <g fill="none" stroke-linecap="round">
      {fibers(rng, 230, 40)}
    </g>
  </g>

  <g transform="rotate(-3.5 {seal_cx} {seal_cy})">
    <g mask="url(#stamp)">
      <path d="{seal}" fill="url(#ink)"/>
      <path d="{keyline}" fill="none" stroke="{PAPER}" stroke-opacity="0.62" stroke-width="8"/>
    </g>
  </g>
</svg>
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--font", required=True, help="文字の形を借りる日本語フォント（.ttf / .otf）")
    parser.add_argument("--out", default="docs/assets/logo.svg")
    parser.add_argument("--char", default="和")
    parser.add_argument("--size", type=int, default=160, help="表示サイズ（px）。README の Markdown 画像はこの大きさで出る")
    parser.add_argument("--seed", type=int, default=7, help="繊維・縁・かすれの乱数の種")
    args = parser.parse_args()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build(args.font, args.char, args.seed, args.size), encoding="utf-8")
    print(f"wrote {out} ({out.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
