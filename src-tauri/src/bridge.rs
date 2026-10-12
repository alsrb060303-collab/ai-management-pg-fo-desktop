//! The bridge to the Claude Code mod (`claude-mod/branch-notes`): the app
//! writes the memos it holds to `<app data>/bridge/state.json`, and the mod
//! leaves requests (`/todo`, `/memo`, ticking a box in its pane) as files in
//! `<app data>/bridge/inbox/`, which the app applies to its store. The app is
//! the only writer of memo data, so sync (Phase 8b) sees every change.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::memo::{Memo, Store};

/// How often the inbox is read.
const POLL: Duration = Duration::from_millis(1500);
/// `heartbeat` is rewritten this often, so the mod can tell the app is running.
const HEARTBEAT: Duration = Duration::from_secs(30);
/// Requests older than this are dropped unread (left over from long ago).
const MAX_REQUEST_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const MAX_REQUEST_BYTES: u64 = 64 * 1024;

static DIRTY: AtomicBool = AtomicBool::new(true);

/// Memo data changed: export it again.
pub fn changed() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// PR and CI of a branch, as the work panel last saw them.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSummary {
    pub pr_number: Option<u64>,
    pub pr_title: Option<String>,
    pub pr_state: Option<String>,
    pub pr_url: Option<String>,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
}

/// `repo key` → `branch` → summary.
static GITHUB: Mutex<BTreeMap<String, BTreeMap<String, GitHubSummary>>> =
    Mutex::new(BTreeMap::new());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportTodo {
    id: String,
    text: String,
    done: bool,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExportBranch {
    note: String,
    todos: Vec<ExportTodo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github: Option<GitHubSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Export {
    version: u32,
    /// Epoch ms; the mod treats an old one as "the app isn't running".
    heartbeat: i64,
    /// "ko" or "en", for the mod's own words.
    locale: &'static str,
    /// Repository key of a local repository without a remote, by its path
    /// key (`path:<main worktree>`), as the app last saw it.
    paths: BTreeMap<String, String>,
    repos: BTreeMap<String, BTreeMap<String, ExportBranch>>,
}

/// A request from the mod.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    AddTodo {
        repo_key: String,
        branch: String,
        text: String,
    },
    SetDone {
        id: String,
        done: bool,
    },
    DeleteTodo {
        id: String,
    },
    AppendNote {
        repo_key: String,
        branch: String,
        text: String,
    },
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn export(store: &Store) -> Result<Export, String> {
    let github = GITHUB.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut repos: BTreeMap<String, BTreeMap<String, ExportBranch>> = BTreeMap::new();
    for (repo, branch) in store.live_branches()? {
        let memo = store.branch(&repo, &branch)?;
        repos.entry(repo).or_default().insert(
            branch,
            ExportBranch {
                note: memo.note,
                todos: memo
                    .todos
                    .into_iter()
                    .map(|t| ExportTodo {
                        id: t.id,
                        text: t.text,
                        done: t.done,
                    })
                    .collect(),
                github: None,
            },
        );
    }
    for (repo, branches) in github {
        for (branch, summary) in branches {
            repos
                .entry(repo.clone())
                .or_default()
                .entry(branch)
                .or_default()
                .github = Some(summary);
        }
    }
    Ok(Export {
        version: 1,
        heartbeat: now_ms(),
        locale: if crate::i18n::english() { "en" } else { "ko" },
        paths: store.repo_paths()?.into_iter().collect(),
        repos,
    })
}

/// Writes next to the target, then renames: the mod never reads half a file.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

/// Applies one request; returns the branch it changed.
pub fn apply(store: &Store, request: Request) -> Result<(String, String), String> {
    match request {
        Request::AddTodo {
            repo_key,
            branch,
            text,
        } => {
            store.add_todo(&repo_key, &branch, &text)?;
            Ok((repo_key, branch))
        }
        Request::SetDone { id, done } => store.update_todo(&id, None, Some(done)),
        Request::DeleteTodo { id } => store.delete_todo(&id),
        Request::AppendNote {
            repo_key,
            branch,
            text,
        } => {
            let note = store.branch(&repo_key, &branch)?.note;
            let text = text.trim();
            let joined = if note.trim().is_empty() {
                text.to_string()
            } else {
                format!("{}\n{text}", note.trim_end())
            };
            store.set_note(&repo_key, &branch, &joined)?;
            Ok((repo_key, branch))
        }
    }
}

/// Reads and removes the inbox's requests, oldest first.
fn take_requests(inbox: &Path) -> Vec<Request> {
    let Ok(entries) = fs::read_dir(inbox) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let meta = fs::metadata(&path).ok();
        let fresh = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age < MAX_REQUEST_AGE);
        let small = meta.is_some_and(|m| m.len() <= MAX_REQUEST_BYTES);
        if fresh
            && small
            && let Ok(text) = fs::read_to_string(&path)
            && let Ok(request) = serde_json::from_str::<Request>(&text)
        {
            out.push(request);
        }
        let _ = fs::remove_file(&path);
    }
    out
}

fn dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("bridge"))
}

pub fn manage(app: &AppHandle) {
    let Some(dir) = dir(app) else {
        return;
    };
    let inbox = dir.join("inbox");
    if fs::create_dir_all(&inbox).is_err() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last_write = Instant::now() - HEARTBEAT;
        loop {
            if let Ok(store) = app.state::<Memo>().store() {
                for request in take_requests(&inbox) {
                    if let Ok((repo, branch)) = apply(store, request) {
                        crate::memo::notify(&app, repo, branch);
                    }
                }
                if DIRTY.swap(false, Ordering::Relaxed) || last_write.elapsed() >= HEARTBEAT {
                    if let Ok(data) = export(store)
                        && let Ok(text) = serde_json::to_string(&data)
                    {
                        let _ = write_atomic(&dir.join("state.json"), &text);
                    }
                    last_write = Instant::now();
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

/// The work panel's latest PR and CI for the project's branch.
#[tauri::command]
pub async fn bridge_github(project: String, summary: Option<GitHubSummary>) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let Ok(info) = crate::memo::repo::inspect(Path::new(&project)) else {
            return;
        };
        let Some(branch) = info.branch else {
            return;
        };
        let mut github = GITHUB.lock().unwrap_or_else(|e| e.into_inner());
        let branches = github.entry(info.key).or_default();
        let changed = match summary {
            Some(s) => branches.insert(branch, s.clone()).as_ref() != Some(&s),
            None => branches.remove(&branch).is_some(),
        };
        if changed {
            self::changed();
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_from_the_mods_json() {
        let r: Request = serde_json::from_str(
            r#"{"op":"addTodo","repoKey":"github.com/o/r","branch":"main","text":"x"}"#,
        )
        .unwrap();
        assert_eq!(
            r,
            Request::AddTodo {
                repo_key: "github.com/o/r".into(),
                branch: "main".into(),
                text: "x".into()
            }
        );
        let r: Request = serde_json::from_str(r#"{"op":"setDone","id":"a","done":true}"#).unwrap();
        assert_eq!(
            r,
            Request::SetDone {
                id: "a".into(),
                done: true
            }
        );
        assert!(serde_json::from_str::<Request>(r#"{"op":"dropTable"}"#).is_err());
    }

    #[test]
    fn requests_change_the_store_and_show_in_the_export() {
        let store = Store::in_memory().unwrap();
        apply(
            &store,
            Request::AddTodo {
                repo_key: "r".into(),
                branch: "main".into(),
                text: "write tests".into(),
            },
        )
        .unwrap();
        for line in ["first", "second"] {
            apply(
                &store,
                Request::AppendNote {
                    repo_key: "r".into(),
                    branch: "main".into(),
                    text: line.into(),
                },
            )
            .unwrap();
        }
        let id = store.branch("r", "main").unwrap().todos[0].id.clone();
        apply(&store, Request::SetDone { id, done: true }).unwrap();

        let data = export(&store).unwrap();
        let main = &data.repos["r"]["main"];
        assert_eq!(main.note, "first\nsecond");
        assert_eq!(main.todos.len(), 1);
        assert!(main.todos[0].done);
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains(r#""heartbeat":"#) && json.contains(r#""todos":"#));
    }

    #[test]
    fn the_inbox_is_read_oldest_first_and_emptied() {
        let dir = std::env::temp_dir().join(format!("aim-inbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("2.json"), r#"{"op":"deleteTodo","id":"b"}"#).unwrap();
        fs::write(dir.join("1.json"), r#"{"op":"deleteTodo","id":"a"}"#).unwrap();
        fs::write(dir.join("3.json"), "not json").unwrap();
        let got = take_requests(&dir);
        assert_eq!(
            got,
            [
                Request::DeleteTodo { id: "a".into() },
                Request::DeleteTodo { id: "b".into() }
            ]
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }
}
