![Washi logo](docs/assets/logo.svg)

# Washi（和紙）

A lightweight viewer for reading Markdown, Typst, LaTeX, Mermaid and PDF quietly, re-rendering every time you save.
Leave the editing to your favorite editor or AI agent; Washi only reads.

Website: [English](https://kiwamizamurai.github.io/washi/) · [日本語](https://kiwamizamurai.github.io/washi/ja/)

## Install

Apple Silicon macOS, with [Homebrew](https://brew.sh):

```sh
brew install --cask kiwamizamurai/tap/washi
brew upgrade --cask washi     # update
```

- This also puts the `washi` command on your PATH and installs `tectonic` for LaTeX (`latexmk` is used instead if you have it). Washi bundles no LaTeX engine.
- Homebrew trusts the tap for the fully qualified name above. To install by short name, run `brew trust --cask kiwamizamurai/tap/washi` first.
- Washi is not signed or notarized (ad-hoc signature), so the Cask removes the quarantine attribute that Homebrew adds.
- Without Homebrew: download `Washi-<version>-aarch64.zip` from [GitHub Releases](https://github.com/kiwamizamurai/washi/releases), unzip it into `/Applications`, then allow it once under System Settings → Privacy & Security → Open Anyway.
- To build from source: see Development below.

## Usage

- **Open**: drop a file on the window, `⌘O`, Finder's "Open With", or `washi a.md b.typ` (one window per file).
- **Paste**: copy text and press `⌘V`; Markdown, Typst, LaTeX or Mermaid is detected from the content.
- **Save to refresh**: save the open file, or any file it includes (LaTeX `\input` and `.bib`, Typst `#include`, Markdown images, even in subfolders), and it re-renders in place, keeping your scroll position.

| Action | Shortcut |
|---|---|
| Open / Reload / Print | `⌘O` / `⌘R` / `⌘P` |
| Find | `⌘F` (`Enter` next, `⇧Enter` previous, `Esc` close) |
| Toggle the outline | `⇧⌘O` |
| Zoom in / out / actual size | `⌘+` / `⌘-` / `⌘0` (`⌘` + wheel and pinch also work) |
| Jump to source | `⌘`-click a Typst / LaTeX PDF |
| Theme, text width | "View" menu |

**Jump to source** opens the matching line in `code`, `cursor`, `zed`, `subl` or `mate` (first one found; otherwise the default text editor). Set `WASHI_EDITOR` to choose, for example `WASHI_EDITOR="code -g {file}:{line}:{column}"`.

## Supported formats

| Format | Rendered as | Notes |
|---|---|---|
| Markdown | HTML | GFM, footnotes, math (KaTeX), Mermaid, syntax highlighting, alerts, front matter, relative images and links |
| Typst | PDF | Built-in compiler; `@preview` packages are fetched on first use; errors show line and column |
| LaTeX | PDF | `latexmk`, or `tectonic` if missing; stops after 300 s (`WASHI_COMPILE_TIMEOUT` changes it) |
| Mermaid | SVG | `.mmd` / `.mermaid`; click a diagram to enlarge it |
| PDF | PDF | Text selection, search, bookmarks |

## For AI agents

`washi <file>` opens a file and **returns immediately**, so agent commands are never blocked.

```sh
washi plan.md --json                  # {"format_version":1,"launched":true,"files":[{"path":"/abs/plan.md","format":"markdown"}]}
washi plan.md --no-launch --json      # validate only
cat draft.md | washi - --name draft   # open stdin; the same --name updates the open window
washi formats --json                  # supported formats and the tools Washi found
```

- **Streams**: success prints one JSON line on stdout and nothing on stderr. Failure prints one JSON line on stderr, `{"format_version":1,"errors":[{"kind":...,"message":...,"path"?:...}]}`, and nothing on stdout.
- **Exit codes**: `0` success, `1` the app could not be launched (`launch_failed`), `2` anything else. Only `0` means success.
- **`kind`**: `usage`, `not_found`, `is_directory`, `unsupported_format`, `stdin_empty`, `stdin_unreadable`, `io`, `launch_failed`. `files[].format` is one of `markdown`, `mermaid`, `typst`, `latex`, `pdf`.
- **Compatibility**: within 1.x keys are only added, never removed, renamed or retyped, so ignore keys you do not know. A breaking change raises `format_version` and is announced in the [changelog](CHANGELOG.md).

## Comparison with other apps

Apps that open Markdown, Typst and LaTeX as files, checked on 2026-10-06 against their repositories, releases and sites. "n/d" means not documented, not "absent".

| | Washi | [Typeset Viewer](https://github.com/osteele/typeset-viewer) | [Texpile](https://github.com/texpile/texpile) | [Oleafly](https://github.com/Oleafly/Oleafly) | [hibi](https://github.com/schmayterling/hibi) | [Quire Writer](https://github.com/Andesprit/quire-writer) |
|---|---|---|---|---|---|---|
| Role | Viewer only | Viewer + notes | Editor | Writing workspace | Note editor | Writing app + AI agent |
| License | MIT | Closed (free) | AGPL-3.0 | AGPL-3.0 | GPL-3.0 | GPL-3.0 |
| Re-renders on external save | ○ (also included files) | ○ (also includes, `.bib`) | n/d | n/d | n/d | n/d |
| Built-in editor | × | × | ○ | ○ | ○ | ○ |
| PDF → source jump | ○ Typst, LaTeX | n/d | n/d | ○ | n/d | ○ Typst |
| macOS download | 24 MB (zip) | 28 MB (zip) | n/d | 191 MB (dmg) | n/d | 56 MB (dmg) |

Apps for only some of the formats: [TeXlyre](https://github.com/TeXlyre/texlyre) (Typst and LaTeX, web), [Moraya](https://github.com/zouwei/moraya) (Markdown and Typst), [SuperGoodViewer](https://github.com/dotsg/SuperGoodViewer) (Markdown only), [Osh](https://github.com/Hyp4tia/Osh) (Quick Look), [Presto](https://github.com/Presto-io/Presto) (Markdown → Typst → PDF).

To **write**, use Oleafly, Texpile, hibi or Quire Writer. To **read**, use Washi or Typeset Viewer: in what I checked, they are the only viewer-only apps that re-render all three formats on save. Typeset Viewer adds notes, review and presentation mode but is closed source and Apple Silicon only; Washi is MIT, renders Mermaid and has a CLI, and keeps to reading.

## Development

```sh
pnpm install
pnpm tauri dev          # run (open a file: pnpm tauri dev -- -- --gui examples/showcase.md)
pnpm test               # frontend tests
cargo test --workspace  # Rust tests (cargo test -p washi-core skips Tauri and is fast)
pnpm release            # release build of the .app
```

Rendering and the CLI live in `crates/washi-core` (no Tauri dependency); the app is `src-tauri` plus `src` (TypeScript). To add a format, implement `Renderer` in `crates/washi-core/src/render/`, add it to `RENDERERS` in `render/mod.rs`, and add its extensions to `fileAssociations` in `src-tauri/tauri.conf.json`. Publishing a GitHub Release (`gh release create v0.1.0 --generate-notes`) runs `.github/workflows/release.yml`: it builds the `.app` for that tag, attaches it to the release, and updates the Homebrew tap. CI (`ci.yml`) runs only when started by hand.

## License

MIT. Dependencies follow their own licenses (Typst: Apache-2.0, pdf.js: Apache-2.0, KaTeX / Mermaid: MIT, comrak: BSD-2-Clause, highlight.js: BSD-3-Clause, and others).

The logo and app icon are generated by `scripts/make_logo.py`.
