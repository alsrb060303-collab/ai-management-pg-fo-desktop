//! System tray icon, global shortcut, and single-instance handling.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dock::{Dock, STATUS_EVENT};
use crate::shell;

/// Shows or hides the current window (main or sidebar) from anywhere.
pub const TOGGLE_SHORTCUT: &str = "CommandOrControl+Alt+Space";

/// Menu items whose labels follow the language.
struct TrayItems {
    toggle: MenuItem<tauri::Wry>,
    main: MenuItem<tauri::Wry>,
    sidebar: MenuItem<tauri::Wry>,
    dock: CheckMenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

fn labels() -> [String; 5] {
    [
        crate::tr!("보이기 / 숨기기", "Show / hide"),
        crate::tr!("메인 창 열기", "Open main window"),
        crate::tr!("사이드바로 전환", "Switch to sidebar"),
        crate::tr!("터미널 옆에 붙이기", "Dock next to terminal"),
        crate::tr!("종료", "Quit"),
    ]
}

/// Re-labels the tray menu after the language changed.
pub fn relabel(app: &AppHandle) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let [toggle, main, sidebar, dock, quit] = labels();
    let _ = items.toggle.set_text(toggle);
    let _ = items.main.set_text(main);
    let _ = items.sidebar.set_text(sidebar);
    let _ = items.dock.set_text(dock);
    let _ = items.quit.set_text(quit);
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let enabled = app.state::<Dock>().settings().enabled;
    let [
        toggle_label,
        main_label,
        sidebar_label,
        dock_label,
        quit_label,
    ] = labels();
    let toggle = MenuItem::with_id(app, "toggle", toggle_label, true, Some(TOGGLE_SHORTCUT))?;
    let main = MenuItem::with_id(app, "main", main_label, true, None::<&str>)?;
    let sidebar = MenuItem::with_id(app, "sidebar", sidebar_label, true, None::<&str>)?;
    let dock_item = CheckMenuItem::with_id(app, "dock", dock_label, true, enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &main,
            &sidebar,
            &PredefinedMenuItem::separator(app)?,
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
                "toggle" => shell::toggle(app, false),
                "main" => shell::show_main(app),
                "sidebar" => shell::show_sidebar(app),
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
                shell::reveal(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayItems {
        toggle: toggle.clone(),
        main: main.clone(),
        sidebar: sidebar.clone(),
        dock: dock_item.clone(),
        quit: quit.clone(),
    });
    // The frontend may have set the language while the menu was being built.
    relabel(app);

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
                shell::toggle(app, true);
            }
        });
    if let Err(err) = result {
        eprintln!("failed to register {TOGGLE_SHORTCUT}: {err}");
    }
}
