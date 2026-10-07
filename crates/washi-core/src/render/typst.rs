use std::{
    collections::HashMap,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use typst::{
    diag::{FileResult, Severity, SourceDiagnostic},
    foundations::{Bytes, Datetime, Duration},
    introspection::PagedPosition,
    layout::{Abs, Point},
    syntax::{DiagSpan, DiagSpanKind, FileId, Source, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
    Library, World, WorldExt,
};
use typst_as_lib::{typst_kit_options::TypstKitFontOptions, TypstEngine, TypstTemplateMainFile};
use typst_ide::{Jump, IdeWorld};
use typst_layout::PagedDocument;

use super::{
    deps, offsets, CompletionItem, Completions, Diagnostic, Output, PreviewPosition, Rendered, Renderer, Severity as DiagSeverity,
    SourceLocation,
};

type Engine = TypstEngine<TypstTemplateMainFile>;

struct Session {
    engine: Engine,
    document: PagedDocument,
    file: Option<PathBuf>,
    root: PathBuf,
}

struct Compiled {
    pdf: Vec<u8>,
    document: PagedDocument,
    engine: Engine,
}

static SESSIONS: LazyLock<Mutex<HashMap<PathBuf, Arc<Session>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct TypstRenderer;

impl Renderer for TypstRenderer {
    fn name(&self) -> &'static str {
        "typst"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["typ"]
    }

    fn render(&self, path: &Path) -> Result<Output, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let Compiled { pdf, document, engine } = compile(source, &root)?;
        let session = Session { engine, document, file: Some(path.to_path_buf()), root };
        SESSIONS.lock().unwrap().insert(path.to_path_buf(), Arc::new(session));
        Ok(Output::Pdf(pdf))
    }

    fn render_text(&self, source: &str) -> Result<Output, String> {
        compile(source.to_owned(), &std::env::temp_dir()).map(|c| Output::Pdf(c.pdf))
    }

    fn render_buffer(&self, path: &Path, text: &str) -> Rendered {
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let (result, diagnostics) = compile_full(text.to_owned(), &root);
        let output = result.map(|Compiled { pdf, document, engine }| {
            let session = Session { engine, document, file: Some(path.to_path_buf()), root };
            SESSIONS.lock().unwrap().insert(path.to_path_buf(), Arc::new(session));
            Output::Pdf(pdf)
        });
        Rendered { output, diagnostics }
    }

    fn buffer_dependency_dirs(&self, path: &Path, text: &str) -> Vec<PathBuf> {
        deps::typst_text(path, Some(text))
    }

    fn locate_forward(&self, path: &Path, line: u32, column: u32) -> Result<Option<PreviewPosition>, String> {
        let session = SESSIONS
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or("not compiled yet; reload and try again")?;
        Ok(session.locate_forward(line, column))
    }

    fn complete(&self, path: &Path, text: &str, offset: usize, explicit: bool) -> Result<Completions, String> {
        let cursor = offsets::utf16_to_byte(text, offset).ok_or("the position is inside a character")?;
        let root = path.parent().unwrap_or(Path::new("."));
        let engine = TypstEngine::builder()
            .main_file(text.to_owned())
            .search_fonts_with(
                TypstKitFontOptions::default()
                    .include_system_fonts(false)
                    .include_embedded_fonts(true),
            )
            .with_file_system_resolver(root)
            .build();
        let found = engine
            .with_world(|world| {
                let source = world.source(world.main()).ok()?;
                typst_ide::autocomplete(&IdeAdapter(world), None::<&PagedDocument>, &source, cursor, explicit)
            })
            .map_err(|e| format!("cannot initialise Typst: {e}"))?;
        let Some((start, items)) = found else {
            return Ok(Completions { offset, items: Vec::new() });
        };
        Ok(Completions {
            offset: offsets::byte_to_utf16(text, start),
            items: items.into_iter().map(completion_item).collect(),
        })
    }

    fn dependency_dirs(&self, path: &Path) -> Vec<PathBuf> {
        super::deps::typst(path)
    }

    fn locate(&self, path: &Path, page: usize, x: f64, y: f64) -> Result<Option<SourceLocation>, String> {
        let session = SESSIONS
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or("not compiled yet; reload and try again")?;
        Ok(session.locate(page, x, y))
    }
}

fn completion_item(c: typst_ide::Completion) -> CompletionItem {
    use typst_ide::CompletionKind as K;
    let kind = match c.kind {
        K::Syntax => "syntax",
        K::Func => "func",
        K::Type => "type",
        K::Param => "param",
        K::Constant => "constant",
        K::Path => "path",
        K::Package => "package",
        K::Label => "label",
        K::Font => "font",
        K::Symbol(_) => "symbol",
    };
    CompletionItem {
        label: c.label.to_string(),
        apply: c.apply.map(|a| a.to_string()),
        detail: c.detail.map(|d| d.to_string()),
        kind: kind.to_owned(),
    }
}

impl Session {
    fn locate_forward(&self, line: u32, column: u32) -> Option<PreviewPosition> {
        self.engine
            .with_world(|world| {
                let source = world.source(world.main()).ok()?;
                let offset = source
                    .lines()
                    .line_column_to_byte((line as usize).saturating_sub(1), (column as usize).saturating_sub(1))?;
                let position = typst_ide::jump_from_cursor(&self.document, &source, offset).into_iter().next()?;
                Some(PreviewPosition {
                    page: position.page.get() as u32,
                    x: position.point.x.to_pt(),
                    y: position.point.y.to_pt(),
                })
            })
            .ok()
            .flatten()
    }

    fn locate(&self, page: usize, x: f64, y: f64) -> Option<SourceLocation> {
        let page = NonZeroUsize::new(page).filter(|p| p.get() <= self.document.pages().len())?;
        let position = PagedPosition { page, point: Point::new(Abs::pt(x), Abs::pt(y)) };
        self.engine
            .with_world(|world| {
                let ide = IdeAdapter(world);
                match typst_ide::jump_from_click(&ide, &self.document, &position)? {
                    Jump::File(id, offset) => self.resolve(world, id, offset),
                    _ => None,
                }
            })
            .ok()
            .flatten()
    }

    fn resolve(&self, world: &dyn World, id: FileId, offset: usize) -> Option<SourceLocation> {
        let (line, column) = world.source(id).ok()?.lines().byte_to_line_column(offset)?;
        let file = if id == world.main() {
            self.file.clone()?
        } else if matches!(id.root(), VirtualRoot::Project) {
            self.root.join(id.vpath().get_without_slash())
        } else {
            return None;
        };
        Some(SourceLocation { file, line: line + 1, column: column + 1 })
    }
}

struct IdeAdapter<'a>(&'a dyn World);

impl World for IdeAdapter<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.0.library()
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.0.book()
    }
    fn main(&self) -> FileId {
        self.0.main()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.0.source(id)
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.0.file(id)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.0.font(index)
    }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.0.today(offset)
    }
}

impl IdeWorld for IdeAdapter<'_> {
    fn upcast(&self) -> &dyn World {
        self
    }
}

fn compile(source: String, root: &Path) -> Result<Compiled, String> {
    compile_full(source, root).0
}

fn compile_full(source: String, root: &Path) -> (Result<Compiled, String>, Vec<Diagnostic>) {
    let engine = TypstEngine::builder()
        .main_file(source)
        .search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(true)
                .include_embedded_fonts(true),
        )
        .with_file_system_resolver(root)
        .with_package_file_resolver()
        .build();

    let mut diagnostics = Vec::new();
    let built = engine
        .with_world(|world| {
            let warned = typst::compile::<PagedDocument>(world);
            diagnostics.extend(diagnostics_of(world, &warned.warnings));
            let document = match warned.output {
                Ok(document) => document,
                Err(errors) => {
                    diagnostics.extend(diagnostics_of(world, &errors));
                    return Err(describe_all(world, &errors));
                }
            };
            let pdf = typst_pdf::pdf(&document, &Default::default()).map_err(|errors| {
                diagnostics.extend(diagnostics_of(world, &errors));
                describe_all(world, &errors)
            })?;
            Ok::<_, String>((pdf, document))
        })
        .map_err(|e| format!("cannot initialise Typst: {e}"))
        .and_then(|inner| inner);
    let result = built.map(|(pdf, document)| Compiled { pdf, document, engine });
    (result, diagnostics)
}

fn diagnostics_of(world: &dyn World, items: &[SourceDiagnostic]) -> Vec<Diagnostic> {
    items
        .iter()
        .map(|d| {
            let severity = match d.severity {
                Severity::Error => DiagSeverity::Error,
                Severity::Warning => DiagSeverity::Warning,
            };
            let place = match d.span.get() {
                DiagSpanKind::Number { id, .. } | DiagSpanKind::Range { id, .. } => {
                    world.range(d.span).and_then(|range| {
                        let source = world.source(id).ok()?;
                        let text = source.text();
                        let (line, column) = offsets::byte_to_line_column(text, range.start);
                        let (end_line, end_column) = offsets::byte_to_line_column(text, range.end);
                        let file = (id != world.main()).then(|| id.vpath().get_without_slash().to_owned());
                        Some((file, line, column, end_line, end_column))
                    })
                }
                DiagSpanKind::Detached => None,
            };
            let (file, line, column, end_line, end_column) = place.unwrap_or((None, 1, 1, 1, 1));
            Diagnostic {
                file,
                line,
                column,
                end_line,
                end_column,
                severity,
                message: d.message.to_string(),
                hints: d.hints.iter().map(|h| h.v.to_string()).collect(),
            }
        })
        .collect()
}

fn describe_all(world: &dyn World, diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| describe(world, d))
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe(world: &dyn World, diagnostic: &SourceDiagnostic) -> String {
    let level = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let location = location(world, diagnostic.span).unwrap_or_default();
    let mut text = format!("{location}{level}: {}", diagnostic.message);
    for hint in &diagnostic.hints {
        text.push_str(&format!("\n  hint: {}", hint.v));
    }
    text
}

fn location(world: &dyn World, span: DiagSpan) -> Option<String> {
    let id = match span.get() {
        DiagSpanKind::Number { id, .. } | DiagSpanKind::Range { id, .. } => id,
        DiagSpanKind::Detached => return None,
    };
    let range = world.range(span)?;
    let (line, column) = world.source(id).ok()?.lines().byte_to_line_column(range.start)?;
    Some(format!(
        "{}:{}:{}: ",
        id.vpath().get_without_slash(),
        line + 1,
        column + 1
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_to_pdf() {
        let pdf = compile(
            "= こんにちは\n#lorem(30)\n$ sum_(i=1)^n i = (n(n+1))/2 $\n".into(),
            Path::new("."),
        )
        .unwrap()
        .pdf;
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn reports_errors_with_line_and_column() {
        let err = compile("= ok\n\n#undefined-function()\n".into(), Path::new(".")).err().unwrap();
        assert!(err.contains("error:"), "{err}");
        assert!(err.contains(":3:"), "行番号が含まれない: {err}");
    }

    #[test]
    fn warnings_do_not_fail_the_build() {
        assert!(compile("= ok\n".into(), Path::new(".")).is_ok());
    }

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("washi-typ-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn probe(path: &Path) -> Vec<SourceLocation> {
        (0..100)
            .step_by(4)
            .filter_map(|y| TypstRenderer.locate(path, 1, 10.0, y as f64).unwrap())
            .collect()
    }

    #[test]
    fn click_maps_back_to_the_line_in_the_main_file() {
        let dir = temp_project("main");
        let main = dir.join("main.typ");
        fs::write(&main, "#set page(margin: 0pt, width: 200pt, height: 100pt)\n#set text(size: 20pt)\nHello world\n").unwrap();
        TypstRenderer.render(&main).unwrap();

        let hits = probe(&main);
        assert!(!hits.is_empty(), "どこを押しても位置が見つからない");
        assert!(hits.iter().all(|h| h.file == main), "{hits:?}");
        assert!(hits.iter().any(|h| h.line == 3), "{hits:?}");
    }

    #[test]
    fn click_maps_into_included_files() {
        let dir = temp_project("include");
        let main = dir.join("main.typ");
        fs::write(dir.join("part.typ"), "Second paragraph from part.\n").unwrap();
        fs::write(
            &main,
            "#set page(margin: 0pt, width: 200pt, height: 100pt)\n#set text(size: 20pt)\nFirst paragraph.\n\n#include \"part.typ\"\n",
        )
        .unwrap();
        TypstRenderer.render(&main).unwrap();

        let files: Vec<PathBuf> = probe(&main).into_iter().map(|h| h.file).collect();
        assert!(files.contains(&main), "{files:?}");
        assert!(files.contains(&dir.join("part.typ")), "{files:?}");
    }

    #[test]
    fn locate_without_a_compiled_session_is_an_error() {
        assert!(TypstRenderer.locate(Path::new("/nowhere/x.typ"), 1, 0.0, 0.0).is_err());
    }

    #[test]
    fn locate_outside_the_document_finds_nothing() {
        let dir = temp_project("bounds");
        let main = dir.join("main.typ");
        fs::write(&main, "Hi\n").unwrap();
        TypstRenderer.render(&main).unwrap();
        assert_eq!(TypstRenderer.locate(&main, 0, 10.0, 10.0).unwrap(), None);
        assert_eq!(TypstRenderer.locate(&main, 99, 10.0, 10.0).unwrap(), None);
    }

    fn pdf_of(rendered: Rendered) -> Vec<u8> {
        match rendered.output.expect("コンパイルできる") {
            Output::Pdf(bytes) => bytes,
            Output::Html(_) => panic!("PDF を期待"),
        }
    }

    #[test]
    fn a_buffer_resolves_includes_from_the_documents_folder() {
        let dir = temp_project("buf-include");
        fs::write(dir.join("part.typ"), "Part text.\n").unwrap();
        let main = dir.join("main.typ");
        let rendered = TypstRenderer.render_buffer(&main, "= Title\n#include \"part.typ\"\n");
        assert!(rendered.output.is_ok(), "{:?}", rendered.output.err());
        assert!(rendered.diagnostics.iter().all(|d| d.severity != DiagSeverity::Error), "{:?}", rendered.diagnostics);
    }

    #[test]
    fn a_buffer_renders_exactly_like_the_saved_file() {
        let dir = temp_project("buf-golden");
        let main = dir.join("main.typ");
        let text = "#set page(width: 200pt, height: 100pt)\n= Hello\nSome text with $x^2$.\n";
        fs::write(&main, text).unwrap();
        let saved = match TypstRenderer.render(&main).unwrap() {
            Output::Pdf(bytes) => bytes,
            Output::Html(_) => panic!("PDF を期待"),
        };
        assert_eq!(pdf_of(TypstRenderer.render_buffer(&main, text)), saved);
    }

    #[test]
    fn errors_come_back_as_structured_diagnostics_even_though_the_render_fails() {
        let dir = temp_project("buf-error");
        let rendered = TypstRenderer.render_buffer(&dir.join("main.typ"), "= A\n#undefined-fn()\n");
        assert!(rendered.output.is_err());
        let error = rendered.diagnostics.iter().find(|d| d.severity == DiagSeverity::Error).expect("エラーの診断");
        assert_eq!((error.file.as_deref(), error.line), (None, 2));
        assert!(error.column >= 1 && error.end_column >= error.column, "{error:?}");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn diagnostic_columns_count_code_points_not_bytes() {
        let dir = temp_project("buf-cols");
        let rendered = TypstRenderer.render_buffer(&dir.join("main.typ"), "日本😀 #undefined-fn()\n");
        let error = rendered.diagnostics.iter().find(|d| d.severity == DiagSeverity::Error).expect("エラーの診断");
        assert_eq!(error.line, 1);
        assert_eq!(error.column, 6, "{error:?}");
    }

    #[test]
    fn warnings_are_reported_while_the_render_succeeds() {
        let dir = temp_project("buf-warn");
        let rendered = TypstRenderer.render_buffer(&dir.join("main.typ"), "#set text(font: \"No Such Font Family Washi\")\nHello\n");
        assert!(rendered.output.is_ok(), "{:?}", rendered.output.err());
        assert!(rendered.diagnostics.iter().any(|d| d.severity == DiagSeverity::Warning), "{:?}", rendered.diagnostics);
    }

    #[test]
    fn a_diagnostic_in_an_included_file_names_that_file() {
        let dir = temp_project("buf-other-file");
        fs::write(dir.join("bad.typ"), "#undefined-in-part()\n").unwrap();
        let rendered = TypstRenderer.render_buffer(&dir.join("main.typ"), "#include \"bad.typ\"\n");
        let error = rendered.diagnostics.iter().find(|d| d.severity == DiagSeverity::Error).expect("エラーの診断");
        assert_eq!(error.file.as_deref(), Some("bad.typ"), "{error:?}");
    }

    #[test]
    fn forward_search_finds_the_page_and_back_again() {
        let dir = temp_project("buf-forward");
        let main = dir.join("main.typ");
        let text = "#set page(margin: 0pt, width: 300pt, height: 120pt)\n#set text(size: 20pt)\nFirst paragraph.\n\nSecond paragraph.\n";
        assert!(TypstRenderer.render_buffer(&main, text).output.is_ok());
        let position = TypstRenderer.locate_forward(&main, 5, 1).unwrap().expect("前方検索で位置が出る");
        assert_eq!(position.page, 1);
        assert!(position.x.is_finite() && position.y.is_finite() && position.y > 20.0, "{position:?}");
        let back = [-8.0, -4.0, 2.0]
            .into_iter()
            .find_map(|dy| TypstRenderer.locate(&main, 1, position.x + 2.0, position.y + dy).unwrap())
            .expect("後方検索");
        assert_eq!((back.file, back.line), (main, 5));
    }

    #[test]
    fn forward_search_without_a_compiled_buffer_is_an_error() {
        assert!(TypstRenderer.locate_forward(Path::new("/nowhere/never.typ"), 1, 1).is_err());
    }

    #[test]
    fn completion_offers_functions_and_reports_utf16_offsets() {
        let dir = temp_project("buf-complete");
        let path = dir.join("main.typ");
        let plain = TypstRenderer.complete(&path, "#lore", 5, false).unwrap();
        assert!(plain.items.iter().any(|c| c.label == "lorem" && c.kind == "func"), "{:?}", plain.items.iter().map(|c| &c.label).collect::<Vec<_>>());

        let text = "日本語😀\n#lore";
        let cursor = text.encode_utf16().count();
        let after = TypstRenderer.complete(&path, text, cursor, false).unwrap();
        assert!(after.items.iter().any(|c| c.label == "lorem"));
        let start = "日本語😀\n#".encode_utf16().count();
        assert_eq!(after.offset, start, "置き換えは `lore` の頭から");
    }

    #[test]
    fn completion_rejects_a_position_inside_a_surrogate_pair() {
        let path = Path::new("/tmp/washi-none/main.typ");
        assert!(TypstRenderer.complete(path, "a😀b", 2, false).is_err());
    }

    #[test]
    fn buffer_dependency_dirs_follow_the_unsaved_text() {
        let dir = temp_project("buf-deps");
        fs::create_dir_all(dir.join("parts")).unwrap();
        let main = dir.join("main.typ");
        assert_eq!(TypstRenderer.buffer_dependency_dirs(&main, "#include \"parts/a.typ\""), vec![dir.join("parts")]);
    }
}
