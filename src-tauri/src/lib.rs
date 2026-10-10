mod claude_config;
mod dock;
mod library;
mod tray;

use tauri::{Manager, WindowEvent};

use dock::{Dock, SIDEBAR_LABEL};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch just shows the running sidebar.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(dock) = app.try_state::<Dock>() {
                dock.set_visible(app, true);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            dock::dock_status,
            dock::dock_set_enabled,
            dock::dock_set_pinned,
            dock::sidebar_hide,
            claude_config::claude_config_scan,
            library::library_overview,
            library::library_set_dir,
            library::library_apply,
            library::library_remove,
            library::library_update,
            library::library_import,
            library::config_set_enabled,
        ])
        .setup(|app| {
            let handle = app.handle();
            Dock::manage(handle);
            library::Library::manage(handle)?;
            tray::create(handle)?;
            Dock::start(handle)?;
            tray::register_shortcut(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the sidebar hides it to the tray; quit from the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == SIDEBAR_LABEL
            {
                api.prevent_close();
                let app = window.app_handle();
                app.state::<Dock>().set_visible(app, false);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
