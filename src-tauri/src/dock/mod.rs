//! Keeps the sidebar window docked next to the user's terminal or editor.
//!
//! On Windows a tracker thread follows the active target window (see `win`).
//! Elsewhere the sidebar is pinned to the right edge of the screen (see `edge`).

pub mod layout;

#[cfg(not(windows))]
mod edge;
#[cfg(windows)]
mod win;

use std::sync::Mutex;
#[cfg(windows)]
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub use layout::Side;

pub const SIDEBAR_LABEL: &str = "sidebar";
/// Sidebar size in logical pixels.
pub const SIDEBAR_WIDTH: f64 = 360.0;
#[cfg_attr(not(windows), allow(dead_code))]
pub const SIDEBAR_MIN_HEIGHT: f64 = 400.0;
pub const STATUS_EVENT: &str = "dock-status";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum TargetApp {
    WindowsTerminal,
    Console,
    VsCode,
    ClaudeDesktop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum DockMode {
    /// Attached beside a target window.
    Docked,
    /// No target; pinned to the screen edge.
    Edge,
    /// Docking is off; the window stays where the user puts it.
    Floating,
}

/// What the magnet button does, cycled by clicking it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Magnet {
    /// Follows whichever terminal or editor window comes to the front.
    #[default]
    Auto,
    /// Stays with the window it is docked to; others coming forward are
    /// ignored. With nothing docked yet, the next target to come forward
    /// becomes the one it stays with.
    Locked,
    /// Docking is off; the window stays where the user puts it.
    Off,
}

impl Magnet {
    /// The next mode for the title-bar button: auto → locked → off → auto.
    pub fn next(self) -> Self {
        match self {
            Magnet::Auto => Magnet::Locked,
            Magnet::Locked => Magnet::Off,
            Magnet::Off => Magnet::Auto,
        }
    }

    pub fn on(self) -> bool {
        self != Magnet::Off
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockStatus {
    pub magnet: Magnet,
    pub pinned: bool,
    pub visible: bool,
    pub mode: DockMode,
    pub target: Option<TargetApp>,
    pub side: Option<Side>,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub magnet: Magnet,
    pub pinned: bool,
    pub visible: bool,
}

pub struct Dock {
    settings: Mutex<Settings>,
    status: Mutex<DockStatus>,
    #[cfg(windows)]
    tracker: OnceLock<win::TrackerHandle>,
}

impl Dock {
    /// Registers the dock as managed state. Call `start` afterwards, once anything
    /// that listens for status events (the tray) is set up. The sidebar starts
    /// hidden; the shell shows it when the sidebar is the current view.
    pub fn manage(app: &AppHandle) {
        let settings = Settings {
            magnet: Magnet::Auto,
            pinned: false,
            visible: false,
        };
        app.manage(Dock {
            settings: Mutex::new(settings),
            status: Mutex::new(DockStatus {
                magnet: settings.magnet,
                pinned: settings.pinned,
                visible: settings.visible,
                mode: DockMode::Floating,
                target: None,
                side: None,
            }),
            #[cfg(windows)]
            tracker: OnceLock::new(),
        });
    }

    /// Starts following target windows and publishing status.
    pub fn start(app: &AppHandle) -> tauri::Result<()> {
        #[cfg(windows)]
        {
            let window = app
                .get_webview_window(SIDEBAR_LABEL)
                .ok_or(tauri::Error::WindowNotFound)?;
            let tracker = win::start(app.clone(), window.hwnd()?)?;
            let _ = app.state::<Dock>().tracker.set(tracker);
        }
        #[cfg(not(windows))]
        app.state::<Dock>().apply(app, false);

        Ok(())
    }

    pub fn settings(&self) -> Settings {
        *self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> DockStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_magnet(&self, app: &AppHandle, magnet: Magnet) {
        self.update(|s| s.magnet = magnet);
        self.apply(app, false);
    }

    /// Sets the magnet without re-placing the sidebar, for a caller that
    /// applies placement right after (switching to the sidebar view).
    pub fn reset_magnet(&self, magnet: Magnet) {
        self.update(|s| s.magnet = magnet);
    }

    pub fn set_pinned(&self, app: &AppHandle, pinned: bool) {
        self.update(|s| s.pinned = pinned);
        self.apply(app, false);
    }

    /// Shows (and focuses) or hides the sidebar at the user's request.
    pub fn set_visible(&self, app: &AppHandle, visible: bool) {
        self.update(|s| s.visible = visible);
        if let Some(window) = app.get_webview_window(SIDEBAR_LABEL) {
            if visible {
                let _ = window.show();
                let _ = window.set_focus();
            } else {
                let _ = window.hide();
            }
        }
        self.apply(app, visible);
    }

    pub fn toggle_visible(&self, app: &AppHandle) {
        // Decide from what is on screen: the dock may have hidden the sidebar with
        // its target while the user setting still says visible.
        let on_screen = self.settings().visible
            && app
                .get_webview_window(SIDEBAR_LABEL)
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(true);
        self.set_visible(app, !on_screen);
    }

    /// Stores the latest status and notifies the frontend when it changed.
    pub(crate) fn publish(&self, app: &AppHandle, status: DockStatus) {
        {
            let mut current = self.status.lock().unwrap_or_else(|e| e.into_inner());
            if *current == status {
                return;
            }
            *current = status.clone();
        }
        let _ = app.emit(STATUS_EVENT, status);
    }

    fn update(&self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings.lock().unwrap_or_else(|e| e.into_inner()));
    }

    /// Re-evaluates placement after a settings change. `user_show` means the user
    /// just asked to see the sidebar, so it must not stay hidden with its target.
    fn apply(&self, app: &AppHandle, user_show: bool) {
        #[cfg(windows)]
        {
            let _ = app;
            if let Some(tracker) = self.tracker.get() {
                tracker.refresh(user_show);
            }
        }
        #[cfg(not(windows))]
        {
            let _ = user_show;
            edge::apply(app, self);
        }
    }
}

#[tauri::command]
pub fn dock_status(dock: State<'_, Dock>) -> DockStatus {
    dock.status()
}

#[tauri::command]
pub fn dock_set_pinned(app: AppHandle, dock: State<'_, Dock>, pinned: bool) {
    dock.set_pinned(&app, pinned);
}

/// The title-bar magnet button: auto → locked → off → auto.
#[tauri::command]
pub fn dock_cycle_magnet(app: AppHandle, dock: State<'_, Dock>) {
    dock.set_magnet(&app, dock.settings().magnet.next());
}

#[tauri::command]
pub fn sidebar_hide(app: AppHandle, dock: State<'_, Dock>) {
    dock.set_visible(&app, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnet_cycles_auto_locked_off() {
        assert_eq!(Magnet::Auto.next(), Magnet::Locked);
        assert_eq!(Magnet::Locked.next(), Magnet::Off);
        assert_eq!(Magnet::Off.next(), Magnet::Auto);
        assert!(Magnet::Locked.on());
        assert!(!Magnet::Off.on());
    }
}
