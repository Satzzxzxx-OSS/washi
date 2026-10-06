use std::path::PathBuf;

use tauri::{ipc::Response, AppHandle, State, WebviewWindow};
use washi_core::{editor, render};

use crate::{
    state::{Documents, PendingFiles},
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
