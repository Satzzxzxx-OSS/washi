use std::path::Path;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use washi_core::render;

use crate::state::{Documents, PendingFiles};

pub const OPEN_EVENT: &str = "washi://open";

const MAIN_WINDOW: &str = "main";
const WINDOW_SIZE: (f64, f64) = (900.0, 1000.0);

pub fn openable(path: &Path) -> Option<String> {
    render::renderer_for(path).map(|_| path.to_string_lossy().into_owned())
}

pub fn openable_paths<I, P>(paths: I) -> Vec<String>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    paths.into_iter().filter_map(|p| openable(p.as_ref())).collect()
}

pub fn open_documents(app: &AppHandle, paths: Vec<String>) {
    for path in paths {
        open_document(app, path);
    }
}

fn open_document(app: &AppHandle, path: String) {
    let documents = app.state::<Documents>();
    if let Some(label) = documents.label_of(&path) {
        focus(app, &label);
        return;
    }
    let pending = app.state::<PendingFiles>();
    let Some(label) = idle_window(app, &documents, &pending).or_else(|| create_window(app)) else {
        return;
    };
    pending.set(&label, path);
    let _ = app.emit_to(label.as_str(), OPEN_EVENT, ());
    focus(app, &label);
}

fn idle_window(app: &AppHandle, documents: &Documents, pending: &PendingFiles) -> Option<String> {
    let mut labels: Vec<String> = app.webview_windows().into_keys().collect();
    labels.sort_by_key(|l| l != MAIN_WINDOW);
    labels
        .into_iter()
        .find(|l| !documents.has(l) && !pending.has(l))
}

fn create_window(app: &AppHandle) -> Option<String> {
    let label = format!("doc-{}", next_serial());
    WebviewWindowBuilder::new(app, &label, WebviewUrl::default())
        .title("Washi")
        .inner_size(WINDOW_SIZE.0, WINDOW_SIZE.1)
        .min_inner_size(480.0, 400.0)
        .build()
        .ok()
        .map(|_| label)
}

fn next_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

fn focus(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_supported_paths_in_order() {
        assert_eq!(
            openable_paths(["--flag", "notes.txt", "a.md", "b.typ", "c.docx"]),
            vec!["a.md", "b.typ"]
        );
        assert!(openable_paths(["x.docx"]).is_empty());
    }

    #[test]
    fn serials_are_unique() {
        assert_ne!(next_serial(), next_serial());
    }
}
