mod autolink;
mod deps;
mod detect;
pub use detect::extension_of_text;
mod markdown;
mod mermaid;
mod pdf;
mod process;
mod synctex;
mod tex;
pub mod tools;
mod offsets;
mod typst;

use std::path::{Path, PathBuf};

pub enum Output {
    Html(String),
    Pdf(Vec<u8>),
}

impl Output {
    pub fn into_wire(self) -> Vec<u8> {
        let (tag, body) = match self {
            Self::Html(html) => (0u8, html.into_bytes()),
            Self::Pdf(bytes) => (1u8, bytes),
        };
        let mut wire = Vec::with_capacity(body.len() + 1);
        wire.push(tag);
        wire.extend_from_slice(&body);
        wire
    }
}

/// 診断（エラーと警告）の重大度
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// 編集中の本文に対する診断。行と列は 1 始まりで、列は Unicode のコードポイントの数
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    /// `None` は、描画した本文そのもの。ほかのファイル（`#include` 先など）なら、プロジェクトの根からの相対パス
    pub file: Option<String>,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub severity: Severity,
    pub message: String,
    pub hints: Vec<String>,
}

/// 保存前の本文の描画結果。失敗しても診断は返す（画面側は、直前の良いプレビューを残して波線だけ更新する）
pub struct Rendered {
    pub output: Result<Output, String>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Rendered {
    /// 画面に渡す形式: `[tag: u8][診断 JSON の長さ: u32 ビッグエンディアン][診断 JSON][本体]`。
    /// `tag` は 0=HTML、1=PDF、2=失敗（本体は UTF-8 のメッセージ。画面は直前の良いプレビューを残す）
    pub fn into_wire(self) -> Vec<u8> {
        let diagnostics = serde_json::to_vec(&self.diagnostics).unwrap_or_else(|_| b"[]".to_vec());
        let (tag, body) = match self.output {
            Ok(Output::Html(html)) => (0u8, html.into_bytes()),
            Ok(Output::Pdf(bytes)) => (1u8, bytes),
            Err(message) => (2u8, message.into_bytes()),
        };
        let mut wire = Vec::with_capacity(5 + diagnostics.len() + body.len());
        wire.push(tag);
        wire.extend_from_slice(&(diagnostics.len() as u32).to_be_bytes());
        wire.extend_from_slice(&diagnostics);
        wire.extend_from_slice(&body);
        wire
    }
}

/// プレビュー上の位置（PDF のポイント、ページは 1 始まり）
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PreviewPosition {
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CompletionItem {
    pub label: String,
    /// 補完で入れる文字列（スニペットの記法を含むことがある）。無ければ `label`
    pub apply: Option<String>,
    pub detail: Option<String>,
    /// `syntax` / `func` / `type` / `param` / `constant` / `path` / `package` / `label` / `font` / `symbol`
    pub kind: String,
}

/// 補完の候補。`offset` は、置き換える範囲の始まり（UTF-16、エディタの位置）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Completions {
    pub offset: usize,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}

pub trait Renderer: Sync {
    /// 形式の名前（`washi --json` の `format` に出る。変えると契約が変わる）
    fn name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn render(&self, path: &Path) -> Result<Output, String>;

    fn render_text(&self, _source: &str) -> Result<Output, String> {
        Err("this format cannot be pasted".into())
    }

    fn locate(&self, _path: &Path, _page: usize, _x: f64, _y: f64) -> Result<Option<SourceLocation>, String> {
        Ok(None)
    }

    /// この文書が読み込むファイルのあるフォルダ（章、参考文献、画像など）。保存の監視先を増やすのに使う
    fn dependency_dirs(&self, _path: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    /// `path` に属する、まだ保存していない本文 `text` を描画する。相対パス（画像、`#include`、`\input` など）は
    /// `path` のあるフォルダから解決する。既定は、文脈を持たない `render_text`
    fn render_buffer(&self, _path: &Path, text: &str) -> Rendered {
        Rendered { output: self.render_text(text), diagnostics: Vec::new() }
    }

    /// 保存前の本文に対する `dependency_dirs`
    fn buffer_dependency_dirs(&self, path: &Path, _text: &str) -> Vec<PathBuf> {
        self.dependency_dirs(path)
    }

    /// ソースの行・列（1 始まり）から、プレビュー上の位置を探す（前方検索）。直前の `render_buffer` / `render` が前提
    fn locate_forward(&self, _path: &Path, _line: u32, _column: u32) -> Result<Option<PreviewPosition>, String> {
        Ok(None)
    }

    /// カーソル位置（UTF-16）での補完。対応しない形式は空
    fn complete(&self, _path: &Path, _text: &str, _offset: usize, _explicit: bool) -> Result<Completions, String> {
        Ok(Completions { offset: _offset, items: Vec::new() })
    }
}

static RENDERERS: [&dyn Renderer; 5] = [
    &markdown::MarkdownRenderer,
    &mermaid::MermaidRenderer,
    &typst::TypstRenderer,
    &tex::TexRenderer(tex::SystemTexEngine),
    &pdf::PdfRenderer,
];

pub fn renderer_for(path: &Path) -> Option<&'static dyn Renderer> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    RENDERERS
        .iter()
        .find(|r| r.extensions().contains(&ext.as_str()))
        .copied()
}

/// 拡張子から、何として描画されるか（`markdown` / `mermaid` / `typst` / `latex` / `pdf`）
pub fn format_of(path: &Path) -> Option<&'static str> {
    renderer_for(path).map(|r| r.name())
}

/// 形式ごとの（名前、拡張子）
pub fn formats() -> Vec<(&'static str, &'static [&'static str])> {
    RENDERERS.iter().map(|r| (r.name(), r.extensions())).collect()
}

pub fn supported_extensions() -> Vec<&'static str> {
    RENDERERS
        .iter()
        .flat_map(|r| r.extensions().iter().copied())
        .collect()
}

pub fn render(path: &Path) -> Result<Output, String> {
    renderer_for(path)
        .ok_or_else(|| format!("unsupported format: {}", path.display()))?
        .render(path)
}

pub fn locate(path: &Path, page: usize, x: f64, y: f64) -> Result<Option<SourceLocation>, String> {
    renderer_for(path)
        .ok_or_else(|| format!("unsupported format: {}", path.display()))?
        .locate(path, page, x, y)
}

/// 文書の保存に加えて監視したいフォルダ。対応しない形式や、読めないファイルでは空
pub fn dependency_dirs(path: &Path) -> Vec<PathBuf> {
    renderer_for(path).map(|r| r.dependency_dirs(path)).unwrap_or_default()
}

/// 保存前の本文を描画する。対応しない形式は、診断なしの失敗
pub fn render_buffer(path: &Path, text: &str) -> Rendered {
    match renderer_for(path) {
        Some(renderer) => renderer.render_buffer(path, text),
        None => Rendered {
            output: Err(format!("unsupported format: {}", path.display())),
            diagnostics: Vec::new(),
        },
    }
}

/// 保存前の本文が読み込むファイルのあるフォルダ
pub fn buffer_dependency_dirs(path: &Path, text: &str) -> Vec<PathBuf> {
    renderer_for(path).map(|r| r.buffer_dependency_dirs(path, text)).unwrap_or_default()
}

pub fn locate_forward(path: &Path, line: u32, column: u32) -> Result<Option<PreviewPosition>, String> {
    match renderer_for(path) {
        Some(renderer) => renderer.locate_forward(path, line, column),
        None => Ok(None),
    }
}

pub fn complete(path: &Path, text: &str, offset: usize, explicit: bool) -> Result<Completions, String> {
    match renderer_for(path) {
        Some(renderer) => renderer.complete(path, text, offset, explicit),
        None => Ok(Completions { offset, items: Vec::new() }),
    }
}

pub fn render_text(text: &str) -> Result<Output, String> {
    let ext = detect::extension_of_text(text);
    renderer_for(Path::new(&format!("pasted.{ext}")))
        .ok_or("no renderer for this format")?
        .render_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_text_is_rendered_by_the_detected_format() {
        assert!(matches!(render_text("# Hello\n"), Ok(Output::Html(_))));
        assert!(matches!(render_text("graph LR; A-->B"), Ok(Output::Html(_))));
        assert!(matches!(render_text("#set page(paper: \"a4\")\n= Hi\n"), Ok(Output::Pdf(_))));
    }

    #[test]
    fn dispatches_by_extension_case_insensitively() {
        for name in ["a.md", "a.MARKDOWN", "a.typ", "a.tex", "a.PDF", "a.mmd"] {
            assert!(renderer_for(Path::new(name)).is_some(), "{name}");
        }
        for name in ["a.txt", "a", "a.docx"] {
            assert!(renderer_for(Path::new(name)).is_none(), "{name}");
        }
    }

    #[test]
    fn format_names_are_stable_and_unique() {
        let names: Vec<_> = formats().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["markdown", "mermaid", "typst", "latex", "pdf"]);
        assert_eq!(format_of(Path::new("/x/a.MD")), Some("markdown"));
        assert_eq!(format_of(Path::new("/x/a.latex")), Some("latex"));
        assert_eq!(format_of(Path::new("/x/a.txt")), None);
    }

    #[test]
    fn extensions_do_not_overlap() {
        let mut all = supported_extensions();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total);
    }

    #[test]
    fn wire_format_is_tag_then_body() {
        assert_eq!(Output::Html("x".into()).into_wire(), vec![0, b'x']);
        assert_eq!(Output::Pdf(vec![7, 8]).into_wire(), vec![1, 7, 8]);
    }

    #[test]
    fn unsupported_file_is_an_error() {
        assert!(render(Path::new("a.txt")).is_err());
    }
}

#[cfg(test)]
mod examples {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    // 同じ文書の描画は新しい方が古い方を打ち切る（アプリでは 1 文書 1 ウィンドウ）。
    // 同じ例を並列で描画するテストがぶつからないよう、外部エンジンを使う描画は直列にする
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn example(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)
    }

    fn html(name: &str) -> String {
        match render(&example(name)).unwrap_or_else(|e| panic!("{name}: {e}")) {
            Output::Html(h) => h,
            Output::Pdf(_) => panic!("{name}: HTML を期待"),
        }
    }

    fn pdf(name: &str) -> Vec<u8> {
        match render(&example(name)).unwrap_or_else(|e| panic!("{name}: {e}")) {
            Output::Pdf(b) => b,
            Output::Html(_) => panic!("{name}: PDF を期待"),
        }
    }

    #[test]
    fn showcase_markdown_uses_every_feature() {
        let h = html("showcase.md");
        for needle in [
            "<table>", "data-math-style=\"display\"", "language-mermaid", "language-rust",
            "markdown-alert", "footnote", "data:image/svg+xml;base64,", "checkbox", "raw HTML omitted",
        ] {
            assert!(h.contains(needle), "showcase.md に {needle} が無い");
        }
        assert!(!h.contains("<script>"));
        assert!(!h.contains("author: kiwamizamurai"), "front matter が漏れている");
    }

    #[test]
    fn architecture_diagram_renders() {
        assert!(html("architecture.mmd").contains("classDiagram"));
    }

    #[test]
    fn report_typst_compiles_with_bibliography_and_many_pages() {
        let bytes = pdf("report.typ");
        assert!(bytes.starts_with(b"%PDF"));
        let pages = bytes.windows(12).filter(|w| w == b"/Type /Page\n").count()
            + bytes.windows(11).filter(|w| w == b"/Type/Page ").count();
        assert!(bytes.len() > 20_000, "出力が小さすぎる: {} bytes (pages≈{pages})", bytes.len());
    }

    #[test]
    fn paper_tex_compiles_when_an_engine_is_available() {
        if tools::find_tool("latexmk").is_none() && tools::find_tool("tectonic").is_none() {
            return;
        }
        let _serial = serial();
        assert!(pdf("paper.tex").starts_with(b"%PDF"));
    }

    fn grid_hits(name: &str) -> Vec<SourceLocation> {
        let _serial = serial();
        let path = example(name);
        render(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        (60..800)
            .step_by(24)
            .flat_map(|y| (80..520).step_by(48).map(move |x| (x, y)))
            .filter_map(|(x, y)| locate(&path, 1, x as f64, y as f64).unwrap())
            .collect()
    }

    #[test]
    fn report_typst_clicks_map_back_to_lines() {
        let hits = grid_hits("report.typ");
        assert!(hits.len() > 5, "ヒットが少なすぎる: {}", hits.len());
        assert!(hits.iter().all(|h| h.file.file_name().is_some_and(|n| n == "main.typ" || n == "report.typ")), "{hits:?}");
        assert!(hits.iter().all(|h| h.line >= 1 && h.line <= 140), "{hits:?}");
    }

    #[test]
    fn paper_tex_clicks_map_back_to_lines_when_an_engine_is_available() {
        if tools::find_tool("latexmk").is_none() && tools::find_tool("tectonic").is_none() {
            return;
        }
        let hits = grid_hits("paper.tex");
        assert!(hits.len() > 5, "ヒットが少なすぎる: {}", hits.len());
        assert!(hits.iter().all(|h| h.file.file_name().is_some_and(|n| n == "paper.tex")), "{hits:?}");
        assert!(hits.iter().all(|h| h.line >= 1 && h.line <= 110), "{hits:?}");
        let distinct: std::collections::BTreeSet<_> = hits.iter().map(|h| h.line).collect();
        assert!(distinct.len() > 3, "行が1つに偏っている: {distinct:?}");
    }

    #[test]
    #[ignore = "ネットワークから @preview パッケージを取得する"]
    fn packages_typst_resolves_preview_packages() {
        assert!(pdf("packages.typ").starts_with(b"%PDF"));
    }
}

#[cfg(test)]
mod e2e_fixtures {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    #[ignore = "e2e/fixtures に実際の描画結果を書き出す"]
    fn dump() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out_dir = root.join("e2e/fixtures");
        fs::create_dir_all(&out_dir).unwrap();
        for name in ["showcase.md", "architecture.mmd", "report.typ", "paper.tex"] {
            let output = render(&root.join("examples").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            fs::write(out_dir.join(format!("{name}.wire")), output.into_wire()).unwrap();
        }
    }

    #[test]
    fn a_buffer_wire_carries_the_tag_the_diagnostics_and_the_body() {
        fn split(wire: &[u8]) -> (u8, serde_json::Value, &[u8]) {
            let len = u32::from_be_bytes(wire[1..5].try_into().unwrap()) as usize;
            (wire[0], serde_json::from_slice(&wire[5..5 + len]).unwrap(), &wire[5 + len..])
        }
        let diagnostic = Diagnostic {
            file: None,
            line: 2,
            column: 3,
            end_line: 2,
            end_column: 9,
            severity: Severity::Error,
            message: "boom".into(),
            hints: vec!["try this".into()],
        };
        let ok = Rendered { output: Ok(Output::Html("<p>x</p>".into())), diagnostics: vec![] }.into_wire();
        assert_eq!(split(&ok), (0, serde_json::json!([]), "<p>x</p>".as_bytes()));
        let pdf = Rendered { output: Ok(Output::Pdf(vec![7, 8])), diagnostics: vec![] }.into_wire();
        assert_eq!(split(&pdf), (1, serde_json::json!([]), &[7u8, 8][..]));
        let failed = Rendered { output: Err("失敗".into()), diagnostics: vec![diagnostic] }.into_wire();
        let (tag, json, body) = split(&failed);
        assert_eq!((tag, body), (2, "失敗".as_bytes()));
        assert_eq!(json[0]["line"], 2);
        assert_eq!(json[0]["severity"], "error");
        assert_eq!(json[0]["file"], serde_json::Value::Null);
        assert_eq!(json[0]["hints"][0], "try this");
    }

    #[test]
    fn unsupported_formats_have_no_buffer_render_completion_or_forward_search() {
        let path = Path::new("/tmp/washi-none/a.txt");
        let rendered = render_buffer(path, "x");
        assert!(rendered.output.is_err() && rendered.diagnostics.is_empty());
        assert!(complete(path, "x", 1, false).unwrap().items.is_empty());
        assert_eq!(locate_forward(path, 1, 1).unwrap(), None);
        assert!(buffer_dependency_dirs(path, "x").is_empty());
    }
}
