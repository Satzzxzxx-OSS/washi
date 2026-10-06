use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    process::{self, Job, RunError},
    synctex::SyncTex,
    tools::find_tool,
    Output, PreviewPosition, Rendered, Renderer, SourceLocation,
};

/// 保存前の本文をコンパイルするための、同じフォルダの隠しファイルの接頭辞
const MIRROR_PREFIX: &str = ".washi-buf-";
/// これより古い隠しファイルは、異常終了の残りとして掃除する
const STALE_MIRROR_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

pub trait TexEngine: Sync {
    /// `source` を `out_dir` にコンパイルする。`cwd` は作業フォルダで、`\input` や図の相対パスはここから解決される
    fn compile(&self, source: &Path, out_dir: &Path, cwd: &Path) -> Result<(), String>;
}

pub struct TexRenderer<E>(pub E);

impl<E: TexEngine> Renderer for TexRenderer<E> {
    fn name(&self) -> &'static str {
        "latex"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["tex", "latex"]
    }

    fn render(&self, path: &Path) -> Result<Output, String> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid file name")?;
        let out_dir = out_dir_for(path);
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

        self.0.compile(path, &out_dir, path.parent().unwrap_or(Path::new(".")))?;

        fs::read(out_dir.join(format!("{stem}.pdf")))
            .map(Output::Pdf)
            .map_err(|e| format!("cannot read the PDF: {e}"))
    }

    fn render_text(&self, source: &str) -> Result<Output, String> {
        let dir = std::env::temp_dir().join(format!("washi-paste-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join("pasted.tex");
        fs::write(&file, source).map_err(|e| e.to_string())?;
        self.render(&file)
    }

    fn dependency_dirs(&self, path: &Path) -> Vec<PathBuf> {
        super::deps::latex(path)
    }

    fn render_buffer(&self, path: &Path, text: &str) -> Rendered {
        Rendered { output: self.render_mirror(path, text), diagnostics: Vec::new() }
    }

    fn buffer_dependency_dirs(&self, path: &Path, text: &str) -> Vec<PathBuf> {
        super::deps::latex_text(path, Some(text))
    }

    fn locate_forward(&self, path: &Path, line: u32, _column: u32) -> Result<Option<PreviewPosition>, String> {
        let synctex = read_synctex(path)?;
        Ok(synctex
            .forward(|input| same_file(path, input), line)
            .map(|hit| PreviewPosition { page: hit.page, x: hit.x, y: hit.y }))
    }

    fn locate(&self, path: &Path, page: usize, x: f64, y: f64) -> Result<Option<SourceLocation>, String> {
        let synctex = read_synctex(path)?;
        let Some(hit) = synctex.inverse(page as u32, x, y) else {
            return Ok(None);
        };
        let file = resolve_input(path, &hit.input);
        Ok(Some(SourceLocation { file, line: hit.line as usize, column: 1 }))
    }
}

impl<E: TexEngine> TexRenderer<E> {
    /// 保存前の本文を、同じフォルダの隠しファイル（`.washi-buf-<名前>.tex`）に書いてコンパイルする。
    /// 作業フォルダが同じなので、`\input`・`.bib`・図の相対パスが、保存したときと同じように解決される
    fn render_mirror(&self, path: &Path, text: &str) -> Result<Output, String> {
        let parent = path.parent().unwrap_or(Path::new("."));
        let out_dir = out_dir_for(path);
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        remove_stale_mirrors(parent);

        let mirror = Mirror::write(path, text)?;
        self.0.compile(&mirror.path, &out_dir, parent)?;
        fs::read(out_dir.join(format!("{}.pdf", mirror.stem)))
            .map(Output::Pdf)
            .map_err(|e| format!("cannot read the PDF: {e}"))
    }
}

/// 隠しファイルの名前（拡張子つき）
fn mirror_name(source: &Path) -> Option<String> {
    Some(format!("{MIRROR_PREFIX}{}.tex", source.file_stem()?.to_str()?))
}

/// コンパイルのために書いた隠しファイル。使い終わったら（`Drop` で）消す
struct Mirror {
    path: PathBuf,
    stem: String,
}

impl Mirror {
    fn write(source: &Path, text: &str) -> Result<Self, String> {
        let name = mirror_name(source).ok_or("invalid file name")?;
        let stem = name.trim_end_matches(".tex").to_owned();
        let beside = source.parent().unwrap_or(Path::new(".")).join(&name);
        if fs::write(&beside, text).is_ok() {
            return Ok(Self { path: beside, stem });
        }
        // フォルダに書き込めないとき（読み取り専用など）は、一時フォルダに書く。作業フォルダは元のままなので、相対パスは解決される
        let dir = out_dir_for(source).join("buffer");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(&name);
        fs::write(&path, text).map_err(|e| format!("cannot write the unsaved text: {e}"))?;
        Ok(Self { path, stem })
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// 異常終了で残った古い隠しファイルを消す
fn remove_stale_mirrors(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with(MIRROR_PREFIX) && name.ends_with(".tex")) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_MIRROR_AGE);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 直近のコンパイル（保存した文書か、保存前の本文）の SyncTeX を読む
fn read_synctex(path: &Path) -> Result<SyncTex, String> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or("invalid file name")?;
    let out_dir = out_dir_for(path);
    let mirror_stem = mirror_name(path).map(|n| n.trim_end_matches(".tex").to_owned()).unwrap_or_default();
    let newest = [stem.to_owned(), mirror_stem]
        .into_iter()
        .map(|s| out_dir.join(format!("{s}.synctex.gz")))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
        .unwrap_or_else(|| out_dir.join(format!("{stem}.synctex.gz")));
    SyncTex::read(&newest).map_err(|_| "no SyncTeX data; reload and try again".to_string())
}

/// SyncTeX の入力ファイル名 `input` が、`source` を指すか（隠しファイルも、元のファイルとして扱う）
fn same_file(source: &Path, input: &str) -> bool {
    fn clean(path: &Path) -> PathBuf {
        path.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect()
    }
    clean(&resolve_input(source, input)) == clean(source)
}

fn out_dir_for(path: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    std::env::temp_dir()
        .join(format!("washi-{}", std::process::id()))
        .join(format!("{:016x}", hasher.finish()))
}

fn resolve_input(source: &Path, input: &str) -> PathBuf {
    let input = Path::new(input);
    // 保存前の本文をコンパイルしたときの隠しファイルは、元のファイルとして扱う
    if input.file_name().and_then(|n| n.to_str()) == mirror_name(source).as_deref() {
        return source.to_path_buf();
    }
    if input.is_absolute() {
        return input.to_path_buf();
    }
    source.parent().unwrap_or(Path::new(".")).join(input)
}

pub struct SystemTexEngine;

impl TexEngine for SystemTexEngine {
    fn compile(&self, source: &Path, out_dir: &Path, cwd: &Path) -> Result<(), String> {
        let (tool, mut command) = command_for(source, out_dir).ok_or(
            "neither latexmk nor tectonic was found; install one, for example with `brew install tectonic`",
        )?;
        // 同じ文書の新しい描画が始まったら、この描画は打ち切られる
        let job = Job::start(source);
        let timeout = process::timeout_from_env();
        let output = process::run(command.current_dir(cwd), timeout, job.cancelled()).map_err(|e| match e {
            RunError::Spawn(e) => format!("cannot start {tool}: {e}"),
            RunError::TimedOut(limit) => format!(
                "{tool} was stopped after {} seconds; set {} to change the limit",
                limit.as_secs(),
                process::TIMEOUT_ENV,
            ),
            RunError::Cancelled => "stopped because a newer render replaced it".to_string(),
        })?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "{tool} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            tail(&String::from_utf8_lossy(&output.stdout), 4000),
        ))
    }
}

fn command_for(source: &Path, out_dir: &Path) -> Option<(&'static str, Command)> {
    if let Some(latexmk) = find_tool("latexmk") {
        let mut command = Command::new(latexmk);
        command
            .args(["-pdf", "-synctex=1", "-interaction=nonstopmode", "-halt-on-error", "-outdir"])
            .arg(out_dir)
            .arg(source);
        return Some(("latexmk", command));
    }
    let tectonic: PathBuf = find_tool("tectonic")?;
    let mut command = Command::new(tectonic);
    command
        .args(["-X", "compile", "--synctex", "--outdir"])
        .arg(out_dir)
        .arg(source);
    Some(("tectonic", command))
}

fn tail(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeEngine(Result<(), String>);

    impl TexEngine for FakeEngine {
        fn compile(&self, source: &Path, out_dir: &Path, _cwd: &Path) -> Result<(), String> {
            self.0.clone()?;
            let stem = source.file_stem().unwrap().to_str().unwrap();
            fs::write(out_dir.join(format!("{stem}.pdf")), b"%PDF-fake").unwrap();
            Ok(())
        }
    }

    #[test]
    fn returns_the_pdf_the_engine_produced() {
        let renderer = TexRenderer(FakeEngine(Ok(())));
        match renderer.render(Path::new("/tmp/washi-fake-doc.tex")).unwrap() {
            Output::Pdf(bytes) => assert_eq!(bytes, b"%PDF-fake"),
            Output::Html(_) => panic!("PDF を期待"),
        }
    }

    #[test]
    fn propagates_engine_errors() {
        let renderer = TexRenderer(FakeEngine(Err("boom".into())));
        let err = renderer.render(Path::new("/tmp/x.tex")).err().unwrap();
        assert_eq!(err, "boom");
    }

    #[test]
    fn output_directories_differ_per_source_file() {
        let a = out_dir_for(Path::new("/x/a.tex"));
        assert_ne!(a, out_dir_for(Path::new("/y/a.tex")));
        assert_eq!(a, out_dir_for(Path::new("/x/a.tex")));
    }

    #[test]
    fn relative_inputs_resolve_against_the_source_directory() {
        assert_eq!(resolve_input(Path::new("/d/main.tex"), "chap/one.tex"), PathBuf::from("/d/chap/one.tex"));
        assert_eq!(resolve_input(Path::new("/d/main.tex"), "/abs/x.tex"), PathBuf::from("/abs/x.tex"));
    }

    #[test]
    fn locate_reports_missing_synctex_data() {
        let renderer = TexRenderer(FakeEngine(Ok(())));
        assert!(renderer.locate(Path::new("/nowhere/never.tex"), 1, 10.0, 10.0).is_err());
    }

    #[test]
    fn tail_respects_char_boundaries() {
        assert_eq!(tail("あいうえお", 7), "えお");
        assert_eq!(tail("abc", 10), "abc");
    }

    // PATH と環境変数を書き換えるので、他のテストと並走しないよう通常は除外する
    // 実行: cargo test -p washi-core hung_engine -- --ignored
    #[test]
    #[cfg(unix)]
    #[ignore = "PATH と WASHI_COMPILE_TIMEOUT を書き換える"]
    fn hung_engine_is_stopped_by_the_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("washi-fake-latexmk-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("latexmk");
        fs::write(&fake, "#!/bin/sh\nmarker=\"$(dirname \"$0\")/ran\"\n[ -e \"$marker\" ] && exit 0\ntouch \"$marker\"\nsleep 60 & wait\n").unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();

        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![dir.clone()];
        dirs.extend(std::env::split_paths(&path));
        std::env::set_var("PATH", std::env::join_paths(dirs).unwrap());
        std::env::set_var(process::TIMEOUT_ENV, "1");

        let started = std::time::Instant::now();
        let err = SystemTexEngine.compile(Path::new("/tmp/washi-hung.tex"), &dir, Path::new("/tmp")).unwrap_err();
        assert!(err.contains("after 1 seconds"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));

        // 2 回目以降の偽エンジンはすぐ成功する。1 回目の実行中に同じ文書の描画が始まると、1 回目が打ち切られる
        fs::remove_file(dir.join("ran")).unwrap();
        std::env::set_var(process::TIMEOUT_ENV, "30");
        let source = PathBuf::from("/tmp/washi-hung-cancel.tex");
        let first = {
            let (dir, source) = (dir.clone(), source.clone());
            std::thread::spawn(move || SystemTexEngine.compile(&source, &dir, Path::new("/tmp")))
        };
        std::thread::sleep(std::time::Duration::from_millis(500));
        let started = std::time::Instant::now();
        SystemTexEngine.compile(&source, &dir, Path::new("/tmp")).unwrap();
        let err = first.join().unwrap().unwrap_err();
        assert!(err.contains("newer render"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        fs::remove_dir_all(&dir).ok();
    }

    use std::sync::Mutex;

    /// 渡されたファイルと作業フォルダ、その時点のファイルの中身を覚えて、`<名前>.pdf` を書く偽のエンジン
    #[derive(Default)]
    struct SpyEngine {
        calls: Mutex<Vec<(PathBuf, PathBuf, String)>>,
    }

    impl TexEngine for SpyEngine {
        fn compile(&self, source: &Path, out_dir: &Path, cwd: &Path) -> Result<(), String> {
            let body = fs::read_to_string(source).unwrap_or_default();
            self.calls.lock().unwrap().push((source.to_path_buf(), cwd.to_path_buf(), body));
            let stem = source.file_stem().unwrap().to_str().unwrap();
            fs::write(out_dir.join(format!("{stem}.pdf")), b"%PDF-spy").unwrap();
            Ok(())
        }
    }

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("washi-tex-buf-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_buffer_is_compiled_from_a_hidden_sibling_in_the_documents_folder() {
        let dir = project("sibling");
        let path = dir.join("paper.tex");
        let renderer = TexRenderer(SpyEngine::default());
        let rendered = renderer.render_buffer(&path, "\\input{chapters/one}");
        match rendered.output.unwrap() {
            Output::Pdf(bytes) => assert_eq!(bytes, b"%PDF-spy"),
            Output::Html(_) => panic!("PDF を期待"),
        }
        let calls = renderer.0.calls.lock().unwrap();
        let (source, cwd, body) = &calls[0];
        assert_eq!(source, &dir.join(".washi-buf-paper.tex"));
        assert_eq!(cwd, &dir, "作業フォルダは元のフォルダ");
        assert_eq!(body, "\\input{chapters/one}", "コンパイル時のファイルの中身は、保存前の本文");
        assert!(!dir.join(".washi-buf-paper.tex").exists(), "終わったら消す");
        assert!(!path.exists(), "本物のファイルには何も書かない");
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_folder_falls_back_to_a_temp_file_but_keeps_the_working_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project("readonly");
        let path = dir.join("paper.tex");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let renderer = TexRenderer(SpyEngine::default());
        let rendered = renderer.render_buffer(&path, "body");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(rendered.output.is_ok(), "{:?}", rendered.output.err());
        let calls = renderer.0.calls.lock().unwrap();
        let (source, cwd, body) = &calls[0];
        assert_eq!(body, "body");
        assert_eq!(cwd, &dir);
        if source.starts_with(&dir) {
            // root などで、読み取り専用でも書けた環境
            return;
        }
        assert!(source.starts_with(out_dir_for(&path)), "{source:?}");
        assert!(!source.exists(), "終わったら消す");
    }

    #[test]
    fn the_hidden_sibling_is_mapped_back_to_the_real_file() {
        let real = Path::new("/proj/paper.tex");
        assert_eq!(resolve_input(real, "/proj/.washi-buf-paper.tex"), real);
        assert_eq!(resolve_input(real, ".washi-buf-paper.tex"), real);
        assert_eq!(resolve_input(real, "chapters/one.tex"), PathBuf::from("/proj/chapters/one.tex"));
        assert!(same_file(real, "/proj/.washi-buf-paper.tex"));
        assert!(same_file(real, "./paper.tex"));
        assert!(!same_file(real, "chapters/paper.tex"));
    }

    #[test]
    fn only_old_hidden_siblings_are_swept() {
        let dir = project("sweep");
        let old = dir.join(".washi-buf-old.tex");
        let fresh = dir.join(".washi-buf-fresh.tex");
        let unrelated = dir.join(".washi-keep.tex");
        for f in [&old, &fresh, &unrelated] {
            fs::write(f, "x").unwrap();
        }
        let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
        fs::File::options().write(true).open(&old).unwrap().set_modified(two_hours_ago).unwrap();
        fs::File::options().write(true).open(&unrelated).unwrap().set_modified(two_hours_ago).unwrap();
        remove_stale_mirrors(&dir);
        assert!(!old.exists() && fresh.exists() && unrelated.exists());
    }

    #[test]
    fn forward_search_reads_the_synctex_of_the_latest_buffer_compile() {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;
        let dir = project("forward");
        let path = dir.join("paper.tex");
        let out_dir = out_dir_for(&path);
        fs::create_dir_all(&out_dir).unwrap();
        let text = format!(
            "SyncTeX Version:1\nInput:1:{}\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n!100\n{{1\n[1,68:4736287,52685372:29760291,47949085,0\n(1,10:4736287,8000000:29760291,800000,200000\ng1,10:9000000,8000000\n)\n]\n}}1\n",
            dir.join(".washi-buf-paper.tex").display()
        );
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(text.as_bytes()).unwrap();
        fs::write(out_dir.join(".washi-buf-paper.synctex.gz"), gz.finish().unwrap()).unwrap();

        let renderer = TexRenderer(SpyEngine::default());
        let position = renderer.locate_forward(&path, 10, 1).unwrap().expect("位置が出る");
        assert_eq!(position.page, 1);
        let back = renderer.locate(&path, 1, position.x + 1.0, position.y + 1.0).unwrap().expect("後方検索");
        assert_eq!((back.file, back.line), (path, 10), "隠しファイルは元のファイルとして返る");
    }

    #[test]
    fn buffer_dependency_dirs_follow_the_unsaved_text() {
        let dir = project("deps");
        fs::create_dir_all(dir.join("chapters")).unwrap();
        let renderer = TexRenderer(SpyEngine::default());
        assert_eq!(renderer.buffer_dependency_dirs(&dir.join("paper.tex"), "\\input{chapters/one}"), vec![dir.join("chapters")]);
    }
}
