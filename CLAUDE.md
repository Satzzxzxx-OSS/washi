# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

Washi（和紙）は Markdown / Typst / LaTeX / Mermaid / PDF を読むための閲覧専用ビューア（Tauri 2 + Rust + TypeScript、フレームワークなしの素の TS）。編集機能は意図的に持たない。ファイルの保存を監視し、スクロール位置とズームを保ったまま再描画する。UI の文言は日本語。README は英語のみで、短く保つ（比較表は要点だけ）。ランディングは日英の 2 ページ（`docs/index.html` が日本語、`docs/en/index.html` が英語）で、内容を変えるときは両方を揃える。

## コマンド

```sh
pnpm install
pnpm tauri dev                  # 起動（ファイルを渡す: pnpm tauri dev -- -- --gui examples/showcase.md）
pnpm build                      # tsc + vite build（型チェックを兼ねる）
pnpm test                       # フロントエンドのテスト（vitest run）
pnpm vitest run src/viewer.test.ts            # 単一ファイルのテスト
pnpm vitest run -t "テスト名"                  # 名前で絞る
cargo test --workspace                                               # Rust のテスト
cargo test -p washi-core <name>                                       # Rust の単一テスト
cargo test -p washi-core -- --ignored    # ネットワーク必須のテスト（examples/packages.typ など）
pnpm release                    # 配布用 .app ビルド（scripts/release.sh。ローカルパスを RUSTFLAGS の remap で除去する）
```

Lint / formatter の設定はない。

### ブラウザだけでの画面確認（Tauri 不要）

`e2e/` は Rust の実際の描画結果を保存し、`e2e/mock-tauri.ts` で Tauri API をモックして返すハーネス。

```sh
cargo test -p washi-core dump -- --ignored   # e2e/fixtures を生成
pnpm dev
# http://localhost:1420/e2e/harness.html?file=/x/examples/showcase.md&outline=1&theme=dark
```

## アーキテクチャ

**Rust がレンダリングし、フロントは表示と状態保持だけを担う。** Cargo workspace は 2 crate: `crates/washi-core`（tauri 非依存。`render/`・`cli/`・`editor.rs`）と `src-tauri`（tauri アプリ本体。`washi-core` に依存）。描画と CLI だけなら `cargo test -p washi-core` で tauri をビルドせずに済む。`main.rs` は先に `washi_core::cli::run` を呼び、`None` のときだけ GUI を起動する。

- `crates/washi-core/src/render/` — 形式ごとの `Renderer` トレイト実装（`markdown` / `mermaid` / `typst` / `tex` / `pdf`）。`render/mod.rs` の `RENDERERS` 静的配列が拡張子→Renderer の登録表。`render()` は `Output::Html` か `Output::Pdf` を返し、IPC では先頭 1 バイトのタグ（0=HTML, 1=PDF）＋本体の `into_wire()` 形式で送る。`render_text()`（貼り付け）と `locate()`（PDF 座標→ソース行）はデフォルト実装つきの任意メソッド。貼り付け時の形式自動判定は `render/detect.rs`。
- 形式追加: Renderer を実装 → `RENDERERS` に 1 行足す。既存の `Output` 種別ならフロントの変更は不要。
- 外部コマンド（latexmk / tectonic など）は `TexEngine` のようにトレイトの背後に置き、テストでは差し替える。実行は `render/process.rs` の `run` を通す: タイムアウト（既定 300 秒、`WASHI_COMPILE_TIMEOUT`）とプロセスグループごとの kill に対応し、`Job::start(path)` で同じ文書の新しい描画が古い描画を打ち切る（フロントは古い結果を token で捨てるので安全）。このため、同じファイルを並列で描画するテストは直列にすること（`render/mod.rs` の `serial()`）。Typst はプロセス内コンパイルなのでタイムアウトできない。Typst は `typst` クレートで直接コンパイル、LaTeX は `latexmk` 優先で無ければ `tectonic`。
- 保存の監視は文書のあるフォルダ（非再帰）に加えて、文書が読み込むファイルのフォルダも見る。`Renderer::dependency_dirs`（`render/deps.rs` がソースを静的に走査: LaTeX の `\input` など、Typst の `#include` など、Markdown の画像）が返したフォルダを、`commands::render` が描画のたびに `FileWatcher::set_dependencies` で監視先へ反映する（描画に失敗しても）。再帰監視はしない（HOME など広いフォルダを開いたときに重くなるため）。
- `src-tauri/src/` の `commands.rs`（フロントから呼ぶ Tauri コマンド）、`launch.rs`（起動引数・Finder から開く・1 ファイル 1 ウィンドウの生成）、`watch.rs`（ファイル監視。エディタの一時ファイルや `.DS_Store` は無視、アトミック保存に対応）、`menu.rs`（ネイティブメニュー）。`washi-core` 側の `cli/`（`washi` CLI。起動を待たずすぐ戻る。`install` / `formats` サブコマンド、`-` で標準入力）、`editor.rs`（同じく washi-core。⌘クリックでのソースジャンプ先エディタ解決。`WASHI_EDITOR` で上書き）。
- `src/viewer.ts` — 描画の流れの中心。`Host`（Tauri 呼び出しの抽象）・`View[]` を注入して使うので、テストやモック環境でも差し替えられる。再描画のたびに `token` をインクリメントして古い描画結果を破棄し、`Scroll` でスクロール位置を保つ。
- `src/views/` — `View` インターフェースの実装（Markdown の HTML、PDF は pdf.js）。`outline.ts` が目次、`jump.ts` がソースジャンプ、`api.ts` が Tauri `invoke` ラッパー。
- Markdown は Rust 側（comrak）で HTML 化し、KaTeX・Mermaid・highlight.js はフロント側で後処理する。相対画像は埋め込み、`.md` / `.typ` / `.tex` への相対リンクは Washi 内で開く。

## CLI の契約（変えるときの規則）

`washi --json` の出力、終了コード、stdout / stderr の使い分けは、AI エージェントが依存する公開した契約。型は `crates/washi-core/src/cli/report.rs` にあり、README の「For AI agents」と対応する。

- 成功は stdout に JSON 1 行、失敗は stderr に JSON 1 行（stdout は空）。`--json` が無ければ文。終了コードは 0 / 1（起動失敗）/ 2（それ以外）で、決めるのは `report::exit_code` の 1 か所。
- 1.x ではキーの追加だけ可。削除・改名・型や意味の変更、`kind` や形式名（`format`）の変更は、`FORMAT_VERSION` を上げて CHANGELOG に書く。
- 契約テスト（`cli/tests.rs` の `contract_*`）がキー集合・型・ストリームを固定している。落ちたら、テストを直す前に「契約を変えてよいか」を判断する。`readme_documents_the_contract` が、`ErrorKind` や形式名の README への書き忘れを検出する。
- 引数なしの `washi` はフォアグラウンドでアプリを起動する（Finder が同じバイナリを引数なしで起動するため。切り離して戻る挙動に変えない）。

## CI とリリース

- `.github/workflows/ci.yml`: `washi-core` のテスト（Linux、Tauri なし）とアプリ全体のテスト・ビルド（macOS）。毎回は走らせない方針なので、手動実行（`workflow_dispatch`）のみ。
- `.github/workflows/release.yml`: GitHub Release を公開（publish）すると起動する（`gh release create v0.1.0 --generate-notes` など）。そのタグを checkout し、タグが `v<major>.<minor>.<patch>` の形で、`package.json` / `tauri.conf.json` / `src-tauri/Cargo.toml` のバージョンと一致しないと失敗する。`pnpm release` で `.app` を作り、`Washi-<version>-aarch64.zip` と `.sha256` を、公開済みの Release に `gh release upload` で添付し、`kiwamizamurai/homebrew-tap` の `Casks/washi.rb` を生成して push する（secret `HOMEBREW_TAP_TOKEN` が必要。未設定なら、この tap の更新だけ警告を出して飛ばす）。Actions から `tag` を指定して手動で再実行もできる。
- 配布は Apple Silicon の macOS のみ。ad-hoc 署名なので Cask の `postflight` で quarantine を外している。LaTeX エンジンは同梱せず、Cask の `depends_on formula: "tectonic"` で入れる。
- ワークフローはまだ GitHub 上で実行していない（Cask 生成部分だけローカルで再現して構文を確認した）。

## 注意点

- アイコンは `python3 scripts/make_logo.py --all` で `app-icon.svg`・`docs/assets/logo.svg`（README 用）・`docs/favicon.svg` を作り、`pnpm tauri icon app-icon.svg --output <一時フォルダ>` で作った PNG/ICNS/ICO のうち、`src-tauri/icons/` に既にある名前のものだけを入れ替える（iOS・Android 用は使わない）。
- Finder への登録拡張子は `src-tauri/tauri.conf.json` の `bundle.fileAssociations`。形式を足したら、ここにも足す（`src-tauri/src/lib.rs` のテストが、`supported_extensions()` との不一致を検出する）。
- Tauri の権限は `src-tauri/capabilities/` と `tauri.conf.json` で管理。新しいプラグイン API を使うときは capability の追加が必要。
- `examples/` は各形式の実文書に近いサンプルで、`dump` テストの入力にもなる。
- CLI は macOS / Linux 向け（Windows 未対応）。署名・公証は未対応。
