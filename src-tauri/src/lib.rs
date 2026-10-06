mod commands;
mod launch;
mod menu;
mod state;
mod watch;

use tauri::{Manager, WindowEvent};

use state::{Documents, PendingFiles};
use watch::FileWatcher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let cwd = std::path::PathBuf::from(cwd);
            let paths = argv.iter().skip(1).map(|a| cwd.join(a));
            launch::open_documents(app, launch::openable_paths(paths));
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(PendingFiles::default())
        .manage(Documents::default())
        .manage(FileWatcher::default())
        .invoke_handler(tauri::generate_handler![
            commands::supported_extensions,
            commands::initial_file,
            commands::jump_to_source,
            commands::print,
            commands::render,
            commands::render_text,
            commands::render_buffer,
            commands::autocomplete,
            commands::forward_locate,
            commands::locate_source,
            commands::read_text,
            commands::write_file,
            commands::watch,
        ])
        .menu(|app| menu::build(app))
        .on_menu_event(|app, event| menu::dispatch(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                let label = window.label();
                window.state::<FileWatcher>().forget(label);
                window.state::<Documents>().close(label);
            }
        })
        .setup(|app| {
            launch::open_documents(app.handle(), launch::openable_paths(std::env::args().skip(1)));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = &event {
            let paths = urls.iter().filter_map(|u| u.to_file_path().ok());
            launch::open_documents(handle, launch::openable_paths(paths));
        }
        let _ = (handle, event);
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    // Finder に登録する拡張子（tauri.conf.json）が、描画できる拡張子とずれないようにする
    #[test]
    fn file_associations_cover_every_supported_extension() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let registered: BTreeSet<&str> = conf["bundle"]["fileAssociations"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|a| a["ext"].as_array().unwrap())
            .map(|e| e.as_str().unwrap())
            .collect();
        let supported: BTreeSet<&str> = washi_core::render::supported_extensions().into_iter().collect();
        assert_eq!(registered, supported);
    }
}
