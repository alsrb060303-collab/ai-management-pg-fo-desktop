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
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, String>;

/// Upper bounds so a runaway paste cannot bloat the database.
const MAX_NOTE: usize = 200_000;
const MAX_TODO: usize = 2_000;

fn db_err(e: rusqlite::Error) -> String {
    crate::tr!("메모 저장소 오류: {e}", "Memo storage error: {e}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A branch has one note, so its id follows from the branch: two devices
/// writing the first note of a branch create the same row, not two.
fn note_id(repo: &str, branch: &str) -> String {
    let hash = Sha256::digest(format!("{repo}\0{branch}").as_bytes());
    let hex: String = hash[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("note:{hex}")
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
            std::fs::create_dir_all(dir).map_err(|e| {
                crate::tr!(
                    "메모 폴더를 만들 수 없어요: {e}",
                    "Couldn't create the memo folder: {e}"
                )
            })?;
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
            return Err(crate::tr!("메모가 너무 길어요", "The memo is too long"));
        }
        let conn = self.conn();
        let now = now_ms();
        let id = note_id(repo, branch);
        let updated = conn
            .execute(
                "UPDATE items SET text = ?2, updated_at = ?3, device_id = ?4, deleted = 0
                 WHERE id = ?1",
                params![id, text, now, self.device],
            )
            .map_err(db_err)?;
        if updated == 0 && !text.is_empty() {
            conn.execute(
                "INSERT INTO items (id, repo_key, branch, kind, text, done, position,
                                    created_at, updated_at, deleted, device_id)
                 VALUES (?1, ?2, ?3, 'note', ?4, 0, 0, ?5, ?5, 0, ?6)",
                params![id, repo, branch, text, now, self.device],
            )
            .map_err(db_err)?;
        }
        Ok(())
    }

    pub fn add_todo(&self, repo: &str, branch: &str, text: &str) -> Result<Todo> {
        let text = text.trim();
        if text.is_empty() {
            return Err(crate::tr!("할 일을 입력해 주세요", "Enter a to-do"));
        }
        if text.len() > MAX_TODO {
            return Err(crate::tr!("할 일이 너무 길어요", "The to-do is too long"));
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
            return Err(crate::tr!(
                "내용이 비어 있어요. 지우려면 삭제를 눌러 주세요",
                "It's empty. To remove it, press Delete"
            ));
        }
        if text.is_some_and(|t| t.len() > MAX_TODO) {
            return Err(crate::tr!("할 일이 너무 길어요", "The to-do is too long"));
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
            return Err(crate::tr!(
                "이미 지워진 할 일이에요",
                "This to-do was already deleted"
            ));
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
            return Err(crate::tr!(
                "이미 지워진 할 일이에요",
                "This to-do was already deleted"
            ));
        };
        let moved = ids.remove(from);
        let index = index.min(ids.len());
        ids.insert(index, moved);
        let now = now_ms();
        // Only the moved row changes: it takes a position between its new
        // neighbours, so a sync merge touches one row, not the whole list.
        let position = |id: &str| -> Result<f64> {
            tx.query_row("SELECT position FROM items WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .map_err(db_err)
        };
        let before = index
            .checked_sub(1)
            .map(|i| position(&ids[i]))
            .transpose()?;
        let after = ids.get(index + 1).map(|id| position(id)).transpose()?;
        let target = match (before, after) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) => a + 1.0,
            (None, Some(b)) => b - 1.0,
            (None, None) => 1.0,
        };
        let fits = before.is_none_or(|a| a < target) && after.is_none_or(|b| target < b);
        if fits {
            tx.execute(
                "UPDATE items SET position = ?2, updated_at = ?3, device_id = ?4 WHERE id = ?1",
                params![id, target, now, self.device],
            )
            .map_err(db_err)?;
        } else {
            // Ties or a gap too small to split: renumber the list.
            for (i, item) in ids.iter().enumerate() {
                tx.execute(
                    "UPDATE items SET position = ?2, updated_at = ?3, device_id = ?4
                     WHERE id = ?1 AND position != ?2",
                    params![item, (i + 1) as f64, now, self.device],
                )
                .map_err(db_err)?;
            }
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

    /// Keeps memos with a repository whose key changed: a remote was added
    /// (memos were under the folder key) or the remote URL changed (memos
    /// were under the old URL). `path_key` stays the same across both, so it
    /// remembers the key used last. Nothing moves when the new key already
    /// has memos.
    pub fn follow_repo(&self, path_key: &str, key: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let last: Option<String> = tx
            .query_row(
                "SELECT repo_key FROM repo_paths WHERE path_key = ?1",
                [path_key],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        let from = last.unwrap_or_else(|| path_key.to_string());
        if from != key {
            let taken: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM items WHERE repo_key = ?1)",
                    [key],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            if !taken {
                let now = now_ms();
                // Note ids derive from the key; tombstones move too.
                let notes: Vec<(String, String)> = {
                    let mut stmt = tx
                        .prepare(
                            "SELECT id, branch FROM items WHERE repo_key = ?1 AND kind = 'note'",
                        )
                        .map_err(db_err)?;
                    stmt.query_map([&from], |r| Ok((r.get(0)?, r.get(1)?)))
                        .map_err(db_err)?
                        .collect::<rusqlite::Result<_>>()
                        .map_err(db_err)?
                };
                for (id, branch) in notes {
                    tx.execute(
                        "UPDATE items SET id = ?2 WHERE id = ?1",
                        params![id, note_id(key, &branch)],
                    )
                    .map_err(db_err)?;
                }
                tx.execute(
                    "UPDATE items SET repo_key = ?2, updated_at = ?3, device_id = ?4
                     WHERE repo_key = ?1",
                    params![from, key, now, self.device],
                )
                .map_err(db_err)?;
            }
        }
        tx.execute(
            "INSERT INTO repo_paths (path_key, repo_key) VALUES (?1, ?2)
             ON CONFLICT (path_key) DO UPDATE SET repo_key = excluded.repo_key",
            params![path_key, key],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)
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
    if version < 2 {
        // Which key each local repository used last (device-local, not synced),
        // and note ids derived from their branch.
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS repo_paths (
                 path_key TEXT PRIMARY KEY,
                 repo_key TEXT NOT NULL
             );",
        )
        .map_err(db_err)?;
        let renamed = (|| -> rusqlite::Result<()> {
            let notes: Vec<(String, String, String)> = conn
                .prepare("SELECT id, repo_key, branch FROM items WHERE kind = 'note'")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (id, repo, branch) in notes {
                conn.execute(
                    "UPDATE items SET id = ?2 WHERE id = ?1",
                    params![id, note_id(&repo, &branch)],
                )?;
            }
            conn.execute_batch("PRAGMA user_version = 2; COMMIT;")
        })();
        if let Err(e) = renamed {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(db_err(e));
        }
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
    fn notes_have_ids_derived_from_their_branch() {
        let a = Store::in_memory().unwrap();
        let b = Store::in_memory().unwrap();
        a.set_note("r", "main", "x").unwrap();
        b.set_note("r", "main", "y").unwrap();
        let id = |s: &Store| -> String {
            s.conn()
                .query_row("SELECT id FROM items WHERE kind = 'note'", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(id(&a), id(&b));
        assert_ne!(note_id("r", "main"), note_id("r", "dev"));
    }

    #[test]
    fn moving_a_todo_changes_only_that_row() {
        let s = Store::in_memory().unwrap();
        let ids: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|t| s.add_todo("r", "main", t).unwrap().id)
            .collect();
        let order = || -> Vec<String> {
            s.branch("r", "main")
                .unwrap()
                .todos
                .into_iter()
                .map(|t| t.text)
                .collect()
        };
        let stamps = || -> Vec<i64> {
            s.branch("r", "main")
                .unwrap()
                .todos
                .iter()
                .map(|t| t.updated_at)
                .collect()
        };
        s.conn()
            .execute("UPDATE items SET updated_at = 1", [])
            .unwrap();
        s.move_todo(&ids[3], 1).unwrap();
        assert_eq!(order(), ["a", "d", "b", "c"]);
        assert_eq!(stamps().iter().filter(|&&t| t == 1).count(), 3);
        s.move_todo(&ids[0], 3).unwrap();
        assert_eq!(order(), ["d", "b", "c", "a"]);
        s.move_todo(&ids[2], 0).unwrap();
        assert_eq!(order(), ["c", "d", "b", "a"]);

        // Ties fall back to renumbering.
        s.conn()
            .execute("UPDATE items SET position = 5", [])
            .unwrap();
        let first = order();
        s.move_todo(&ids[1], 0).unwrap();
        let mut expected = first.clone();
        expected.retain(|t| t != "b");
        expected.insert(0, "b".into());
        assert_eq!(order(), expected);
    }

    #[test]
    fn memos_follow_a_repository_whose_key_changed() {
        let s = Store::in_memory().unwrap();
        s.set_note("path:/r", "main", "note").unwrap();
        s.add_todo("path:/r", "main", "todo").unwrap();

        // First seen with no remote, then a remote is added.
        s.follow_repo("path:/r", "path:/r").unwrap();
        s.follow_repo("path:/r", "github.com/me/r").unwrap();
        let memo = s.branch("github.com/me/r", "main").unwrap();
        assert_eq!(memo.note, "note");
        assert_eq!(memo.todos.len(), 1);
        assert!(s.counts("path:/r").unwrap().is_empty());
        // The moved note keeps working with its new key.
        s.set_note("github.com/me/r", "main", "edited").unwrap();
        assert_eq!(s.branch("github.com/me/r", "main").unwrap().note, "edited");

        // The remote is renamed.
        s.follow_repo("path:/r", "github.com/me/r2").unwrap();
        assert_eq!(s.branch("github.com/me/r2", "main").unwrap().note, "edited");

        // A key that already has memos (another clone) is never overwritten.
        s.add_todo("github.com/me/other", "main", "theirs").unwrap();
        s.follow_repo("path:/r", "github.com/me/other").unwrap();
        assert_eq!(
            s.branch("github.com/me/other", "main").unwrap().todos.len(),
            1
        );
        assert_eq!(s.branch("github.com/me/r2", "main").unwrap().note, "edited");
    }

    #[test]
    fn migration_derives_note_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memo.db");
        {
            let conn = Connection::open(&path).unwrap();
            migrate_v1_only(&conn);
            conn.execute(
                "INSERT INTO items (id, repo_key, branch, kind, text, created_at, updated_at, device_id)
                 VALUES ('random', 'r', 'main', 'note', 'old', 1, 1, 'd')",
                [],
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        s.set_note("r", "main", "new").unwrap();
        let count: i64 = s
            .conn()
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(s.branch("r", "main").unwrap().note, "new");
    }

    fn migrate_v1_only(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE items (
                 id TEXT PRIMARY KEY, repo_key TEXT NOT NULL, branch TEXT NOT NULL,
                 kind TEXT NOT NULL, text TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0,
                 position REAL NOT NULL DEFAULT 0, created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
                 device_id TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )
        .unwrap();
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
