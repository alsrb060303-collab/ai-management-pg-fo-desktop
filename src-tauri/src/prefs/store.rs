//! Local SQLite store for preferences (`<app data>/prefs.db`).
//!
//! Same shape as the memo and activity stores: every row has `updated_at` and
//! the device that changed it last, deleting a key leaves a tombstone
//! (`deleted = 1`) so the deletion can sync, and `synced_at` is the
//! `updated_at` last uploaded or applied from the server (a row whose two
//! differ waits to be uploaded). Only keys on `super::SYNCED_KEYS` ever leave
//! the device; the store itself keeps every key alike.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::{MAX_KEY, MAX_VALUE, SYNCED_KEYS, SYNCED_VALUE_CHARS};

pub type Result<T> = std::result::Result<T, String>;

fn db_err(e: rusqlite::Error) -> String {
    crate::tr!("설정 저장소 오류: {e}", "Preferences storage error: {e}")
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A preference as it travels between devices. A deleted row has an empty
/// value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncRow {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
    pub deleted: bool,
    pub device_id: String,
}

/// A key whose value changed (`None` = deleted), to tell the windows.
pub type Change = (String, Option<String>);

/// Keys are short ASCII without control characters or spaces.
pub fn check_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > MAX_KEY || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(crate::tr!(
            "설정 키는 공백 없는 ASCII 문자 1~{MAX_KEY}자여야 해요",
            "A preference key must be 1-{MAX_KEY} ASCII characters without spaces"
        ));
    }
    Ok(())
}

pub fn check_value(value: &str) -> Result<()> {
    if value.len() > MAX_VALUE {
        return Err(crate::tr!(
            "설정 값이 너무 커요 (최대 {MAX_VALUE}바이트)",
            "The preference value is too large (at most {MAX_VALUE} bytes)"
        ));
    }
    Ok(())
}

fn synced_list() -> String {
    SYNCED_KEYS
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(",")
}

pub struct Store {
    conn: Mutex<Connection>,
    device: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| {
                crate::tr!(
                    "설정 폴더를 만들 수 없어요: {e}",
                    "Couldn't create the preferences folder: {e}"
                )
            })?;
        }
        Self::init(Connection::open(path).map_err(db_err)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_err)?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        migrate(&conn)?;
        let device = match conn
            .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_err)?
        {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES ('device_id', ?1)",
                    [&id],
                )
                .map_err(db_err)?;
                id
            }
        };
        Ok(Store {
            conn: Mutex::new(conn),
            device,
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(test)]
    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(db_err)
    }

    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        let conn = self.conn();
        match value {
            Some(v) => conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, v],
            ),
            None => conn.execute("DELETE FROM meta WHERE key = ?1", [key]),
        }
        .map(drop)
        .map_err(db_err)
    }

    /// Every key that isn't deleted.
    pub fn all(&self) -> Result<BTreeMap<String, String>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT key, value FROM prefs WHERE deleted = 0")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db_err)?;
        rows.collect::<rusqlite::Result<_>>().map_err(db_err)
    }

    #[cfg(test)]
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.all()?.remove(key))
    }

    /// `(updated_at, deleted)` of a key, even a deleted one.
    #[cfg(test)]
    pub fn stamp(&self, key: &str) -> Result<Option<(i64, bool)>> {
        self.conn()
            .query_row(
                "SELECT updated_at, deleted FROM prefs WHERE key = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db_err)
    }

    /// Stores `value` (`None` deletes the key). `updated_at` only moves when
    /// the value really changes; returns whether it did. A change always
    /// outranks what is stored, even when this device's clock runs behind a
    /// value it applied from another device.
    pub fn set(&self, key: &str, value: Option<&str>) -> Result<bool> {
        check_key(key)?;
        if let Some(v) = value {
            check_value(v)?;
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let current: Option<(String, i64, bool)> = tx
            .query_row(
                "SELECT value, updated_at, deleted FROM prefs WHERE key = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;
        let unchanged = match (&current, value) {
            (Some((_, _, true)) | None, None) => true,
            (Some((old, _, false)), Some(new)) => old == new,
            _ => false,
        };
        if unchanged {
            return Ok(false);
        }
        let at = now_ms().max(current.as_ref().map_or(0, |(_, at, _)| at + 1));
        tx.execute(
            "INSERT INTO prefs (key, value, updated_at, device_id, synced_at, deleted)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)
             ON CONFLICT (key) DO UPDATE SET
                 value = excluded.value, updated_at = excluded.updated_at,
                 device_id = excluded.device_id, synced_at = NULL,
                 deleted = excluded.deleted",
            params![key, value.unwrap_or(""), at, self.device, value.is_none()],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(true)
    }

    // ---- sync ----

    /// Synced keys changed here since their last upload, oldest first. Values
    /// the security rules would refuse (too long) stay home.
    pub fn pending(&self, limit: usize) -> Result<Vec<SyncRow>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT key, value, updated_at, deleted, device_id FROM prefs
                 WHERE key IN ({}) AND (synced_at IS NULL OR synced_at != updated_at)
                 ORDER BY updated_at",
                synced_list()
            ))
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(SyncRow {
                    key: r.get(0)?,
                    value: r.get(1)?,
                    updated_at: r.get(2)?,
                    deleted: r.get(3)?,
                    device_id: r.get(4)?,
                })
            })
            .map_err(db_err)?;
        let mut rows: Vec<SyncRow> = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_err)?
            .into_iter()
            .filter(|r| r.value.chars().count() <= SYNCED_VALUE_CHARS)
            .collect();
        rows.truncate(limit);
        Ok(rows)
    }

    /// Records rows as uploaded, unless they changed again meanwhile.
    pub fn mark_synced(&self, rows: &[(String, i64)]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        for (key, updated_at) in rows {
            tx.execute(
                "UPDATE prefs SET synced_at = updated_at WHERE key = ?1 AND updated_at = ?2",
                params![key, updated_at],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)
    }

    /// Everything needs uploading again (another account, or a reset).
    pub fn mark_all_unsynced(&self) -> Result<()> {
        self.conn()
            .execute("UPDATE prefs SET synced_at = NULL", [])
            .map(drop)
            .map_err(db_err)
    }

    /// Applies rows from another device: the newer `updated_at` wins (the
    /// device id breaks ties, the same way the server rules do). Keys that
    /// aren't synced are ignored. Returns the keys whose visible value
    /// changed.
    pub fn apply_remote(&self, rows: &[SyncRow]) -> Result<Vec<Change>> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let mut changed: Vec<Change> = Vec::new();
        for row in rows {
            if !SYNCED_KEYS.contains(&row.key.as_str())
                || check_value(&row.value).is_err()
                || row.value.chars().count() > SYNCED_VALUE_CHARS
            {
                continue;
            }
            let local: Option<(String, i64, bool, String)> = tx
                .query_row(
                    "SELECT value, updated_at, deleted, device_id FROM prefs WHERE key = ?1",
                    [&row.key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(db_err)?;
            if let Some((_, at, _, device)) = &local
                && (row.updated_at, row.device_id.as_str()) <= (*at, device.as_str())
            {
                continue;
            }
            let value = if row.deleted { "" } else { row.value.as_str() };
            tx.execute(
                "INSERT INTO prefs (key, value, updated_at, device_id, synced_at, deleted)
                 VALUES (?1, ?2, ?3, ?4, ?3, ?5)
                 ON CONFLICT (key) DO UPDATE SET
                     value = excluded.value, updated_at = excluded.updated_at,
                     device_id = excluded.device_id, synced_at = excluded.synced_at,
                     deleted = excluded.deleted",
                params![row.key, value, row.updated_at, row.device_id, row.deleted],
            )
            .map_err(db_err)?;
            let visible = |v: &str, deleted: bool| (!deleted).then(|| v.to_string());
            let before = local.and_then(|(v, _, d, _)| visible(&v, d));
            let after = visible(value, row.deleted);
            if before != after {
                changed.push((row.key.clone(), after));
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(changed)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db_err)?;
    if version < 1 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS prefs (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL,
                 updated_at INTEGER NOT NULL,
                 device_id TEXT NOT NULL,
                 synced_at INTEGER,
                 deleted INTEGER NOT NULL DEFAULT 0
             );
             PRAGMA user_version = 1;
             COMMIT;",
        )
        .map_err(db_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, value: &str, at: i64, device: &str) -> SyncRow {
        SyncRow {
            key: key.into(),
            value: value.into(),
            updated_at: at,
            deleted: false,
            device_id: device.into(),
        }
    }

    #[test]
    fn set_get_and_delete() {
        let s = Store::in_memory().unwrap();
        assert!(s.all().unwrap().is_empty());
        assert!(s.set("theme", Some("dark")).unwrap());
        assert!(s.set("panelWidth", Some("{\"a\":1}")).unwrap());
        assert_eq!(s.get("theme").unwrap().as_deref(), Some("dark"));
        assert_eq!(s.all().unwrap().len(), 2);

        assert!(s.set("theme", None).unwrap());
        assert_eq!(s.get("theme").unwrap(), None);
        assert_eq!(s.all().unwrap().len(), 1);
        // The tombstone stays, so the deletion can sync.
        assert_eq!(s.stamp("theme").unwrap().map(|(_, d)| d), Some(true));
        // Deleting again, or a key that never existed, changes nothing.
        assert!(!s.set("theme", None).unwrap());
        assert!(!s.set("nothing", None).unwrap());
        assert_eq!(s.stamp("nothing").unwrap(), None);
        // Setting it again revives it.
        assert!(s.set("theme", Some("light")).unwrap());
        assert_eq!(s.get("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn an_unchanged_value_does_not_bump_updated_at() {
        let s = Store::in_memory().unwrap();
        s.set("theme", Some("dark")).unwrap();
        let first = s.stamp("theme").unwrap().unwrap().0;
        s.mark_synced(&[("theme".into(), first)]).unwrap();

        assert!(!s.set("theme", Some("dark")).unwrap());
        assert_eq!(s.stamp("theme").unwrap().unwrap().0, first);
        assert!(s.pending(10).unwrap().is_empty(), "still synced");

        assert!(s.set("theme", Some("light")).unwrap());
        assert!(s.stamp("theme").unwrap().unwrap().0 > first);
        assert_eq!(s.pending(10).unwrap().len(), 1);
    }

    #[test]
    fn keys_and_values_are_validated() {
        let s = Store::in_memory().unwrap();
        assert!(s.set("", Some("x")).is_err());
        assert!(s.set("has space", Some("x")).is_err());
        assert!(s.set("한글", Some("x")).is_err());
        assert!(s.set(&"k".repeat(101), Some("x")).is_err());
        assert!(s.set(&"k".repeat(100), Some("x")).is_ok());
        assert!(s.set("big", Some(&"v".repeat(MAX_VALUE + 1))).is_err());
        assert!(s.set("big", Some(&"v".repeat(MAX_VALUE))).is_ok());
    }

    #[test]
    fn only_allow_listed_keys_are_pending_for_sync() {
        let s = Store::in_memory().unwrap();
        for key in SYNCED_KEYS {
            s.set(key, Some("v")).unwrap();
        }
        s.set("recentProjects", Some("[]")).unwrap();
        s.set("toolPrefs", Some("{}")).unwrap();
        let pending = s.pending(100).unwrap();
        let mut keys: Vec<_> = pending.iter().map(|r| r.key.as_str()).collect();
        keys.sort();
        let mut want = SYNCED_KEYS.to_vec();
        want.sort();
        assert_eq!(keys, want);
        assert!(pending.iter().all(|r| r.device_id == s.device()));

        // A value the rules would refuse stays home.
        s.set("theme", Some(&"x".repeat(SYNCED_VALUE_CHARS + 1)))
            .unwrap();
        assert!(!s.pending(100).unwrap().iter().any(|r| r.key == "theme"));
    }

    #[test]
    fn deleting_a_synced_key_uploads_a_tombstone() {
        let s = Store::in_memory().unwrap();
        s.set("language", Some("ko")).unwrap();
        let at = s.stamp("language").unwrap().unwrap().0;
        s.mark_synced(&[("language".into(), at)]).unwrap();
        s.set("language", None).unwrap();
        // A mark for the older stamp doesn't count.
        s.mark_synced(&[("language".into(), at)]).unwrap();
        let pending = s.pending(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].deleted && pending[0].value.is_empty());
        s.mark_synced(&[("language".into(), pending[0].updated_at)])
            .unwrap();
        assert!(s.pending(10).unwrap().is_empty());
        s.mark_all_unsynced().unwrap();
        assert_eq!(s.pending(10).unwrap().len(), 1);
    }

    #[test]
    fn apply_remote_newer_wins_and_device_id_breaks_ties() {
        let s = Store::in_memory().unwrap();
        s.set("theme", Some("dark")).unwrap();
        let at = s.stamp("theme").unwrap().unwrap().0;

        // Older: ignored.
        let c = s
            .apply_remote(&[row("theme", "old", at - 1, "zzz")])
            .unwrap();
        assert!(c.is_empty());
        assert_eq!(s.get("theme").unwrap().as_deref(), Some("dark"));

        // Same time: a smaller device id loses, a larger one wins.
        let c = s.apply_remote(&[row("theme", "tie-low", at, "")]).unwrap();
        assert!(c.is_empty());
        let big = format!("{}z", s.device());
        let c = s
            .apply_remote(&[row("theme", "tie-high", at, &big)])
            .unwrap();
        assert_eq!(c, [("theme".to_string(), Some("tie-high".to_string()))]);

        // Newer: wins, and is recorded as already synced.
        let c = s
            .apply_remote(&[row("theme", "newer", at + 10, "laptop")])
            .unwrap();
        assert_eq!(c, [("theme".to_string(), Some("newer".to_string()))]);
        assert!(s.pending(10).unwrap().is_empty());

        // A newer tombstone deletes; the same state again reports nothing.
        let mut gone = row("theme", "", at + 20, "laptop");
        gone.deleted = true;
        assert_eq!(
            s.apply_remote(std::slice::from_ref(&gone)).unwrap(),
            [("theme".to_string(), None)]
        );
        assert_eq!(s.get("theme").unwrap(), None);
        assert!(s.apply_remote(&[gone]).unwrap().is_empty());

        // A key we haven't seen is created.
        let c = s
            .apply_remote(&[row("fontSize", "14", 5, "laptop")])
            .unwrap();
        assert_eq!(c, [("fontSize".to_string(), Some("14".to_string()))]);
    }

    #[test]
    fn apply_remote_ignores_keys_that_do_not_sync() {
        let s = Store::in_memory().unwrap();
        let c = s
            .apply_remote(&[row("recentProjects", "[]", 5, "laptop")])
            .unwrap();
        assert!(c.is_empty());
        assert_eq!(s.get("recentProjects").unwrap(), None);
    }

    #[test]
    fn a_local_change_outranks_a_remote_value_from_a_fast_clock() {
        let s = Store::in_memory().unwrap();
        let future = now_ms() + 3_600_000;
        s.apply_remote(&[row("theme", "remote", future, "laptop")])
            .unwrap();
        assert!(s.set("theme", Some("mine")).unwrap());
        assert_eq!(s.stamp("theme").unwrap().unwrap().0, future + 1);
        let c = s
            .apply_remote(&[row("theme", "remote", future, "laptop")])
            .unwrap();
        assert!(c.is_empty());
        assert_eq!(s.get("theme").unwrap().as_deref(), Some("mine"));
    }

    #[test]
    fn the_device_id_is_stable() {
        let dir = std::env::temp_dir().join(format!("prefs-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("prefs.db");
        let a = Store::open(&path).unwrap();
        let id = a.device().to_string();
        a.set("theme", Some("dark")).unwrap();
        drop(a);
        let b = Store::open(&path).unwrap();
        assert_eq!(b.device(), id);
        assert_eq!(b.get("theme").unwrap().as_deref(), Some("dark"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
