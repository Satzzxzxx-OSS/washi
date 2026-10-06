use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    process::{self, Job, RunError},
    synctex::SyncTex,
    tools::find_tool,
    Output, Renderer, SourceLocation,
};

pub trait TexEngine: Sync {
    fn compile(&self, source: &Path, out_dir: &Path) -> Result<(), String>;
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
            .ok_or("ファイル名が不正です")?;
        let out_dir = out_dir_for(path);
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

        self.0.compile(path, &out_dir)?;

        fs::read(out_dir.join(format!("{stem}.pdf")))
            .map(Output::Pdf)
            .map_err(|e| format!("PDF を読めません: {e}"))
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

    fn locate(&self, path: &Path, page: usize, x: f64, y: f64) -> Result<Option<SourceLocation>, String> {
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or("ファイル名が不正です")?;
        let data = out_dir_for(path).join(format!("{stem}.synctex.gz"));
        let synctex = SyncTex::read(&data).map_err(|_| "SyncTeX の情報がありません。再読み込みしてください".to_string())?;
        let Some(hit) = synctex.inverse(page as u32, x, y) else {
            return Ok(None);
        };
        let file = resolve_input(path, &hit.input);
        Ok(Some(SourceLocation { file, line: hit.line as usize, column: 1 }))
    }
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
    if input.is_absolute() {
        return input.to_path_buf();
    }
    source.parent().unwrap_or(Path::new(".")).join(input)
}

pub struct SystemTexEngine;

impl TexEngine for SystemTexEngine {
    fn compile(&self, source: &Path, out_dir: &Path) -> Result<(), String> {
        let (tool, mut command) = command_for(source, out_dir).ok_or(
            "latexmk も tectonic も見つかりません。`brew install tectonic` などで入れてください",
        )?;
        let dir = source.parent().unwrap_or(Path::new("."));
        // 同じ文書の新しい描画が始まったら、この描画は打ち切られる
        let job = Job::start(source);
        let timeout = process::timeout_from_env();
        let output = process::run(command.current_dir(dir), timeout, job.cancelled()).map_err(|e| match e {
            RunError::Spawn(e) => format!("{tool} を起動できません: {e}"),
            RunError::TimedOut(limit) => format!(
                "{tool} が {} 秒を超えたため中断しました。{} で秒数を変えられます",
                limit.as_secs(),
                process::TIMEOUT_ENV,
            ),
            RunError::Cancelled => "新しい描画に置き換えられたため中断しました".to_string(),
        })?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "{tool} が失敗しました:\n{}\n{}",
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
        fn compile(&self, source: &Path, out_dir: &Path) -> Result<(), String> {
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
        let err = SystemTexEngine.compile(Path::new("/tmp/washi-hung.tex"), &dir).unwrap_err();
        assert!(err.contains("1 秒を超えた"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));

        // 2 回目以降の偽エンジンはすぐ成功する。1 回目の実行中に同じ文書の描画が始まると、1 回目が打ち切られる
        fs::remove_file(dir.join("ran")).unwrap();
        std::env::set_var(process::TIMEOUT_ENV, "30");
        let source = PathBuf::from("/tmp/washi-hung-cancel.tex");
        let first = {
            let (dir, source) = (dir.clone(), source.clone());
            std::thread::spawn(move || SystemTexEngine.compile(&source, &dir))
        };
        std::thread::sleep(std::time::Duration::from_millis(500));
        let started = std::time::Instant::now();
        SystemTexEngine.compile(&source, &dir).unwrap();
        let err = first.join().unwrap().unwrap_err();
        assert!(err.contains("置き換え"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        fs::remove_dir_all(&dir).ok();
    }
}
