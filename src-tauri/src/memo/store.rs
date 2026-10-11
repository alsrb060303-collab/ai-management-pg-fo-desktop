//! Local SQLite store for branch notes and TODOs.
//!
//! The schema is ready for the paid sync adapter: every row has an
//! `updated_at` and the device that changed it last, and deletions are kept
//! as tombstones (`deleted = 1`) so they can be propagated.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

pub type Result<T> = std::result::Result<T, String>;

/// Upper bounds so a runaway paste cannot bloat the database.
const MAX_NOTE: usize = 200_000;
const MAX_TODO: usize = 2_000;

fn db_err(e: rusqlite::Error) -> String {
    format!("메모 저장소 오류: {e}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub position: f64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchMemo {
    pub note: String,
    /// `None` until a note was saved.
    pub note_updated_at: Option<i64>,
    pub todos: Vec<Todo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCounts {
    pub branch: String,
    pub open: u32,
    pub total: u32,
    pub has_note: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
    device: String,
}

fn todo_from(row: &Row) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get("id")?,
        text: row.get("text")?,
        done: row.get::<_, i64>("done")? != 0,
        position: row.get("position")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("메모 폴더를 만들 수 없어요: {e}"))?;
        }
        let conn = Connection::open(path).map_err(db_err)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_err)?;
        // WAL keeps readers and the writer from blocking each other.
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

    pub fn branch(&self, repo: &str, branch: &str) -> Result<BranchMemo> {
        let conn = self.conn();
        let note: Option<(String, i64)> = conn
            .query_row(
                "SELECT text, updated_at FROM items
                 WHERE repo_key = ?1 AND branch = ?2 AND kind = 'note' AND deleted = 0",
                params![repo, branch],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let mut stmt = conn
            .prepare(
                "SELECT * FROM items
                 WHERE repo_key = ?1 AND branch = ?2 AND kind = 'todo' AND deleted = 0
                 ORDER BY position, created_at",
            )
            .map_err(db_err)?;
        let todos = stmt
            .query_map(params![repo, branch], todo_from)
            .map_err(db_err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_err)?;
        let (note, note_updated_at) = match note {
            Some((text, at)) => (text, Some(at)),
            None => (String::new(), None),
        };
        Ok(BranchMemo {
            note,
            note_updated_at,
            todos,
        })
    }

    /// Saves the branch note (one per branch).
    pub fn set_note(&self, repo: &str, branch: &str, text: &str) -> Result<()> {
        if text.len() > MAX_NOTE {
            return Err("메모가 너무 길어요".into());
        }
        let conn = self.conn();
        let now = now_ms();
        let updated = conn
            .execute(
                "UPDATE items SET text = ?3, updated_at = ?4, device_id = ?5, deleted = 0
                 WHERE repo_key = ?1 AND branch = ?2 AND kind = 'note'",
                params![repo, branch, text, now, self.device],
            )
            .map_err(db_err)?;
        if updated == 0 && !text.is_empty() {
            conn.execute(
                "INSERT INTO items (id, repo_key, branch, kind, text, done, position,
                                    created_at, updated_at, deleted, device_id)
                 VALUES (?1, ?2, ?3, 'note', ?4, 0, 0, ?5, ?5, 0, ?6)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    repo,
                    branch,
                    text,
                    now,
                    self.device
                ],
            )
            .map_err(db_err)?;
        }
        Ok(())
    }

    pub fn add_todo(&self, repo: &str, branch: &str, text: &str) -> Result<Todo> {
        let text = text.trim();
        if text.is_empty() {
            return Err("할 일을 입력해 주세요".into());
        }
        if text.len() > MAX_TODO {
            return Err("할 일이 너무 길어요".into());
        }
        let conn = self.conn();
        let next: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), 0) + 1 FROM items
                 WHERE repo_key = ?1 AND branch = ?2 AND kind = 'todo' AND deleted = 0",
                params![repo, branch],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO items (id, repo_key, branch, kind, text, done, position,
                                created_at, updated_at, deleted, device_id)
             VALUES (?1, ?2, ?3, 'todo', ?4, 0, ?5, ?6, ?6, 0, ?7)",
            params![id, repo, branch, text, next, now, self.device],
        )
        .map_err(db_err)?;
        Ok(Todo {
            id,
            text: text.to_string(),
            done: false,
            position: next,
            created_at: now,
            updated_at: now,
        })
    }

    /// Changes a TODO's text and/or done state. Returns its branch key.
    pub fn update_todo(
        &self,
        id: &str,
        text: Option<&str>,
        done: Option<bool>,
    ) -> Result<(String, String)> {
        let text = text.map(str::trim);
        if text.is_some_and(str::is_empty) {
            return Err("내용이 비어 있어요. 지우려면 삭제를 눌러 주세요".into());
        }
        if text.is_some_and(|t| t.len() > MAX_TODO) {
            return Err("할 일이 너무 길어요".into());
        }
        let conn = self.conn();
        let changed = conn
            .execute(
                "UPDATE items SET text = COALESCE(?2, text), done = COALESCE(?3, done),
                        updated_at = ?4, device_id = ?5
                 WHERE id = ?1 AND kind = 'todo' AND deleted = 0",
                params![id, text, done.map(i64::from), now_ms(), self.device],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err("이미 지워진 할 일이에요".into());
        }
        Self::key_of(&conn, id)
    }

    fn key_of(conn: &Connection, id: &str) -> Result<(String, String)> {
        conn.query_row(
            "SELECT repo_key, branch FROM items WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db_err)
    }

    /// Deletes a TODO (kept as a tombstone for sync). Returns its branch key.
    pub fn delete_todo(&self, id: &str) -> Result<(String, String)> {
        let conn = self.conn();
        conn.execute(
            "UPDATE items SET deleted = 1, updated_at = ?2, device_id = ?3
             WHERE id = ?1 AND kind = 'todo' AND deleted = 0",
            params![id, now_ms(), self.device],
        )
        .map_err(db_err)?;
        Self::key_of(&conn, id)
    }

    /// Moves a TODO to `index` among its branch's live TODOs.
    pub fn move_todo(&self, id: &str, index: usize) -> Result<(String, String)> {
        let mut conn = self.conn();
        let (repo, branch) = Self::key_of(&conn, id)?;
        let tx = conn.transaction().map_err(db_err)?;
        let mut ids: Vec<String> = {
            let mut stmt = tx
                .prepare(
                    "SELECT id FROM items
                     WHERE repo_key = ?1 AND branch = ?2 AND kind = 'todo' AND deleted = 0
                     ORDER BY position, created_at",
                )
                .map_err(db_err)?;
            stmt.query_map(params![repo, branch], |r| r.get(0))
                .map_err(db_err)?
                .collect::<rusqlite::Result<_>>()
                .map_err(db_err)?
        };
        let Some(from) = ids.iter().position(|x| x == id) else {
            return Err("이미 지워진 할 일이에요".into());
        };
        let moved = ids.remove(from);
        ids.insert(index.min(ids.len()), moved);
        let now = now_ms();
        for (i, item) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE items SET position = ?2, updated_at = ?3, device_id = ?4
                 WHERE id = ?1 AND position != ?2",
                params![item, (i + 1) as f64, now, self.device],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok((repo, branch))
    }

    /// Deletes the branch's finished TODOs.
    pub fn clear_done(&self, repo: &str, branch: &str) -> Result<usize> {
        self.conn()
            .execute(
                "UPDATE items SET deleted = 1, updated_at = ?3, device_id = ?4
                 WHERE repo_key = ?1 AND branch = ?2 AND kind = 'todo' AND done = 1 AND deleted = 0",
                params![repo, branch, now_ms(), self.device],
            )
            .map_err(db_err)
    }

    /// Branches of a repository that have a note or TODOs.
    pub fn counts(&self, repo: &str) -> Result<Vec<BranchCounts>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT branch,
                        SUM(kind = 'todo' AND done = 0),
                        SUM(kind = 'todo'),
                        SUM(kind = 'note' AND text != '')
                 FROM items WHERE repo_key = ?1 AND deleted = 0
                 GROUP BY branch",
            )
            .map_err(db_err)?;
        stmt.query_map([repo], |r| {
            Ok(BranchCounts {
                branch: r.get(0)?,
                open: r.get::<_, i64>(1)? as u32,
                total: r.get::<_, i64>(2)? as u32,
                has_note: r.get::<_, i64>(3)? > 0,
            })
        })
        .map_err(db_err)?
        .filter(|c| !c.as_ref().is_ok_and(|c| c.total == 0 && !c.has_note))
        .collect::<rusqlite::Result<_>>()
        .map_err(db_err)
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
             CREATE TABLE IF NOT EXISTS items (
                 id TEXT PRIMARY KEY,
                 repo_key TEXT NOT NULL,
                 branch TEXT NOT NULL,
                 kind TEXT NOT NULL CHECK (kind IN ('note', 'todo')),
                 text TEXT NOT NULL,
                 done INTEGER NOT NULL DEFAULT 0,
                 position REAL NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 deleted INTEGER NOT NULL DEFAULT 0,
                 device_id TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS items_branch ON items (repo_key, branch, kind, deleted);
             CREATE INDEX IF NOT EXISTS items_updated ON items (updated_at);
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

    #[test]
    fn notes_are_one_per_branch() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.branch("r", "main").unwrap(), BranchMemo::default());
        s.set_note("r", "main", "first").unwrap();
        s.set_note("r", "main", "second").unwrap();
        s.set_note("r", "dev", "other").unwrap();
        let memo = s.branch("r", "main").unwrap();
        assert_eq!(memo.note, "second");
        assert!(memo.note_updated_at.is_some());
        // An empty note on a branch without one creates nothing.
        s.set_note("r", "empty", "").unwrap();
        assert_eq!(s.counts("r").unwrap().len(), 2);
    }

    #[test]
    fn todos_add_update_reorder_and_delete() {
        let s = Store::in_memory().unwrap();
        let a = s.add_todo("r", "main", "  a ").unwrap();
        let b = s.add_todo("r", "main", "b").unwrap();
        let c = s.add_todo("r", "main", "c").unwrap();
        assert_eq!(a.text, "a");
        assert!(s.add_todo("r", "main", "   ").is_err());

        s.update_todo(&b.id, None, Some(true)).unwrap();
        s.update_todo(&c.id, Some("c2"), None).unwrap();
        assert!(s.update_todo(&c.id, Some(" "), None).is_err());

        assert_eq!(s.move_todo(&c.id, 0).unwrap(), ("r".into(), "main".into()));
        let texts: Vec<_> = s
            .branch("r", "main")
            .unwrap()
            .todos
            .into_iter()
            .map(|t| (t.text, t.done))
            .collect();
        assert_eq!(
            texts,
            [
                ("c2".into(), false),
                ("a".into(), false),
                ("b".into(), true)
            ]
        );

        assert_eq!(s.clear_done("r", "main").unwrap(), 1);
        s.delete_todo(&a.id).unwrap();
        let left: Vec<_> = s
            .branch("r", "main")
            .unwrap()
            .todos
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(left, ["c2"]);
        assert!(s.update_todo(&a.id, None, Some(true)).is_err());

        let counts = s.counts("r").unwrap();
        assert_eq!(
            counts,
            [BranchCounts {
                branch: "main".into(),
                open: 1,
                total: 1,
                has_note: false
            }]
        );
    }

    #[test]
    fn deletions_are_kept_as_tombstones_for_sync() {
        let s = Store::in_memory().unwrap();
        let t = s.add_todo("r", "main", "x").unwrap();
        s.delete_todo(&t.id).unwrap();
        let conn = s.conn();
        let (deleted, device): (i64, String) = conn
            .query_row(
                "SELECT deleted, device_id FROM items WHERE id = ?1",
                [&t.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(device, s.device);
    }

    #[test]
    fn reopening_keeps_data_and_device_id() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memo.db");
        let device = {
            let s = Store::open(&path).unwrap();
            s.add_todo("r", "main", "keep").unwrap();
            s.device.clone()
        };
        let s = Store::open(&path).unwrap();
        assert_eq!(s.device, device);
        assert_eq!(s.branch("r", "main").unwrap().todos.len(), 1);
    }
}
