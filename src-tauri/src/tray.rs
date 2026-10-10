//! System tray icon, global shortcut, and single-instance handling.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dock::{Dock, STATUS_EVENT};

/// Shows or hides the sidebar from anywhere.
pub const TOGGLE_SHORTCUT: &str = "CommandOrControl+Alt+Space";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let enabled = app.state::<Dock>().settings().enabled;
    let toggle = MenuItem::with_id(
        app,
        "toggle",
        "보이기 / 숨기기",
        true,
        Some(TOGGLE_SHORTCUT),
    )?;
    let dock_item = CheckMenuItem::with_id(
        app,
        "dock",
        "터미널 옆에 붙이기",
        true,
        enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &dock_item,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("AI Management")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let dock = app.state::<Dock>();
            match event.id().as_ref() {
                "toggle" => dock.toggle_visible(app),
                "dock" => dock.set_enabled(app, !dock.settings().enabled),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                app.state::<Dock>().toggle_visible(app);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    // Keep the check mark in sync when docking is toggled from the sidebar.
    let handle = app.clone();
    app.listen(STATUS_EVENT, move |_| {
        let enabled = handle.state::<Dock>().settings().enabled;
        let _ = dock_item.set_checked(enabled);
    });
    Ok(())
}

/// Registers the global toggle shortcut. A shortcut taken by another app is not
/// fatal; the tray still works.
pub fn register_shortcut(app: &AppHandle) {
    let result = app
        .global_shortcut()
        .on_shortcut(TOGGLE_SHORTCUT, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                app.state::<Dock>().toggle_visible(app);
            }
        });
    if let Err(err) = result {
        eprintln!("failed to register {TOGGLE_SHORTCUT}: {err}");
    }
}
