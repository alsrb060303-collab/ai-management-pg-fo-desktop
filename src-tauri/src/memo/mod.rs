//! Branch notes and TODOs, stored locally (`<app data>/memo.db`).
//!
//! Notes and TODOs belong to a repository (normalized remote, see `repo`) and
//! a branch, so every clone and worktree of a repository shows the same ones.
//! Both windows read the same store; changes are announced with
//! `MEMO_EVENT` so the other window refreshes.

mod repo;
mod store;

use std::path::Path;
use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use store::Store;
pub use store::{BranchMemo, Todo};

pub const MEMO_EVENT: &str = "memo-changed";

/// The store opens lazily, so a broken database never blocks app start.
pub struct Memo {
    path: std::path::PathBuf,
    store: OnceLock<Store>,
}

impl Memo {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let path = app.path().app_data_dir()?.join("memo.db");
        app.manage(Memo {
            path,
            store: OnceLock::new(),
        });
        Ok(())
    }

    fn store(&self) -> store::Result<&Store> {
        if let Some(store) = self.store.get() {
            return Ok(store);
        }
        let store = Store::open(&self.path)?;
        Ok(self.store.get_or_init(|| store))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    /// Still a local branch (memos outlive deleted branches).
    pub exists: bool,
    pub current: bool,
    pub open: u32,
    pub total: u32,
    pub has_note: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoContext {
    pub repo_key: String,
    pub repo_name: String,
    pub is_git: bool,
    /// Branch the memo follows by default: the checked-out one (also while
    /// rebasing it), or "" outside git and on a detached HEAD.
    pub current: String,
    pub detached_at: Option<String>,
    pub branches: Vec<BranchInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Changed {
    repo_key: String,
    branch: String,
}

fn context(store: &Store, project: &Path) -> store::Result<MemoContext> {
    let info = repo::inspect(project)?;
    if let Some(path_key) = &info.path_key {
        store.follow_repo(path_key, &info.key)?;
    }
    let current = info.branch.clone().unwrap_or_default();
    let counts = store.counts(&info.key)?;

    let mut names: Vec<String> = info.branches.clone();
    for c in &counts {
        if !names.contains(&c.branch) {
            names.push(c.branch.clone());
        }
    }
    if !names.contains(&current) {
        names.push(current.clone());
    }
    let mut branches: Vec<BranchInfo> = names
        .into_iter()
        .map(|name| {
            let c = counts.iter().find(|c| c.branch == name);
            BranchInfo {
                exists: info.branches.contains(&name) || (!info.is_git && name.is_empty()),
                current: name == current,
                open: c.map_or(0, |c| c.open),
                total: c.map_or(0, |c| c.total),
                has_note: c.is_some_and(|c| c.has_note),
                name,
            }
        })
        .collect();
    // Current first, then branches with memos, then the rest by name.
    branches.sort_by(|a, b| {
        let rank = |x: &BranchInfo| (!x.current, x.total == 0 && !x.has_note, !x.exists);
        rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name))
    });

    Ok(MemoContext {
        repo_key: info.key,
        repo_name: info.name,
        is_git: info.is_git,
        current,
        detached_at: info.detached_at,
        branches,
    })
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Store) -> store::Result<T> + Send + 'static,
) -> store::Result<T> {
    tauri::async_runtime::spawn_blocking(move || f(app.state::<Memo>().store()?))
        .await
        .map_err(|e| e.to_string())?
}

fn notify(app: &AppHandle, repo_key: String, branch: String) {
    let _ = app.emit(MEMO_EVENT, Changed { repo_key, branch });
}

#[tauri::command]
pub async fn memo_context(app: AppHandle, project: String) -> Result<MemoContext, String> {
    blocking(app, move |store| context(store, Path::new(&project))).await
}

#[tauri::command]
pub async fn memo_get(
    app: AppHandle,
    repo_key: String,
    branch: String,
) -> Result<BranchMemo, String> {
    blocking(app, move |store| store.branch(&repo_key, &branch)).await
}

#[tauri::command]
pub async fn memo_set_note(
    app: AppHandle,
    repo_key: String,
    branch: String,
    text: String,
) -> Result<(), String> {
    let (k, b) = (repo_key.clone(), branch.clone());
    blocking(app.clone(), move |store| store.set_note(&k, &b, &text)).await?;
    notify(&app, repo_key, branch);
    Ok(())
}

#[tauri::command]
pub async fn memo_add_todo(
    app: AppHandle,
    repo_key: String,
    branch: String,
    text: String,
) -> Result<Todo, String> {
    let (k, b) = (repo_key.clone(), branch.clone());
    let todo = blocking(app.clone(), move |store| store.add_todo(&k, &b, &text)).await?;
    notify(&app, repo_key, branch);
    Ok(todo)
}

#[tauri::command]
pub async fn memo_update_todo(
    app: AppHandle,
    id: String,
    text: Option<String>,
    done: Option<bool>,
) -> Result<(), String> {
    let (k, b) = blocking(app.clone(), move |store| {
        store.update_todo(&id, text.as_deref(), done)
    })
    .await?;
    notify(&app, k, b);
    Ok(())
}

#[tauri::command]
pub async fn memo_delete_todo(app: AppHandle, id: String) -> Result<(), String> {
    let (k, b) = blocking(app.clone(), move |store| store.delete_todo(&id)).await?;
    notify(&app, k, b);
    Ok(())
}

#[tauri::command]
pub async fn memo_move_todo(app: AppHandle, id: String, index: usize) -> Result<(), String> {
    let (k, b) = blocking(app.clone(), move |store| store.move_todo(&id, index)).await?;
    notify(&app, k, b);
    Ok(())
}

#[tauri::command]
pub async fn memo_clear_done(
    app: AppHandle,
    repo_key: String,
    branch: String,
) -> Result<(), String> {
    let (k, b) = (repo_key.clone(), branch.clone());
    blocking(app.clone(), move |store| store.clear_done(&k, &b)).await?;
    notify(&app, repo_key, branch);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_merges_git_branches_with_memo_branches() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(repo)
                    .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            )
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&["branch", "dev"]);

        let store = Store::in_memory().unwrap();
        let key = repo::inspect(repo).unwrap().key;
        store.add_todo(&key, "gone", "old").unwrap();
        store.add_todo(&key, "dev", "x").unwrap();

        let ctx = context(&store, repo).unwrap();
        assert_eq!(ctx.current, "main");
        let names: Vec<_> = ctx
            .branches
            .iter()
            .map(|b| (b.name.as_str(), b.exists, b.open))
            .collect();
        assert_eq!(
            names,
            [("main", true, 0), ("dev", true, 1), ("gone", false, 1)]
        );
    }
}
