//! User preferences, kept in the backend so they survive app updates (the
//! webview's `localStorage` doesn't) and follow the user between devices.
//!
//! `<app data>/prefs.db` holds opaque string values by key (the frontend
//! stores JSON or plain strings). Only the keys in `SYNCED_KEYS` sync through
//! the account (`sync::prefs`, `users/{uid}/prefs/{key}`); every other key is
//! device-specific and never leaves this device.

pub mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

pub use store::Store;

/// Emitted to every window (payload `{ key, value }`, `value` null when the
/// key was deleted) when a preference changed here or by sync.
pub const PREFS_EVENT: &str = "prefs-changed";

/// The portable preferences that sync across devices. Everything else (recent
/// projects, per-path tool choices, panel widths, tabs...) is local only.
/// Extend the list here; the security rules accept any key that matches its
/// document id, and `sync::prefs` reads this list.
pub const SYNCED_KEYS: &[&str] = &[
    "theme",
    "language",
    "fontSize",
    "notifyEnabled",
    "defaultTool",
];

pub fn is_synced(key: &str) -> bool {
    SYNCED_KEYS.contains(&key)
}

/// Keys are ASCII, at most this many bytes.
pub const MAX_KEY: usize = 100;
/// Values are at most this many bytes.
pub const MAX_VALUE: usize = 64 * 1024;
/// The security rules allow synced values up to this many characters; a longer
/// one stays on this device.
pub const SYNCED_VALUE_CHARS: usize = 4096;

/// The store opens lazily, so a broken database never blocks app start.
pub struct Prefs {
    path: PathBuf,
    store: OnceLock<Store>,
}

impl Prefs {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let path = app.path().app_data_dir()?.join("prefs.db");
        app.manage(Prefs {
            path,
            store: OnceLock::new(),
        });
        Ok(())
    }

    pub fn store(&self) -> store::Result<&Store> {
        if let Some(store) = self.store.get() {
            return Ok(store);
        }
        let store = Store::open(&self.path)?;
        Ok(self.store.get_or_init(|| store))
    }
}

/// Tells every window that a preference changed.
pub fn announce(app: &AppHandle, key: &str, value: Option<&str>) {
    let _ = app.emit(PREFS_EVENT, json!({ "key": key, "value": value }));
}

async fn blocking<T, F>(app: AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> store::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(app.state::<Prefs>().store()?))
        .await
        .map_err(|e| e.to_string())?
}

/// Every preference that isn't deleted.
#[tauri::command]
pub async fn prefs_all(app: AppHandle) -> Result<BTreeMap<String, String>, String> {
    blocking(app, |store| store.all()).await
}

/// Sets a preference; a `null` value deletes it.
#[tauri::command]
pub async fn prefs_set(app: AppHandle, key: String, value: Option<String>) -> Result<(), String> {
    let (k, v) = (key.clone(), value.clone());
    let changed = blocking(app.clone(), move |store| store.set(&k, v.as_deref())).await?;
    if changed {
        announce(&app, &key, value.as_deref());
        if is_synced(&key) {
            crate::sync::changed(&app);
        }
    }
    Ok(())
}
