//! The work panel: the open project's branch, changed files, local branches
//! and worktrees, and the branch and worktree operations it offers, plus
//! committing, pushing and pulling (with the GitHub sign-in when there is one).

mod conflict;
mod ops;
mod read;

use std::path::{Path, PathBuf};

use crate::tools::ToolId;
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

/// The Codex app's worktree root, to label its worktrees; `None` when the
/// home folder is unknown.
fn codex_worktree_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let home = app.path().home_dir().ok()?;
    let env = crate::tools::codex::Env::detect(home);
    Some(read::codex_worktree_root_in(&env.codex_home))
}

/// `None` when the project is not a git repository.
#[tauri::command]
pub async fn work_overview(
    app: tauri::AppHandle,
    project: String,
    count_worktrees: bool,
) -> Result<Option<Overview>, String> {
    let codex_root = codex_worktree_root(&app);
    tauri::async_runtime::spawn_blocking(move || match read::root(Path::new(&project))? {
        Some(root) => read::overview(&root, count_worktrees, codex_root.as_deref()).map(Some),
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
    tool: ToolId,
) -> Result<String, String> {
    blocking(project, move |root| {
        ops::add_worktree(root, name.trim(), base.as_deref(), tool)
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

#[tauri::command]
pub async fn work_stage(project: String, paths: Vec<String>, stage: bool) -> Result<(), String> {
    blocking(project, move |root| ops::stage(root, &paths, stage)).await
}

#[tauri::command]
pub async fn work_commit_diff(project: String, hash: String) -> Result<Diff, String> {
    blocking(project, move |root| ops::commit_diff(root, &hash)).await
}

/// How `claude` is asked for a commit message. The diff follows on stdin.
const DRAFT_PROMPT: &str = "Write a git commit message for the diff on stdin. \
Match the language and style of the recent commit subjects if there are any, otherwise write in English. \
First line: an imperative summary under 72 characters. If the change needs it, add a blank line and a short body. \
Reply with the message only: no quotes, no code fences, no explanation.";

/// A commit message drafted by the user's own `claude` (`claude -p`, no
/// tools, not saved as a session) from what would be committed.
#[tauri::command]
pub async fn work_draft_message(project: String, all: bool) -> Result<String, String> {
    blocking(project, move |root| {
        let context = ops::message_context(root, all)?;
        let (ok, out, err) = crate::claude_mod::claude_with_input(
            root,
            &[
                "-p",
                DRAFT_PROMPT,
                "--model",
                "haiku",
                "--tools",
                "",
                "--no-session-persistence",
                "--output-format",
                "text",
            ],
            Some(context),
            std::time::Duration::from_secs(120),
        )?;
        let text = out.trim();
        if !ok || text.is_empty() {
            let why = if err.trim().is_empty() {
                text
            } else {
                err.trim()
            };
            return Err(crate::tr!(
                "claude가 메시지를 만들지 못했어요: {why}",
                "claude couldn't draft a message: {why}"
            ));
        }
        Ok(strip_fences(text))
    })
    .await
}

/// Drops a ``` fence the model may wrap the message in anyway.
fn strip_fences(text: &str) -> String {
    let t = text.trim();
    match t.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.split_once('\n').map_or("", |(_, r)| r);
            rest.trim_end().trim_end_matches("```").trim().to_string()
        }
        None => t.to_string(),
    }
}

#[tauri::command]
pub async fn work_conflict(
    project: String,
    path: String,
) -> Result<conflict::ConflictFile, String> {
    blocking(project, move |root| conflict::read(root, &path)).await
}

/// Saves the resolved file (`text`), or deletes it (`None`), and marks it resolved.
#[tauri::command]
pub async fn work_resolve(
    project: String,
    path: String,
    text: Option<String>,
) -> Result<(), String> {
    blocking(project, move |root| {
        conflict::resolve(root, &path, text.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn work_continue(project: String) -> Result<(), String> {
    blocking(project, conflict::continue_operation).await
}

#[tauri::command]
pub async fn work_abort(project: String) -> Result<(), String> {
    blocking(project, conflict::abort_operation).await
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

#[cfg(test)]
mod tests {
    #[test]
    fn fenced_drafts_are_unwrapped() {
        assert_eq!(
            super::strip_fences("```\nAdd x\n\nBody\n```"),
            "Add x\n\nBody"
        );
        assert_eq!(super::strip_fences("```text\nAdd x\n```\n"), "Add x");
        assert_eq!(super::strip_fences("  Add x \n"), "Add x");
    }
}
