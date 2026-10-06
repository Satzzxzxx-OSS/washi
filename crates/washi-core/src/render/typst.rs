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

use super::{Output, Renderer, SourceLocation};

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

    fn dependency_dirs(&self, path: &Path) -> Vec<PathBuf> {
        super::deps::typst(path)
    }

    fn locate(&self, path: &Path, page: usize, x: f64, y: f64) -> Result<Option<SourceLocation>, String> {
        let session = SESSIONS
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or("まだコンパイルされていません。再読み込みしてください")?;
        Ok(session.locate(page, x, y))
    }
}

impl Session {
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

    let (pdf, document) = engine
        .with_world(|world| {
            let document = typst::compile::<PagedDocument>(world)
                .output
                .map_err(|diagnostics| describe_all(world, &diagnostics))?;
            let pdf = typst_pdf::pdf(&document, &Default::default())
                .map_err(|diagnostics| describe_all(world, &diagnostics))?;
            Ok::<_, String>((pdf, document))
        })
        .map_err(|e| format!("Typst を初期化できません: {e}"))??;
    Ok(Compiled { pdf, document, engine })
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
}
