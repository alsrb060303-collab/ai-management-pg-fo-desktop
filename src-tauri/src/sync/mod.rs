//! Memo sync across devices through Firebase (Phase 8b). The local SQLite
//! store stays the source of truth; a background thread uploads rows changed
//! here and applies rows other devices uploaded, newest `updatedAt` winning.
//! Only accounts the owner put on the allow list (`allowed/{uid}`) can sync
//! until paid plans exist (Phase 8c).

mod firebase;

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::memo::{MEMO_EVENT, Memo, Store};
use firebase::{Client, Config, Cursor, Endpoints, Error, Session};

/// The Firebase web app's public settings. Empty until the project exists;
/// `AIM_FIREBASE_API_KEY` / `AIM_FIREBASE_PROJECT` override them.
const API_KEY: &str = "";
const PROJECT_ID: &str = "";

pub const STATUS_EVENT: &str = "sync-status";
/// How often to look for changes from other devices while the app runs.
const INTERVAL: Duration = Duration::from_secs(60);
/// Edits are gathered for this long before uploading.
const DEBOUNCE: Duration = Duration::from_secs(3);
const BATCH: usize = 300;

fn config() -> Option<Config> {
    let pick = |var: &str, fallback: &str| {
        std::env::var(var)
            .ok()
            .or_else(|| Some(fallback.to_string()))
            .filter(|v| !v.trim().is_empty())
    };
    Some(Config {
        api_key: pick("AIM_FIREBASE_API_KEY", API_KEY)?,
        project_id: pick("AIM_FIREBASE_PROJECT", PROJECT_ID)?,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// This build has no Firebase project.
    #[default]
    NotConfigured,
    /// Turned off in settings.
    Off,
    /// Sync signs in with the GitHub account; not signed in there.
    NeedsGitHub,
    /// Signed in, but not on the allow list yet (`uid` to add).
    NotAllowed,
    Syncing,
    Idle,
    Error,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    pub uid: Option<String>,
    /// Epoch ms of the last completed sync.
    pub last_sync: Option<i64>,
    pub message: Option<String>,
}

pub struct Sync {
    status: Mutex<Status>,
    session: Mutex<Option<Session>>,
    /// When the worker should run next.
    next: Mutex<Instant>,
    wake: Condvar,
    agent: ureq::Agent,
}

const ENABLED_KEY: &str = "sync_enabled";
const UID_KEY: &str = "sync_uid";
const CURSOR_KEY: &str = "sync_cursor";

impl Sync {
    pub fn manage(app: &AppHandle) {
        app.manage(Sync {
            status: Mutex::new(Status::default()),
            session: Mutex::new(None),
            next: Mutex::new(Instant::now() + Duration::from_secs(5)),
            wake: Condvar::new(),
            agent: firebase::agent(),
        });
        let handle = app.clone();
        std::thread::spawn(move || worker(handle));
        // Signing in or out of GitHub changes what sync can do.
        let handle = app.clone();
        app.listen(crate::github::AUTH_EVENT, move |_| {
            *handle
                .state::<Sync>()
                .session
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            kick(&handle, Duration::ZERO);
        });
    }

    fn set_status(&self, app: &AppHandle, status: Status) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status.clone();
        let _ = app.emit(STATUS_EVENT, status);
    }
}

/// Asks for a sync within `delay` (earlier requests win).
pub fn kick(app: &AppHandle, delay: Duration) {
    let Some(sync) = app.try_state::<Sync>() else {
        return;
    };
    let mut next = sync.next.lock().unwrap_or_else(|e| e.into_inner());
    let at = Instant::now() + delay;
    if at < *next {
        *next = at;
        sync.wake.notify_one();
    }
}

/// After a local memo edit: upload soon.
pub fn changed(app: &AppHandle) {
    kick(app, DEBOUNCE);
}

fn worker(app: AppHandle) {
    loop {
        {
            let sync = app.state::<Sync>();
            let mut next = sync.next.lock().unwrap_or_else(|e| e.into_inner());
            while Instant::now() < *next {
                let wait = *next - Instant::now();
                next = sync
                    .wake
                    .wait_timeout(next, wait)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            *next = Instant::now() + INTERVAL;
        }
        run(&app);
    }
}

fn run(app: &AppHandle) {
    let sync = app.state::<Sync>();
    let last_sync = sync
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .last_sync;
    let status = match cycle(app, &sync, last_sync) {
        Ok(status) => status,
        Err(e) => {
            if matches!(e, Error::BadToken) {
                *sync.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            let uid = sync
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .uid
                .clone();
            Status {
                state: State::Error,
                uid,
                last_sync,
                message: Some(e.into()),
            }
        }
    };
    sync.set_status(app, status);
}

fn cycle(app: &AppHandle, sync: &Sync, last_sync: Option<i64>) -> firebase::Result<Status> {
    let Some(config) = config() else {
        return Ok(Status::default());
    };
    let memo = app.state::<Memo>();
    let store = memo.store().map_err(Error::Other)?;
    if store.meta(ENABLED_KEY).map_err(Error::Other)?.as_deref() == Some("0") {
        return Ok(Status {
            state: State::Off,
            ..Status::default()
        });
    }
    let Some(token) = crate::github::current_token(app) else {
        return Ok(Status {
            state: State::NeedsGitHub,
            ..Status::default()
        });
    };
    let ends = Endpoints::default();
    let client = Client {
        agent: &sync.agent,
        ends: &ends,
        config: &config,
    };
    let session = {
        let cached = sync
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match cached.filter(Session::fresh) {
            Some(s) => s,
            None => {
                let s = client.sign_in(&token)?;
                *sync.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(s.clone());
                s
            }
        }
    };
    let uid = Some(session.uid.clone());
    if !client.allowed(&session)? {
        return Ok(Status {
            state: State::NotAllowed,
            uid,
            ..Status::default()
        });
    }
    sync.set_status(
        app,
        Status {
            state: State::Syncing,
            uid: uid.clone(),
            last_sync,
            message: None,
        },
    );

    for (repo_key, branch) in exchange(store, &client, &session)? {
        let _ = app.emit(
            MEMO_EVENT,
            serde_json::json!({ "repoKey": repo_key, "branch": branch }),
        );
    }

    Ok(Status {
        state: State::Idle,
        uid,
        last_sync: Some(now_ms()),
        message: None,
    })
}

/// One round with the server: upload what changed here, then apply what
/// other devices uploaded. Returns the branches whose memos changed.
fn exchange(
    store: &Store,
    client: &Client,
    session: &Session,
) -> firebase::Result<Vec<(String, String)>> {
    // Another account than last time: its copy has none of this yet.
    if store.meta(UID_KEY).map_err(Error::Other)?.as_deref() != Some(session.uid.as_str()) {
        store.mark_all_unsynced().map_err(Error::Other)?;
        store.set_meta(CURSOR_KEY, None).map_err(Error::Other)?;
        store
            .set_meta(UID_KEY, Some(&session.uid))
            .map_err(Error::Other)?;
    }

    // Upload what changed here.
    loop {
        let rows = store.pending(BATCH).map_err(Error::Other)?;
        if rows.is_empty() {
            break;
        }
        let accepted = client.push(session, &rows)?;
        let done: Vec<(String, i64)> = rows
            .iter()
            .filter(|r| accepted.contains(&r.id))
            .map(|r| (r.id.clone(), r.updated_at))
            .collect();
        store.mark_synced(&done).map_err(Error::Other)?;
        // Refused rows lost to newer ones on the server; the pull brings those.
        if done.len() < rows.len() || rows.len() < BATCH {
            break;
        }
    }

    // Apply what other devices uploaded.
    let mut changed = Vec::new();
    let mut cursor = store
        .meta(CURSOR_KEY)
        .map_err(Error::Other)?
        .and_then(|c| Cursor::decode(&c));
    loop {
        let (rows, last) = client.pull(session, cursor.as_ref(), BATCH)?;
        for key in store.apply_remote(&rows).map_err(Error::Other)? {
            if !changed.contains(&key) {
                changed.push(key);
            }
        }
        if let Some(last) = last {
            store
                .set_meta(CURSOR_KEY, Some(&last.encode()))
                .map_err(Error::Other)?;
            cursor = Some(last);
        }
        if rows.len() < BATCH {
            break;
        }
    }
    Ok(changed)
}

#[tauri::command]
pub fn sync_status(app: AppHandle) -> Status {
    app.state::<Sync>()
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
pub fn sync_now(app: AppHandle) {
    kick(&app, Duration::ZERO);
}

#[tauri::command]
pub async fn sync_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle
            .state::<Memo>()
            .store()?
            .set_meta(ENABLED_KEY, Some(if enabled { "1" } else { "0" }))
    })
    .await
    .map_err(|e| e.to_string())??;
    kick(&app, Duration::ZERO);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::test_serve as serve;

    #[test]
    fn exchange_uploads_local_rows_and_applies_remote_ones() {
        let store = Store::in_memory().unwrap();
        store.add_todo("github.com/o/r", "main", "mine").unwrap();
        let mine = store.pending(10).unwrap()[0].id.clone();

        let remote = r#"[{"document":{"name":"projects/p/databases/(default)/documents/users/u1/items/theirs","fields":{"repoKey":{"stringValue":"github.com/o/r"},"branch":{"stringValue":"dev"},"kind":{"stringValue":"todo"},"text":{"stringValue":"from laptop"},"done":{"booleanValue":false},"position":{"doubleValue":1},"createdAt":{"integerValue":"5"},"updatedAt":{"integerValue":"9"},"deleted":{"booleanValue":false},"deviceId":{"stringValue":"laptop"},"syncedAt":{"timestampValue":"2026-10-11T00:00:00Z"}}}}]"#;
        let (base, seen) = serve(vec![
            (200, r#"{"localId":"u1","idToken":"t","expiresIn":"3600"}"#),
            (200, r#"{"writeResults":[{}],"status":[{}]}"#),
            (200, remote),
        ]);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let ends = Endpoints {
            auth: base.clone(),
            firestore: base,
        };
        let config = Config {
            api_key: "k".into(),
            project_id: "p".into(),
        };
        let client = Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        };
        let session = client.sign_in("gho").unwrap();
        let changed = exchange(&store, &client, &session).unwrap();

        assert_eq!(changed, [("github.com/o/r".to_string(), "dev".to_string())]);
        assert!(store.pending(10).unwrap().is_empty(), "uploaded and marked");
        assert_eq!(
            store.branch("github.com/o/r", "dev").unwrap().todos[0].text,
            "from laptop"
        );
        assert_eq!(store.meta(UID_KEY).unwrap().as_deref(), Some("u1"));
        assert!(store.meta(CURSOR_KEY).unwrap().is_some());
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains(&mine));
    }
}
