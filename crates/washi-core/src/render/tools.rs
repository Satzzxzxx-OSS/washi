use std::path::{Path, PathBuf};

const FALLBACK_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/Library/TeX/texbin"];

pub fn find_tool(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(FALLBACK_DIRS.iter().map(PathBuf::from));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".cargo/bin"));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}
