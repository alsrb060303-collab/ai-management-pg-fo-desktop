//! Local SQLite store for the work-tree logs (`<app data>/activity.db`).
//!
//! Same shape as the memo store: every row has `updated_at` and the device
//! that changed it last, deletions (also retention) are tombstones
//! (`deleted = 1`) so they propagate, and `synced_at` is the `updated_at`
//! last uploaded or applied from the server (a row whose two differ waits to
//! be uploaded).

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{BranchTip, GitEvent, GitKind, PromptEvent, SessionInfo};

pub type Result<T> = std::result::Result<T, String>;

pub const DEFAULT_RETENTION_DAYS: u32 = 90;
const RETENTION_KEY: &str = "retention_days";
const SYNC_PROMPTS_KEY: &str = "sync_prompts";
const DAY_MS: i64 = 86_400_000;

fn db_err(e: rusqlite::Error) -> String {
    crate::tr!(
        "활동 기록 저장소 오류: {e}",
        "Activity log storage error: {e}"
    )
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// How long events are kept, and whether prompts leave this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// `None` keeps events forever.
    pub retention_days: Option<u32>,
    pub sync_prompts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            retention_days: Some(DEFAULT_RETENTION_DAYS),
            sync_prompts: true,
        }
    }
}

impl Settings {
    /// Events at or before this time (ms) are past retention.
    pub fn cutoff(&self, now: i64) -> Option<i64> {
        self.retention_days
            .map(|d| now.saturating_sub(i64::from(d) * DAY_MS))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Git(GitEvent),
    Prompt(PromptEvent),
}

impl Event {
    pub fn id(&self) -> &str {
        match self {
            Event::Git(e) => &e.id,
            Event::Prompt(e) => &e.id,
        }
    }

    pub fn repo(&self) -> &str {
        match self {
            Event::Git(e) => &e.repo,
            Event::Prompt(e) => &e.repo,
        }
    }
}

/// An event as it travels between devices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncRow {
    pub updated_at: i64,
    pub deleted: bool,
    pub device_id: String,
    pub event: Event,
}

fn to_text<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn from_text<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

fn list_col(row: &Row, name: &str) -> rusqlite::Result<Vec<String>> {
    Ok(from_text(&row.get::<_, String>(name)?).unwrap_or_default())
}

fn kind_text(kind: GitKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn git_from(row: &Row) -> rusqlite::Result<GitEvent> {
    let kind: String = row.get("kind")?;
    Ok(GitEvent {
        id: row.get("id")?,
        repo: row.get("repo")?,
        kind: serde_json::from_value(serde_json::Value::String(kind)).unwrap_or(GitKind::Commit),
        at: row.get("at")?,
        branch: row.get("branch")?,
        sha: row.get("sha")?,
        parents: list_col(row, "parents")?,
        pr: row.get::<_, Option<i64>>("pr")?.map(|n| n as u64),
        title: row.get("title")?,
        state: row.get("state")?,
        url: row.get("url")?,
        author: row.get("author")?,
    })
}

fn prompt_from(row: &Row) -> rusqlite::Result<PromptEvent> {
    let tool: String = row.get("tool")?;
    Ok(PromptEvent {
        id: row.get("id")?,
        repo: row.get("repo")?,
        tool: serde_json::from_value(serde_json::Value::String(tool)).unwrap_or_default(),
        session: row.get("session")?,
        at: row.get("at")?,
        branch: row.get("branch")?,
        text: row.get("text")?,
        files: list_col(row, "files")?,
        commits: list_col(row, "commits")?,
        pushed: row.get::<_, i64>("pushed")? != 0,
    })
}

fn tool_text(e: &PromptEvent) -> String {
    serde_json::to_value(e.tool)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The event with its content fields emptied (what a tombstone keeps).
pub fn tombstone(event: &Event) -> Event {
    match event {
        Event::Git(e) => Event::Git(GitEvent {
            title: String::new(),
            branch: None,
            parents: Vec::new(),
            url: None,
            author: None,
            ..e.clone()
        }),
        Event::Prompt(e) => Event::Prompt(PromptEvent {
            text: String::new(),
            branch: None,
            files: Vec::new(),
            commits: Vec::new(),
            ..e.clone()
        }),
    }
}

/// Writes (inserts or replaces) one event row with the given bookkeeping.
fn write_event(
    conn: &Connection,
    event: &Event,
    updated_at: i64,
    device: &str,
    synced_at: Option<i64>,
    deleted: bool,
) -> rusqlite::Result<()> {
    let blanked;
    let event = if deleted {
        blanked = tombstone(event);
        &blanked
    } else {
        event
    };
    match event {
        Event::Git(e) => conn.execute(
            "INSERT INTO git_events (id, repo, kind, at, branch, sha, parents, pr, title,
                                     state, url, author, updated_at, device_id, synced_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT (id) DO UPDATE SET
                repo = excluded.repo, kind = excluded.kind, at = excluded.at,
                branch = excluded.branch, sha = excluded.sha, parents = excluded.parents,
                pr = excluded.pr, title = excluded.title, state = excluded.state,
                url = excluded.url, author = excluded.author, updated_at = excluded.updated_at,
                device_id = excluded.device_id, synced_at = excluded.synced_at,
                deleted = excluded.deleted",
            params![
                e.id,
                e.repo,
                kind_text(e.kind),
                e.at,
                e.branch,
                e.sha,
                to_text(&e.parents),
                e.pr.map(|n| n as i64),
                e.title,
                e.state,
                e.url,
                e.author,
                updated_at,
                device,
                synced_at,
                deleted
            ],
        ),
        Event::Prompt(e) => conn.execute(
            "INSERT INTO prompt_events (id, repo, tool, session, at, branch, text, files,
                                        commits, pushed, updated_at, device_id, synced_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT (id) DO UPDATE SET
                repo = excluded.repo, tool = excluded.tool, session = excluded.session,
                at = excluded.at, branch = excluded.branch, text = excluded.text,
                files = excluded.files, commits = excluded.commits, pushed = excluded.pushed,
                updated_at = excluded.updated_at, device_id = excluded.device_id,
                synced_at = excluded.synced_at, deleted = excluded.deleted",
            params![
                e.id,
                e.repo,
                tool_text(e),
                e.session,
                e.at,
                e.branch,
                e.text,
                to_text(&e.files),
                to_text(&e.commits),
                e.pushed,
                updated_at,
                device,
                synced_at,
                deleted
            ],
        ),
    }
    .map(drop)
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
                    "활동 기록 폴더를 만들 수 없어요: {e}",
                    "Couldn't create the activity log folder: {e}"
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

    // ---- meta, settings, reader cursors ----

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

    pub fn settings(&self) -> Result<Settings> {
        let retention_days = match self.meta(RETENTION_KEY)?.as_deref() {
            None => Some(DEFAULT_RETENTION_DAYS),
            Some("never") => None,
            Some(n) => Some(n.parse().unwrap_or(DEFAULT_RETENTION_DAYS)),
        };
        Ok(Settings {
            retention_days,
            sync_prompts: self.meta(SYNC_PROMPTS_KEY)?.as_deref() != Some("0"),
        })
    }

    pub fn set_settings(&self, settings: Settings) -> Result<()> {
        let days = settings.retention_days.map(|d| d.to_string());
        self.set_meta(RETENTION_KEY, Some(days.as_deref().unwrap_or("never")))?;
        self.set_meta(
            SYNC_PROMPTS_KEY,
            Some(if settings.sync_prompts { "1" } else { "0" }),
        )
    }

    /// The readers' cursor for `key` (opaque to the store); `Null` if none.
    pub fn scan_state(&self, key: &str) -> Result<serde_json::Value> {
        let text: Option<String> = self
            .conn()
            .query_row("SELECT value FROM scan_state WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_err)?;
        Ok(text
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null))
    }

    pub fn set_scan_state(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO scan_state (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value.to_string()],
            )
            .map(drop)
            .map_err(db_err)
    }

    pub fn set_tips(&self, repo: &str, tips: &[BranchTip]) -> Result<()> {
        self.set_meta(&format!("tips:{repo}"), Some(&to_text(&tips)))
    }

    pub fn tips(&self, repo: &str) -> Result<Vec<BranchTip>> {
        Ok(self
            .meta(&format!("tips:{repo}"))?
            .and_then(|t| from_text(&t))
            .unwrap_or_default())
    }

    // ---- collecting ----

    /// Stores collected events; returns how many were new or changed. Events
    /// that did not change keep their `updated_at`, and a tombstone stays
    /// (an event pruned or deleted is not brought back by collecting again).
    pub fn upsert(&self, events: &[Event]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let now = now_ms();
        let mut changed = 0;
        for event in events {
            let existing: Option<(Event, i64, bool)> = match event {
                Event::Git(e) => tx
                    .query_row("SELECT * FROM git_events WHERE id = ?1", [&e.id], |r| {
                        Ok((
                            Event::Git(git_from(r)?),
                            r.get("updated_at")?,
                            r.get::<_, i64>("deleted")? != 0,
                        ))
                    })
                    .optional(),
                Event::Prompt(e) => tx
                    .query_row("SELECT * FROM prompt_events WHERE id = ?1", [&e.id], |r| {
                        Ok((
                            Event::Prompt(prompt_from(r)?),
                            r.get("updated_at")?,
                            r.get::<_, i64>("deleted")? != 0,
                        ))
                    })
                    .optional(),
            }
            .map_err(db_err)?;
            let updated_at = match &existing {
                Some((_, _, true)) => continue,
                Some((old, _, false)) if old == event => continue,
                Some((_, at, _)) => now.max(at + 1),
                None => now,
            };
            write_event(&tx, event, updated_at, &self.device, None, false).map_err(db_err)?;
            changed += 1;
        }
        tx.commit().map_err(db_err)?;
        Ok(changed)
    }

    /// Tombstones events older than `cutoff` (ms). Returns how many.
    pub fn prune(&self, cutoff: i64) -> Result<usize> {
        let conn = self.conn();
        let now = now_ms();
        let mut n = 0;
        // A tombstone keeps no content: only the ids and times that make it
        // win (and stay deleted) on every device.
        for (table, blank) in [
            (
                "git_events",
                "title = '', branch = NULL, parents = '[]', url = NULL, author = NULL",
            ),
            (
                "prompt_events",
                "text = '', branch = NULL, files = '[]', commits = '[]'",
            ),
        ] {
            n += conn
                .execute(
                    &format!(
                        "UPDATE {table} SET deleted = 1, updated_at = MAX(?1, updated_at + 1),
                                device_id = ?2, {blank}
                         WHERE deleted = 0 AND at < ?3"
                    ),
                    params![now, self.device, cutoff],
                )
                .map_err(db_err)?;
        }
        Ok(n)
    }

    // ---- reading ----

    /// Live commits and other GitHub events of a repository, newest first.
    /// The cursor is `(before, before_id)`: events strictly older, or at the
    /// same time with a smaller id (commit times tie often); with only
    /// `before`, events strictly older.
    pub fn git_page(
        &self,
        repo: &str,
        limit: usize,
        before: Option<i64>,
        before_id: Option<&str>,
    ) -> Result<Vec<GitEvent>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM git_events
                 WHERE repo = ?1 AND deleted = 0
                   AND (at < ?2 OR (at = ?2 AND ?4 IS NOT NULL AND id < ?4))
                 ORDER BY at DESC, id DESC LIMIT ?3",
            )
            .map_err(db_err)?;
        stmt.query_map(
            params![repo, before.unwrap_or(i64::MAX), limit as i64, before_id],
            git_from,
        )
        .map_err(db_err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(db_err)
    }

    pub fn prompt_page(
        &self,
        repo: &str,
        limit: usize,
        before: Option<i64>,
        before_id: Option<&str>,
    ) -> Result<Vec<PromptEvent>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM prompt_events
                 WHERE repo = ?1 AND deleted = 0
                   AND (at < ?2 OR (at = ?2 AND ?4 IS NOT NULL AND id < ?4))
                 ORDER BY at DESC, id DESC LIMIT ?3",
            )
            .map_err(db_err)?;
        stmt.query_map(
            params![repo, before.unwrap_or(i64::MAX), limit as i64, before_id],
            prompt_from,
        )
        .map_err(db_err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(db_err)
    }

    // ---- sync ----

    /// Rows changed here since they were last synced (tombstones included),
    /// oldest change first. Prompts are left out unless `prompts`.
    pub fn pending(&self, limit: usize, prompts: bool) -> Result<Vec<SyncRow>> {
        let conn = self.conn();
        let mut rows: Vec<SyncRow> = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM git_events WHERE synced_at IS NULL OR synced_at != updated_at
                 ORDER BY updated_at LIMIT ?1",
            )
            .map_err(db_err)?;
        for row in stmt
            .query_map([limit as i64], |r| {
                Ok(SyncRow {
                    updated_at: r.get("updated_at")?,
                    deleted: r.get::<_, i64>("deleted")? != 0,
                    device_id: r.get("device_id")?,
                    event: Event::Git(git_from(r)?),
                })
            })
            .map_err(db_err)?
        {
            rows.push(row.map_err(db_err)?);
        }
        if prompts {
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM prompt_events WHERE synced_at IS NULL OR synced_at != updated_at
                     ORDER BY updated_at LIMIT ?1",
                )
                .map_err(db_err)?;
            for row in stmt
                .query_map([limit as i64], |r| {
                    Ok(SyncRow {
                        updated_at: r.get("updated_at")?,
                        deleted: r.get::<_, i64>("deleted")? != 0,
                        device_id: r.get("device_id")?,
                        event: Event::Prompt(prompt_from(r)?),
                    })
                })
                .map_err(db_err)?
            {
                rows.push(row.map_err(db_err)?);
            }
        }
        rows.sort_by_key(|r| r.updated_at);
        rows.truncate(limit);
        Ok(rows)
    }

    /// Records rows as uploaded, unless they changed again meanwhile.
    pub fn mark_synced(&self, rows: &[(String, i64)]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        for (id, updated_at) in rows {
            for table in ["git_events", "prompt_events"] {
                tx.execute(
                    &format!(
                        "UPDATE {table} SET synced_at = updated_at
                         WHERE id = ?1 AND updated_at = ?2"
                    ),
                    params![id, updated_at],
                )
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)
    }

    /// Everything needs uploading again (another account, or a reset).
    pub fn mark_all_unsynced(&self) -> Result<()> {
        let conn = self.conn();
        for table in ["git_events", "prompt_events"] {
            conn.execute(&format!("UPDATE {table} SET synced_at = NULL"), [])
                .map_err(db_err)?;
        }
        Ok(())
    }

    /// Applies rows from another device: the newer `updated_at` wins (the
    /// device id breaks ties, the same way the server rules do). Prompt rows
    /// are skipped unless `prompts`. Returns the repositories that changed.
    pub fn apply_remote(&self, rows: &[SyncRow], prompts: bool) -> Result<Vec<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let mut changed: Vec<String> = Vec::new();
        for row in rows {
            let table = match row.event {
                Event::Git(_) => "git_events",
                Event::Prompt(_) if prompts => "prompt_events",
                Event::Prompt(_) => continue,
            };
            let local: Option<(i64, String)> = tx
                .query_row(
                    &format!("SELECT updated_at, device_id FROM {table} WHERE id = ?1"),
                    [row.event.id()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?;
            let newer = match &local {
                None => true,
                Some((at, device)) => {
                    (row.updated_at, row.device_id.as_str()) > (*at, device.as_str())
                }
            };
            if !newer {
                continue;
            }
            write_event(
                &tx,
                &row.event,
                row.updated_at,
                &row.device_id,
                Some(row.updated_at),
                row.deleted,
            )
            .map_err(db_err)?;
            let repo = row.event.repo().to_string();
            if !changed.contains(&repo) {
                changed.push(repo);
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(changed)
    }
}

/// Sessions in a page of prompts, newest session first.
pub fn sessions(events: &[PromptEvent]) -> Vec<SessionInfo> {
    let mut out: Vec<SessionInfo> = Vec::new();
    // `events` is newest first, so the first one seen of a session has its
    // latest branch.
    for e in events {
        match out
            .iter_mut()
            .find(|s| s.tool == e.tool && s.id == e.session)
        {
            Some(s) => {
                s.started = s.started.min(e.at);
                s.ended = s.ended.max(e.at);
                s.prompts += 1;
            }
            None => out.push(SessionInfo {
                tool: e.tool,
                id: e.session.clone(),
                started: e.at,
                ended: e.at,
                branch: e.branch.clone(),
                prompts: 1,
            }),
        }
    }
    out.sort_by(|a, b| b.ended.cmp(&a.ended).then_with(|| a.id.cmp(&b.id)));
    out
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db_err)?;
    if version < 1 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS scan_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS git_events (
                 id TEXT PRIMARY KEY,
                 repo TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 at INTEGER NOT NULL,
                 branch TEXT,
                 sha TEXT,
                 parents TEXT NOT NULL DEFAULT '[]',
                 pr INTEGER,
                 title TEXT NOT NULL,
                 state TEXT,
                 url TEXT,
                 author TEXT,
                 updated_at INTEGER NOT NULL,
                 device_id TEXT NOT NULL,
                 synced_at INTEGER,
                 deleted INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS git_events_repo ON git_events (repo, deleted, at);
             CREATE INDEX IF NOT EXISTS git_events_updated ON git_events (updated_at);
             CREATE TABLE IF NOT EXISTS prompt_events (
                 id TEXT PRIMARY KEY,
                 repo TEXT NOT NULL,
                 tool TEXT NOT NULL,
                 session TEXT NOT NULL,
                 at INTEGER NOT NULL,
                 branch TEXT,
                 text TEXT NOT NULL,
                 files TEXT NOT NULL DEFAULT '[]',
                 commits TEXT NOT NULL DEFAULT '[]',
                 pushed INTEGER NOT NULL DEFAULT 0,
                 updated_at INTEGER NOT NULL,
                 device_id TEXT NOT NULL,
                 synced_at INTEGER,
                 deleted INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS prompt_events_repo ON prompt_events (repo, deleted, at);
             CREATE INDEX IF NOT EXISTS prompt_events_updated ON prompt_events (updated_at);
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
    use crate::tools::ToolId;

    fn git(id: &str, repo: &str, at: i64) -> GitEvent {
        GitEvent {
            id: id.into(),
            repo: repo.into(),
            kind: GitKind::Commit,
            at,
            branch: Some("main".into()),
            sha: Some(id.into()),
            parents: vec!["p1".into(), "p2".into()],
            pr: None,
            title: format!("commit {id}"),
            state: None,
            url: None,
            author: Some("me".into()),
        }
    }

    fn prompt(id: &str, session: &str, at: i64) -> PromptEvent {
        PromptEvent {
            id: id.into(),
            repo: "r".into(),
            tool: ToolId::Codex,
            session: session.into(),
            at,
            branch: Some("dev".into()),
            text: "고쳐 줘".into(),
            files: vec!["a.rs".into()],
            commits: vec!["abc".into()],
            pushed: true,
        }
    }

    fn g(e: GitEvent) -> Event {
        Event::Git(e)
    }

    #[test]
    fn upsert_is_idempotent_and_updates_on_change() {
        let s = Store::in_memory().unwrap();
        assert_eq!(
            s.upsert(&[g(git("a", "r", 10)), g(git("b", "r", 20))])
                .unwrap(),
            2
        );
        let first = s.pending(10, true).unwrap();
        let stamp = first
            .iter()
            .find(|r| r.event.id() == "a")
            .unwrap()
            .updated_at;

        assert_eq!(s.upsert(&[g(git("a", "r", 10))]).unwrap(), 0, "unchanged");
        let again = s.pending(10, true).unwrap();
        assert_eq!(
            again
                .iter()
                .find(|r| r.event.id() == "a")
                .unwrap()
                .updated_at,
            stamp
        );

        let mut changed = git("a", "r", 10);
        changed.title = "amended".into();
        assert_eq!(s.upsert(&[g(changed)]).unwrap(), 1);
        let after = s.pending(10, true).unwrap();
        let a = after.iter().find(|r| r.event.id() == "a").unwrap();
        assert!(a.updated_at > stamp);
        assert_eq!(s.git_page("r", 10, None, None).unwrap().len(), 2);
    }

    #[test]
    fn round_trips_every_field() {
        let s = Store::in_memory().unwrap();
        let mut e = git("a", "r", 10);
        e.kind = GitKind::PrMerged;
        e.pr = Some(7);
        e.state = Some("main".into());
        e.url = Some("https://x".into());
        let p = prompt("p1", "s1", 5);
        s.upsert(&[g(e.clone()), Event::Prompt(p.clone())]).unwrap();
        assert_eq!(s.git_page("r", 10, None, None).unwrap(), [e]);
        assert_eq!(s.prompt_page("r", 10, None, None).unwrap(), [p]);
    }

    #[test]
    fn pages_newest_first_by_repo() {
        let s = Store::in_memory().unwrap();
        let events: Vec<Event> = (1..=5)
            .map(|i| g(git(&format!("c{i}"), "r", i * 10)))
            .collect();
        s.upsert(&events).unwrap();
        s.upsert(&[g(git("other", "x", 100))]).unwrap();

        let first = s.git_page("r", 2, None, None).unwrap();
        assert_eq!(first.iter().map(|e| e.at).collect::<Vec<_>>(), [50, 40]);
        let second = s.git_page("r", 2, Some(40), None).unwrap();
        assert_eq!(second.iter().map(|e| e.at).collect::<Vec<_>>(), [30, 20]);
        let last = s.git_page("r", 2, Some(20), None).unwrap();
        assert_eq!(last.len(), 1);
        assert!(s.git_page("none", 10, None, None).unwrap().is_empty());
    }

    #[test]
    fn paging_by_time_and_id_keeps_events_that_share_a_time() {
        let s = Store::in_memory().unwrap();
        let events: Vec<Event> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| g(git(id, "r", 100)))
            .chain([g(git("old", "r", 50))])
            .collect();
        s.upsert(&events).unwrap();
        s.upsert(&[Event::Prompt(prompt("pa", "s", 100))]).unwrap();
        s.upsert(&[Event::Prompt(prompt("pb", "s", 100))]).unwrap();

        let first = s.git_page("r", 2, None, None).unwrap();
        assert_eq!(
            first.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["e", "d"]
        );
        let last = first.last().unwrap();
        let second = s.git_page("r", 2, Some(last.at), Some(&last.id)).unwrap();
        assert_eq!(
            second.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["c", "b"]
        );
        let third = s.git_page("r", 2, Some(100), Some("b")).unwrap();
        assert_eq!(
            third.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["a", "old"]
        );
        // Only `before`: strictly older, as before.
        let older = s.git_page("r", 10, Some(100), None).unwrap();
        assert_eq!(
            older.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["old"]
        );

        let p = s.prompt_page("r", 1, None, None).unwrap();
        assert_eq!(p[0].id, "pb");
        let p = s.prompt_page("r", 1, Some(100), Some("pb")).unwrap();
        assert_eq!(p[0].id, "pa");
    }

    #[test]
    fn pruned_and_remote_tombstones_keep_no_content() {
        let s = Store::in_memory().unwrap();
        s.upsert(&[g(git("old", "r", 100))]).unwrap();
        s.upsert(&[Event::Prompt(prompt("p-old", "s", 100))])
            .unwrap();
        assert_eq!(s.prune(1000).unwrap(), 2);
        let rows = s.pending(10, true).unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert!(r.deleted);
            assert_eq!(r.event, tombstone(&r.event), "already blank");
            match &r.event {
                Event::Git(e) => {
                    assert!(e.title.is_empty() && e.author.is_none() && e.parents.is_empty());
                    assert!(e.branch.is_none() && e.url.is_none());
                }
                Event::Prompt(e) => {
                    assert!(e.text.is_empty() && e.files.is_empty() && e.commits.is_empty());
                    assert!(e.branch.is_none());
                }
            }
        }

        // A deleted row arriving from another device is stored blank too.
        let remote = SyncRow {
            updated_at: i64::MAX / 2,
            deleted: true,
            device_id: "other".into(),
            event: g(git("x", "r", 5)),
        };
        s.apply_remote(&[remote], true).unwrap();
        let x = s
            .pending(10, true)
            .unwrap()
            .into_iter()
            .find(|r| r.event.id() == "x");
        assert!(x.is_none(), "applied rows are synced");
        let conn = s.conn();
        let title: String = conn
            .query_row("SELECT title FROM git_events WHERE id = 'x'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "");
    }

    #[test]
    fn retention_leaves_tombstones_that_stay() {
        let s = Store::in_memory().unwrap();
        s.upsert(&[g(git("old", "r", 100)), g(git("new", "r", 5000))])
            .unwrap();
        s.upsert(&[Event::Prompt(prompt("p-old", "s", 100))])
            .unwrap();
        // Pretend both were uploaded.
        let rows: Vec<_> = s
            .pending(10, true)
            .unwrap()
            .iter()
            .map(|r| (r.event.id().to_string(), r.updated_at))
            .collect();
        s.mark_synced(&rows).unwrap();
        assert!(s.pending(10, true).unwrap().is_empty());

        assert_eq!(s.prune(1000).unwrap(), 2);
        assert_eq!(s.prune(1000).unwrap(), 0, "already tombstoned");
        let live = s.git_page("r", 10, None, None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, "new");
        assert!(s.prompt_page("r", 10, None, None).unwrap().is_empty());

        // The tombstones wait to be uploaded.
        let pending = s.pending(10, true).unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|r| r.deleted));

        // Collecting the old event again does not bring it back.
        assert_eq!(s.upsert(&[g(git("old", "r", 100))]).unwrap(), 0);
        assert_eq!(s.git_page("r", 10, None, None).unwrap().len(), 1);
    }

    #[test]
    fn pending_skips_prompts_when_asked_and_mark_synced_clears() {
        let s = Store::in_memory().unwrap();
        s.upsert(&[g(git("a", "r", 1)), Event::Prompt(prompt("p", "s", 2))])
            .unwrap();
        assert_eq!(s.pending(10, false).unwrap().len(), 1);
        assert_eq!(s.pending(10, true).unwrap().len(), 2);
        let all = s.pending(10, true).unwrap();
        let done: Vec<_> = all
            .iter()
            .map(|r| (r.event.id().to_string(), r.updated_at))
            .collect();
        s.mark_synced(&done).unwrap();
        assert!(s.pending(10, true).unwrap().is_empty());
        s.mark_all_unsynced().unwrap();
        assert_eq!(s.pending(10, true).unwrap().len(), 2);
    }

    #[test]
    fn remote_rows_apply_by_last_write_wins() {
        let s = Store::in_memory().unwrap();
        s.upsert(&[g(git("a", "r", 1))]).unwrap();
        let mine = s.pending(10, true).unwrap().remove(0);

        let mut theirs = mine.clone();
        theirs.updated_at = mine.updated_at - 1;
        theirs.device_id = "z".into();
        assert!(
            s.apply_remote(&[theirs.clone()], true).unwrap().is_empty(),
            "older loses"
        );

        theirs.updated_at = mine.updated_at + 1;
        if let Event::Git(e) = &mut theirs.event {
            e.title = "from laptop".into();
        }
        assert_eq!(s.apply_remote(&[theirs.clone()], true).unwrap(), ["r"]);
        assert_eq!(
            s.git_page("r", 10, None, None).unwrap()[0].title,
            "from laptop"
        );
        assert!(
            s.pending(10, true).unwrap().is_empty(),
            "applied rows are not re-uploaded"
        );

        // A remote tombstone hides the event.
        theirs.updated_at += 1;
        theirs.deleted = true;
        s.apply_remote(&[theirs], true).unwrap();
        assert!(s.git_page("r", 10, None, None).unwrap().is_empty());
    }

    #[test]
    fn remote_prompts_are_ignored_when_prompt_sync_is_off() {
        let s = Store::in_memory().unwrap();
        let row = SyncRow {
            updated_at: 5,
            deleted: false,
            device_id: "d".into(),
            event: Event::Prompt(prompt("p", "s", 1)),
        };
        assert!(
            s.apply_remote(std::slice::from_ref(&row), false)
                .unwrap()
                .is_empty()
        );
        assert!(s.prompt_page("r", 10, None, None).unwrap().is_empty());
        assert_eq!(s.apply_remote(&[row], true).unwrap(), ["r"]);
        assert_eq!(s.prompt_page("r", 10, None, None).unwrap().len(), 1);
    }

    #[test]
    fn sessions_summarize_a_page() {
        let events = vec![
            prompt("p3", "s1", 30),
            prompt("p2", "s2", 20),
            prompt("p1", "s1", 10),
        ];
        let list = sessions(&events);
        assert_eq!(list.len(), 2);
        assert_eq!(
            (
                list[0].id.as_str(),
                list[0].started,
                list[0].ended,
                list[0].prompts
            ),
            ("s1", 10, 30, 2)
        );
        assert_eq!(list[1].id, "s2");
    }

    #[test]
    fn settings_state_and_tips_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("activity.db");
        let device = {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.settings().unwrap(), Settings::default());
            s.set_settings(Settings {
                retention_days: None,
                sync_prompts: false,
            })
            .unwrap();
            s.set_scan_state("r", &serde_json::json!({"a": 1})).unwrap();
            s.set_tips(
                "r",
                &[BranchTip {
                    name: "main".into(),
                    local: Some("abc".into()),
                    remote: None,
                    is_default: true,
                    is_current: true,
                }],
            )
            .unwrap();
            s.device().to_string()
        };
        let s = Store::open(&path).unwrap();
        assert_eq!(s.device(), device);
        let st = s.settings().unwrap();
        assert_eq!((st.retention_days, st.sync_prompts), (None, false));
        assert_eq!(s.scan_state("r").unwrap()["a"], 1);
        assert!(s.scan_state("none").unwrap().is_null());
        assert_eq!(s.tips("r").unwrap()[0].name, "main");
        assert!(s.tips("none").unwrap().is_empty());
        assert_eq!(Settings::default().cutoff(100 * DAY_MS), Some(10 * DAY_MS));
    }
}
