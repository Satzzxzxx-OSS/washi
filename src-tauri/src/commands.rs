use std::path::PathBuf;

use serde::Serialize;
use tauri::{ipc::Response, AppHandle, State, WebviewWindow};
use washi_core::{
    editor,
    files::{self, DiskText, SaveResult},
    render,
};

use crate::{
    state::{DirtyWindows, Documents, PendingFiles},
    watch::FileWatcher,
};

#[tauri::command]
pub fn supported_extensions() -> Vec<&'static str> {
    render::supported_extensions()
}

#[tauri::command]
pub fn initial_file(window: WebviewWindow, pending: State<PendingFiles>) -> Option<String> {
    pending.take(window.label())
}

#[tauri::command]
pub fn print(window: WebviewWindow) -> Result<(), String> {
    window.print().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn render(
    window: WebviewWindow,
    watcher: State<'_, FileWatcher>,
    path: String,
) -> Result<Response, String> {
    let (output, dirs) = tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        let output = render::render(&path);
        // 描画に失敗しても調べる（読み込み先のファイルを直せば、また描画できるように）
        (output, render::dependency_dirs(&path))
    })
    .await
    .map_err(|e| e.to_string())?;
    watcher.set_dependencies(window.label(), &dirs);
    Ok(Response::new(output?.into_wire()))
}

/// 保存前の本文を描画する。応答は `Rendered::into_wire` の形式（診断つき。失敗しても診断は返る）
#[tauri::command]
pub async fn render_buffer(
    window: WebviewWindow,
    watcher: State<'_, FileWatcher>,
    path: String,
    text: String,
) -> Result<Response, String> {
    let (rendered, dirs) = tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        let rendered = render::render_buffer(&path, &text);
        (rendered, render::buffer_dependency_dirs(&path, &text))
    })
    .await
    .map_err(|e| e.to_string())?;
    watcher.set_dependencies(window.label(), &dirs);
    Ok(Response::new(rendered.into_wire()))
}

/// カーソル位置（UTF-16）での補完（Typst）
#[tauri::command]
pub async fn autocomplete(path: String, text: String, offset: usize, explicit: bool) -> Result<render::Completions, String> {
    tauri::async_runtime::spawn_blocking(move || render::complete(&PathBuf::from(path), &text, offset, explicit))
        .await
        .map_err(|e| e.to_string())?
}

/// ソースの行・列（1 始まり）に対応する、プレビュー上の位置（前方検索）
#[tauri::command]
pub async fn forward_locate(path: String, line: u32, column: u32) -> Result<Option<render::PreviewPosition>, String> {
    tauri::async_runtime::spawn_blocking(move || render::locate_forward(&PathBuf::from(path), line, column))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct LocatedSource {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// プレビューの位置に対応するソースの位置。外部のエディタは起動しない（アプリ内のエディタが受け取る）
#[tauri::command]
pub async fn locate_source(path: String, page: usize, x: f64, y: f64) -> Result<Option<LocatedSource>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(render::locate(&PathBuf::from(path), page, x, y)?.map(|l| LocatedSource {
            file: l.file.to_string_lossy().into_owned(),
            line: l.line,
            column: l.column,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn read_text(path: String) -> Result<DiskText, String> {
    tauri::async_runtime::spawn_blocking(move || files::read_text(&PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
}

/// 保存する。**このウィンドウが開いているファイルにしか書かない**（画面側が壊れても、任意のファイルは書けない）。
/// `base_hash` は、読み込んだときのハッシュ。ディスクが変わっていれば `conflict` を返し、`force` のときだけ上書きする
#[tauri::command]
pub async fn write_file(
    window: WebviewWindow,
    documents: State<'_, Documents>,
    path: String,
    text: String,
    base_hash: Option<String>,
    force: Option<bool>,
) -> Result<SaveResult, String> {
    if !documents.owns(window.label(), &path) {
        return Err("Can only save the file this window has open".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        files::write_file(&PathBuf::from(path), &text, base_hash.as_deref(), force.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 未保存の変更があるかを、Rust 側にも伝える（⌘Q のときに、確認を出すため）
#[tauri::command]
pub fn set_dirty(window: WebviewWindow, dirty_windows: State<DirtyWindows>, dirty: bool) {
    dirty_windows.set(window.label(), dirty);
}

#[tauri::command]
pub async fn render_text(text: String) -> Result<Response, String> {
    let output = tauri::async_runtime::spawn_blocking(move || render::render_text(&text))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Response::new(output.into_wire()))
}

#[tauri::command]
pub async fn jump_to_source(path: String, page: usize, x: f64, y: f64) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(location) = render::locate(&PathBuf::from(&path), page, x, y)? else {
            return Ok(None);
        };
        editor::open(&location)?;
        let name = location.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Some(format!("{name}:{}", location.line)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn watch(
    app: AppHandle,
    window: WebviewWindow,
    watcher: State<FileWatcher>,
    documents: State<Documents>,
    path: String,
) -> Result<(), String> {
    documents.open(window.label(), &path);
    watcher.watch(app, window.label(), &PathBuf::from(path))
}
