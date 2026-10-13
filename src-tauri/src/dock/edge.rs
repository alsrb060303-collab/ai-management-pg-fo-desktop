//! Non-Windows fallback: pin the sidebar to the right edge of its monitor.
//! Window tracking is Windows-only for now, and Wayland ignores positioning.

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use super::layout::{self, Rect};
use super::{Dock, DockMode, DockStatus, SIDEBAR_LABEL, SIDEBAR_WIDTH};

pub fn apply(app: &AppHandle, dock: &Dock) {
    let settings = dock.settings();
    let Some(window) = app.get_webview_window(SIDEBAR_LABEL) else {
        return;
    };
    let _ = window.set_always_on_top(settings.pinned);

    if settings.magnet.on()
        && let Ok(Some(monitor)) = window.current_monitor()
    {
        let area = monitor.work_area();
        let work = Rect {
            left: area.position.x,
            top: area.position.y,
            right: area.position.x + area.size.width as i32,
            bottom: area.position.y + area.size.height as i32,
        };
        let width = layout::to_physical(SIDEBAR_WIDTH, monitor.scale_factor());
        let rect = layout::place_at_edge(work, width);
        // GTK will not shrink a non-resizable window below its default size; the
        // window is undecorated, so the user still cannot resize it by hand.
        let _ = window.set_resizable(true);
        let _ = window.set_position(PhysicalPosition::new(rect.left, rect.top));
        let _ = window.set_size(PhysicalSize::new(rect.width() as u32, rect.height() as u32));
    }

    dock.publish(
        app,
        DockStatus {
            magnet: settings.magnet,
            pinned: settings.pinned,
            visible: settings.visible,
            mode: if settings.magnet.on() {
                DockMode::Edge
            } else {
                DockMode::Floating
            },
            target: None,
            side: None,
        },
    );
}
