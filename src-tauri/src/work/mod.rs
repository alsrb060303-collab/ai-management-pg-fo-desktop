//! The work panel: the open project's branch, changed files, local branches
//! and worktrees, and the branch and worktree operations it offers, plus
//! committing, pushing and pulling (with the GitHub sign-in when there is one).

mod ops;
mod read;

use std::path::{Path, PathBuf};

use ops::{BranchCheck, Diff};
use read::Overview;

async fn blocking<T: Send + 'static>(
    project: String,
    f: impl FnOnce(&Path) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&repo_root(&project)?))
        .await
        .map_err(|e| e.to_string())?
}

fn repo_root(project: &str) -> Result<PathBuf, String> {
    read::root(Path::new(project))?
        .ok_or_else(|| crate::tr!("git 저장소가 아니에요", "Not a git repository"))
}

/// `None` when the project is not a git repository.
#[tauri::command]
pub async fn work_overview(
    project: String,
    count_worktrees: bool,
) -> Result<Option<Overview>, String> {
    tauri::async_runtime::spawn_blocking(move || match read::root(Path::new(&project))? {
        Some(root) => read::overview(&root, count_worktrees).map(Some),
        None => Ok(None),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn work_diff(
    project: String,
    path: String,
    orig_path: Option<String>,
    untracked: bool,
) -> Result<Diff, String> {
    blocking(project, move |root| {
        ops::diff(root, &path, orig_path.as_deref(), untracked)
    })
    .await
}

#[tauri::command]
pub async fn work_switch(project: String, branch: String) -> Result<(), String> {
    blocking(project, move |root| ops::switch(root, &branch)).await
}

#[tauri::command]
pub async fn work_create_branch(
    project: String,
    name: String,
    base: Option<String>,
    switch_to: bool,
) -> Result<(), String> {
    blocking(project, move |root| {
        ops::create_branch(root, name.trim(), base.as_deref(), switch_to)
    })
    .await
}

#[tauri::command]
pub async fn work_check_branch(project: String, name: String) -> Result<BranchCheck, String> {
    blocking(project, move |root| ops::check_branch(root, &name)).await
}

#[tauri::command]
pub async fn work_delete_branch(project: String, name: String, force: bool) -> Result<(), String> {
    blocking(project, move |root| ops::delete_branch(root, &name, force)).await
}

#[tauri::command]
pub async fn work_add_worktree(
    project: String,
    name: String,
    base: Option<String>,
) -> Result<String, String> {
    blocking(project, move |root| {
        ops::add_worktree(root, name.trim(), base.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn work_remove_worktree(
    project: String,
    path: String,
    force: bool,
) -> Result<(), String> {
    blocking(project, move |root| {
        ops::remove_worktree(root, &path, force)
    })
    .await
}

#[tauri::command]
pub async fn work_prune_worktrees(project: String) -> Result<(), String> {
    blocking(project, ops::prune_worktrees).await
}

#[tauri::command]
pub async fn work_commit(project: String, message: String, all: bool) -> Result<String, String> {
    blocking(project, move |root| ops::commit(root, &message, all)).await
}

#[tauri::command]
pub async fn work_push(app: tauri::AppHandle, project: String) -> Result<(), String> {
    let auth = crate::github::git_auth(&app);
    blocking(project, move |root| ops::push(root, &auth)).await
}

#[tauri::command]
pub async fn work_pull(app: tauri::AppHandle, project: String) -> Result<(), String> {
    let auth = crate::github::git_auth(&app);
    blocking(project, move |root| ops::pull(root, &auth)).await
}

/// The installed git's version (`git version 2.47.0.windows.1`), or `None`
/// when git cannot be run, so the setup guide can start with installing it.
#[tauri::command]
pub async fn work_git_version(project: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = Path::new(&project);
        let dir = if dir.is_dir() { dir } else { Path::new(".") };
        crate::git::run(dir, &["--version"]).ok()
    })
    .await
    .ok()
    .flatten()
}
