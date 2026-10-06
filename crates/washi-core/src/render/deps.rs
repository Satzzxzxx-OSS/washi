//! 文書が読み込む他のファイル（章・参考文献・画像など）のあるフォルダを、ソースから調べる。
//! 保存を監視するフォルダを増やすために使う。エンジンが実際に読んだ一覧ではなく、ソースの静的な走査なので、
//! マクロで組み立てたパスは拾えない。

use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

use comrak::{nodes::NodeValue, parse_document, Arena};
use regex::Regex;

/// たどるファイル数の上限（循環した include や巨大な文書で止まらなくなるのを防ぐ）
const MAX_FILES: usize = 64;

/// 見つかった依存のフォルダ（存在するものだけ、重複なし）。ルートや HOME のような広すぎるフォルダは除く。
fn directories(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in paths {
        let dir = normalize(&dir);
        let too_broad = dir.parent().is_none() || home.as_deref() == Some(dir.as_path());
        if too_broad || !dir.is_dir() || !seen.insert(dir.clone()) {
            continue;
        }
        out.push(dir);
    }
    out
}

/// `.` と `..` を字句的にたたむ（シンボリックリンクは解決しない）
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn is_remote(target: &str) -> bool {
    target.contains("://") || target.starts_with("data:") || target.starts_with("mailto:")
}

// ---- Markdown ----

pub fn markdown(source: &str, base: &Path) -> Vec<PathBuf> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &comrak::Options::default());
    let mut dirs = Vec::new();
    for node in root.descendants() {
        if let NodeValue::Image(link) = &node.data.borrow().value {
            let url = link.url.split(['#', '?']).next().unwrap_or("");
            if url.is_empty() || is_remote(url) {
                continue;
            }
            if let Some(parent) = base.join(url).parent() {
                dirs.push(parent.to_path_buf());
            }
        }
    }
    directories(dirs)
}

// ---- LaTeX ----

static TEX_COMMAND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(input|include|subfile|bibliography|addbibresource|includegraphics|includepdf|lstinputlisting|verbatiminput|usepackage|RequirePackage|documentclass)\*?\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}",
    )
    .unwrap()
});
static TEX_GRAPHICSPATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\graphicspath\s*\{((?:\s*\{[^}]*\}\s*)+)\}").unwrap());
static BRACED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{([^}]*)\}").unwrap());

fn strip_tex_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let mut escaped = false;
            for (i, c) in line.char_indices() {
                match c {
                    '\\' => escaped = !escaped,
                    '%' if !escaped => return &line[..i],
                    _ => escaped = false,
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 主ファイルで、保存前の本文があればそれを、そうでなければディスクのファイルを読む
fn read_source(file: &Path, main: &Path, main_text: Option<&str>) -> Option<String> {
    match main_text {
        Some(text) if file == main => Some(text.to_owned()),
        _ => fs::read_to_string(file).ok(),
    }
}

fn rooted(root: &Path, target: &str) -> PathBuf {
    let target = Path::new(target.trim());
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    }
}

/// `\input{chapters/intro}` のように拡張子が無ければ `.tex` を補う
fn tex_file(root: &Path, target: &str) -> PathBuf {
    let path = rooted(root, target);
    if path.extension().is_some_and(|e| e == "tex") {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".tex");
        PathBuf::from(name)
    }
}

pub fn latex(main: &Path) -> Vec<PathBuf> {
    latex_text(main, None)
}

/// `main_text` は、保存前の本文。あれば、ディスクの主ファイルの代わりに走査する
pub fn latex_text(main: &Path, main_text: Option<&str>) -> Vec<PathBuf> {
    // TeX は読み込み先を、最初に開いた文書のあるフォルダを基準に解決する
    let root = main.parent().unwrap_or(Path::new("."));
    let mut dirs = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![main.to_path_buf()];

    while let Some(file) = queue.pop() {
        if visited.len() >= MAX_FILES || !visited.insert(file.clone()) {
            continue;
        }
        let Some(source) = read_source(&file, main, main_text) else { continue };
        let source = strip_tex_comments(&source);

        for caps in TEX_COMMAND.captures_iter(&source) {
            let command = &caps[1];
            for name in caps[2].split(',').map(str::trim).filter(|n| !n.is_empty()) {
                match command {
                    "input" | "include" | "subfile" => {
                        let path = tex_file(root, name);
                        dirs.extend(path.parent().map(Path::to_path_buf));
                        queue.push(path);
                    }
                    // 自作のスタイルやクラスは、同じフォルダにあるときだけ関係する
                    "usepackage" | "RequirePackage" | "documentclass" => {
                        for ext in ["sty", "cls"] {
                            let path = rooted(root, &format!("{name}.{ext}"));
                            if path.is_file() {
                                dirs.extend(path.parent().map(Path::to_path_buf));
                            }
                        }
                    }
                    _ => dirs.extend(rooted(root, name).parent().map(Path::to_path_buf)),
                }
            }
        }
        for caps in TEX_GRAPHICSPATH.captures_iter(&source) {
            for dir in BRACED.captures_iter(&caps[1]) {
                dirs.push(rooted(root, &dir[1]));
            }
        }
    }
    directories(dirs)
}

// ---- Typst ----

static TYPST_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\b(include|import|image|bibliography|read|csv|json|yaml|toml|xml|cbor|pdf)\s*\(?\s*(?:\w+\s*:\s*)?"([^"]+)""#,
    )
    .unwrap()
});

pub fn typst(main: &Path) -> Vec<PathBuf> {
    typst_text(main, None)
}

/// `main_text` は、保存前の本文。あれば、ディスクの主ファイルの代わりに走査する
pub fn typst_text(main: &Path, main_text: Option<&str>) -> Vec<PathBuf> {
    // Typst は `/` から始まるパスをプロジェクトのルート（開いた文書のフォルダ）から、
    // それ以外を読み込んでいるファイルのフォルダから解決する
    let root = main.parent().unwrap_or(Path::new("."));
    let mut dirs = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![main.to_path_buf()];

    while let Some(file) = queue.pop() {
        if visited.len() >= MAX_FILES || !visited.insert(file.clone()) {
            continue;
        }
        let Some(source) = read_source(&file, main, main_text) else { continue };
        let here = file.parent().unwrap_or(root);

        for caps in TYPST_PATH.captures_iter(&source) {
            let target = &caps[2];
            if target.starts_with('@') || is_remote(target) {
                continue;
            }
            let path = match target.strip_prefix('/') {
                Some(rest) => root.join(rest),
                None => here.join(target),
            };
            dirs.extend(path.parent().map(Path::to_path_buf));
            if matches!(&caps[1], "include" | "import") && path.extension().is_some_and(|e| e == "typ") {
                queue.push(path);
            }
        }
    }
    directories(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Project(PathBuf);

    impl Project {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("washi-deps-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn dir(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sorted(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
        v.sort();
        v
    }

    #[test]
    fn latex_finds_chapters_bibliography_and_figures() {
        let p = Project::new("tex");
        p.file("chapters/intro.tex", "intro");
        p.file("refs/refs.bib", "");
        p.file("figs/a.png", "");
        let main = p.file(
            "main.tex",
            "\\input{chapters/intro}\n\\bibliography{refs/refs}\n\\includegraphics[width=3cm]{figs/a.png}\n",
        );
        assert_eq!(
            sorted(latex(&main)),
            sorted(vec![p.dir("chapters"), p.dir("refs"), p.dir("figs")])
        );
    }

    #[test]
    fn latex_follows_nested_inputs_relative_to_the_main_file() {
        let p = Project::new("nested");
        p.file("a/one.tex", "\\input{b/two}");
        p.file("b/two.tex", "\\addbibresource{c/x.bib}");
        p.file("c/x.bib", "");
        let main = p.file("main.tex", "\\include{a/one}");
        assert_eq!(sorted(latex(&main)), sorted(vec![p.dir("a"), p.dir("b"), p.dir("c")]));
    }

    #[test]
    fn latex_ignores_comments_and_handles_lists_and_graphicspath() {
        let p = Project::new("misc");
        p.file("real/x.tex", "");
        p.file("gone/x.tex", "");
        p.file("img/a.png", "");
        p.file("refs/a.bib", "");
        p.file("refs/b.bib", "");
        let main = p.file(
            "main.tex",
            "% \\input{gone/x}\n\\input{real/x} 100\\% \\bibliography{refs/a, refs/b}\n\\graphicspath{{img/}}\n",
        );
        assert_eq!(sorted(latex(&main)), sorted(vec![p.dir("real"), p.dir("refs"), p.dir("img")]));
    }

    #[test]
    fn latex_uses_local_packages_only_when_they_exist() {
        let p = Project::new("sty");
        p.file("sty/mystyle.sty", "");
        let main = p.file("main.tex", "\\usepackage{amsmath}\n\\usepackage{sty/mystyle}\n");
        assert_eq!(latex(&main), vec![p.dir("sty")]);
    }

    #[test]
    fn latex_survives_cycles_and_missing_files() {
        let p = Project::new("cycle");
        p.file("a/a.tex", "\\input{b/b}");
        p.file("b/b.tex", "\\input{a/a}\n\\input{nowhere/x}");
        let main = p.file("main.tex", "\\input{a/a}");
        assert_eq!(sorted(latex(&main)), sorted(vec![p.dir("a"), p.dir("b")]));
    }

    #[test]
    fn a_dependency_in_the_documents_own_folder_is_reported_once() {
        let p = Project::new("flat");
        p.file("other.tex", "");
        let main = p.file("main.tex", "\\input{other}");
        assert_eq!(latex(&main), vec![p.0.clone()]);
    }

    #[test]
    fn typst_resolves_relative_paths_from_the_including_file() {
        let p = Project::new("typ");
        p.file("parts/one.typ", "#include \"deep/two.typ\"\n#image(\"pics/x.png\")");
        p.file("parts/deep/two.typ", "#bibliography(\"../../refs/r.bib\")");
        p.file("parts/pics/x.png", "");
        p.file("refs/r.bib", "");
        let main = p.file("main.typ", "#include \"parts/one.typ\"\n#import \"@preview/cetz:0.3.0\": canvas\n");
        assert_eq!(
            sorted(typst(&main)),
            sorted(vec![p.dir("parts"), p.dir("parts/deep"), p.dir("parts/pics"), p.dir("refs")])
        );
    }

    #[test]
    fn typst_slash_paths_start_at_the_project_root() {
        let p = Project::new("typroot");
        p.file("chapters/c.typ", "#image(\"/assets/logo.svg\")");
        p.file("assets/logo.svg", "");
        let main = p.file("main.typ", "#include \"chapters/c.typ\"");
        assert_eq!(sorted(typst(&main)), sorted(vec![p.dir("chapters"), p.dir("assets")]));
    }

    #[test]
    fn typst_reads_data_files_and_ignores_remote_and_packages() {
        let p = Project::new("typdata");
        p.file("data/t.csv", "");
        let main = p.file(
            "main.typ",
            "#let t = csv(\"data/t.csv\")\n#image(\"https://example.com/x.png\")\n#import \"@local/x:1.0.0\": y\n",
        );
        assert_eq!(typst(&main), vec![p.dir("data")]);
    }

    #[test]
    fn typst_survives_cycles() {
        let p = Project::new("typcycle");
        p.file("a/a.typ", "#include \"../b/b.typ\"");
        p.file("b/b.typ", "#include \"../a/a.typ\"");
        let main = p.file("main.typ", "#include \"a/a.typ\"");
        assert_eq!(sorted(typst(&main)), sorted(vec![p.dir("a"), p.dir("b")]));
    }

    #[test]
    fn markdown_watches_the_folders_of_local_images_only() {
        let p = Project::new("md");
        p.file("assets/a.png", "");
        let md = "![a](assets/a.png)\n![b](https://example.com/b.png)\n![c](data:image/png;base64,AAAA)\n[link](other.md)\n";
        assert_eq!(markdown(md, &p.0), vec![p.dir("assets")]);
    }

    #[test]
    fn broad_folders_are_never_watched() {
        assert!(directories(vec![PathBuf::from("/")]).is_empty());
        if let Some(home) = std::env::var_os("HOME") {
            assert!(directories(vec![PathBuf::from(home)]).is_empty());
        }
    }

    #[test]
    fn dot_segments_are_folded_and_duplicates_removed() {
        let p = Project::new("norm");
        p.file("a/x", "");
        let dirs = directories(vec![p.dir("a/../a"), p.dir("a"), p.dir("a/.")]);
        assert_eq!(dirs, vec![p.dir("a")]);
    }

    #[test]
    fn latex_scans_the_unsaved_text_instead_of_the_file_on_disk() {
        let p = Project::new("tex-buffer");
        p.file("chapters/one.tex", "");
        let main = p.file("main.tex", "nothing yet");
        assert!(latex(&main).is_empty() || latex(&main) == vec![p.0.clone()]);
        assert_eq!(latex_text(&main, Some("\\input{chapters/one}")), vec![p.dir("chapters")]);
    }

    #[test]
    fn typst_scans_the_unsaved_text_instead_of_the_file_on_disk() {
        let p = Project::new("typ-buffer");
        p.file("parts/a.typ", "");
        let main = p.file("main.typ", "= plain");
        assert_eq!(typst_text(&main, Some("#include \"parts/a.typ\"")), vec![p.dir("parts")]);
    }
}
