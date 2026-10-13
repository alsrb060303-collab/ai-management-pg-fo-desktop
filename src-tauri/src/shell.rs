//! The two app windows: the main workspace and the docked sidebar.
//!
//! Only one is on screen at a time. Switching to the sidebar closes the main
//! window; switching back hides the sidebar and opens the main window again.
//! The last choice is remembered for the next launch.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindowBuilder};

use crate::dock::{Dock, Magnet};

pub const MAIN_LABEL: &str = "main";
pub const VIEW_EVENT: &str = "view-mode";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Main,
    Sidebar,
}

pub struct Shell {
    mode: Mutex<ViewMode>,
    file: Option<PathBuf>,
    /// Held while the main window is being built, so two requests in quick
    /// succession (double launch, repeated shortcut) cannot build two windows
    /// with the same label.
    creating: Mutex<()>,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    mode: ViewMode,
}

impl Shell {
    /// Registers the shell state with the mode saved by the last run.
    pub fn manage(app: &AppHandle) {
        let file = app
            .path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join("view.json"));
        let saved = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .unwrap_or_default();
        app.manage(Shell {
            mode: Mutex::new(saved.mode),
            file,
            creating: Mutex::new(()),
        });
    }

    pub fn mode(&self) -> ViewMode {
        *self.mode.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_mode(&self, app: &AppHandle, mode: ViewMode) {
        {
            let mut current = self.mode.lock().unwrap_or_else(|e| e.into_inner());
            if *current == mode {
                return;
            }
            *current = mode;
        }
        if let Some(file) = &self.file {
            // Losing this only means the next launch opens the default window.
            let _ = file.parent().map(std::fs::create_dir_all);
            if let Ok(json) = serde_json::to_vec(&Saved { mode }) {
                let _ = std::fs::write(file, json);
            }
        }
        let _ = app.emit(VIEW_EVENT, mode);
    }
}

/// Opens the window for the saved mode. Call once the dock has started.
pub fn restore(app: &AppHandle) {
    match app.state::<Shell>().mode() {
        ViewMode::Main => show_main(app),
        ViewMode::Sidebar => show_sidebar(app),
    }
}

/// Hides the sidebar and opens (or brings back) the main window.
pub fn show_main(app: &AppHandle) {
    app.state::<Shell>().set_mode(app, ViewMode::Main);
    app.state::<Dock>().set_visible(app, false);
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    // Building a window inside a synchronous command or event handler deadlocks
    // on Windows, and this runs from tray, shortcut and single-instance handlers.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = create_main(&app) {
            eprintln!("failed to open the main window: {err}");
        }
    });
}

/// Closes the main window and shows the sidebar beside the terminal. Coming
/// from the main window, the magnet starts over in auto mode, following
/// whichever window is in front.
pub fn show_sidebar(app: &AppHandle) {
    let shell = app.state::<Shell>();
    let dock = app.state::<Dock>();
    if shell.mode() != ViewMode::Sidebar {
        dock.reset_magnet(Magnet::Auto);
    }
    shell.set_mode(app, ViewMode::Sidebar);
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.close();
    }
    dock.set_visible(app, true);
}

/// Shows the window for the current mode, e.g. on a second launch or tray click.
pub fn reveal(app: &AppHandle) {
    match app.state::<Shell>().mode() {
        ViewMode::Main => show_main(app),
        ViewMode::Sidebar => app.state::<Dock>().set_visible(app, true),
    }
}

/// Hides the current window if it is up, else shows it.
///
/// `need_focus`: the global shortcut only hides a main window that is in
/// front. The tray menu takes focus away before it runs, so it hides any
/// main window that is on screen.
pub fn toggle(app: &AppHandle, need_focus: bool) {
    match app.state::<Shell>().mode() {
        ViewMode::Main => match app.get_webview_window(MAIN_LABEL) {
            Some(window)
                if window.is_visible().unwrap_or(false)
                    && !window.is_minimized().unwrap_or(false)
                    && (!need_focus || window.is_focused().unwrap_or(false)) =>
            {
                let _ = window.minimize();
            }
            _ => show_main(app),
        },
        ViewMode::Sidebar => app.state::<Dock>().toggle_visible(app),
    }
}

/// Runs on an async-runtime worker, never on the main thread, so blocking on
/// the lock cannot stall the event loop.
fn create_main(app: &AppHandle) -> tauri::Result<()> {
    let shell = app.state::<Shell>();
    let _creating = shell.creating.lock().unwrap_or_else(|e| e.into_inner());
    // The user may have switched to the sidebar while this request waited.
    if shell.mode() != ViewMode::Main {
        return Ok(());
    }
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_LABEL)
        .ok_or(tauri::Error::WindowNotFound)?;
    let window = WebviewWindowBuilder::from_config(app, config)?.build()?;
    // Building takes a while; a switch to the sidebar in the meantime found no
    // window to close.
    if shell.mode() == ViewMode::Main {
        let _ = window.set_focus();
    } else {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub fn view_mode(shell: tauri::State<'_, Shell>) -> ViewMode {
    shell.mode()
}

/// Async so that opening the main window never runs on the main thread.
#[tauri::command]
pub async fn view_switch(app: AppHandle, mode: ViewMode) {
    match mode {
        ViewMode::Main => show_main(&app),
        ViewMode::Sidebar => show_sidebar(&app),
    }
}
