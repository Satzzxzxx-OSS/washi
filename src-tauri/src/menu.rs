use tauri::{
    menu::{Menu, MenuItem, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, Wry,
};

pub const MENU_EVENT: &str = "washi://menu";

pub mod id {
    pub const OPEN: &str = "open";
    pub const RELOAD: &str = "reload";
    pub const PRINT: &str = "print";
    pub const PASTE: &str = "paste";
    pub const FIND: &str = "find";
    pub const OUTLINE: &str = "outline";
    pub const THEME_SYSTEM: &str = "theme-system";
    pub const THEME_LIGHT: &str = "theme-light";
    pub const THEME_DARK: &str = "theme-dark";
    pub const WIDTH_NARROW: &str = "width-narrow";
    pub const WIDTH_WIDE: &str = "width-wide";
    pub const WIDTH_FULL: &str = "width-full";
    pub const ZOOM_IN: &str = "zoom-in";
    pub const ZOOM_OUT: &str = "zoom-out";
    pub const ZOOM_RESET: &str = "zoom-reset";
}

fn item(app: &AppHandle, id: &str, label: &str, accelerator: Option<&str>) -> tauri::Result<MenuItem<Wry>> {
    let builder = MenuItemBuilder::with_id(id, label);
    match accelerator {
        Some(accelerator) => builder.accelerator(accelerator).build(app),
        None => builder.build(app),
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    #[cfg(target_os = "macos")]
    {
        let application = SubmenuBuilder::new(app, "Washi")
            .about(None)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        menu.append(&application)?;
    }

    let file = SubmenuBuilder::new(app, "ファイル")
        .item(&item(app, id::OPEN, "開く…", Some("CmdOrCtrl+O"))?)
        .item(&item(app, id::RELOAD, "再読み込み", Some("CmdOrCtrl+R"))?)
        .separator()
        .item(&item(app, id::PRINT, "印刷…", Some("CmdOrCtrl+P"))?)
        .separator()
        .close_window()
        .build()?;

    let edit = SubmenuBuilder::new(app, "編集")
        .copy()
        .select_all()
        .separator()
        .item(&item(app, id::PASTE, "貼り付けて開く", Some("CmdOrCtrl+V"))?)
        .separator()
        .item(&item(app, id::FIND, "検索…", Some("CmdOrCtrl+F"))?)
        .build()?;

    let theme = SubmenuBuilder::new(app, "テーマ")
        .item(&item(app, id::THEME_SYSTEM, "システムに合わせる", None)?)
        .item(&item(app, id::THEME_LIGHT, "ライト", None)?)
        .item(&item(app, id::THEME_DARK, "ダーク", None)?)
        .build()?;

    let width = SubmenuBuilder::new(app, "本文の幅")
        .item(&item(app, id::WIDTH_NARROW, "標準", None)?)
        .item(&item(app, id::WIDTH_WIDE, "広め", None)?)
        .item(&item(app, id::WIDTH_FULL, "ウィンドウいっぱい", None)?)
        .build()?;

    let view = SubmenuBuilder::new(app, "表示")
        .item(&item(app, id::OUTLINE, "目次を表示／隠す", Some("CmdOrCtrl+Shift+O"))?)
        .separator()
        .item(&item(app, id::ZOOM_IN, "拡大", Some("CmdOrCtrl+="))?)
        .item(&item(app, id::ZOOM_OUT, "縮小", Some("CmdOrCtrl+-"))?)
        .item(&item(app, id::ZOOM_RESET, "実際のサイズ", Some("CmdOrCtrl+0"))?)
        .separator()
        .item(&theme)
        .item(&width)
        .separator()
        .fullscreen()
        .build()?;

    let window = SubmenuBuilder::new(app, "ウィンドウ")
        .minimize()
        .maximize()
        .build()?;

    menu.append(&file)?;
    menu.append(&edit)?;
    menu.append(&view)?;
    menu.append(&window)?;
    Ok(menu)
}

pub fn dispatch(app: &AppHandle, menu_id: &str) {
    let focused = app
        .webview_windows()
        .into_values()
        .find(|w| w.is_focused().unwrap_or(false));
    if let Some(window) = focused {
        let _ = app.emit_to(window.label(), MENU_EVENT, menu_id);
    }
}

#[cfg(test)]
mod tests {
    use super::id::*;

    #[test]
    fn ids_are_unique() {
        let all = [
            OPEN, RELOAD, PRINT, PASTE, FIND, OUTLINE, THEME_SYSTEM, THEME_LIGHT, THEME_DARK,
            WIDTH_NARROW, WIDTH_WIDE, WIDTH_FULL, ZOOM_IN, ZOOM_OUT, ZOOM_RESET,
        ];
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len());
    }
}
