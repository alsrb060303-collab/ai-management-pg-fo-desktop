//! Work trees: two separate logs per repository, drawn as time-ordered trees.
//!
//! - GitHub log (`GitEvent`): commits on GitHub's branches, merges, pull
//!   requests and CI results, read from the GitHub API (`git_log`). What is
//!   only here (commits not pushed, uncommitted changes) is read from the
//!   local `git` on every look (`LocalLog`) and never stored.
//! - Prompt log (`PromptEvent`): the user's prompts to Claude Code and Codex,
//!   read from their own session files (`sessions`), with the files each
//!   prompt changed and the commits made while it ran.
//!
//! Both are kept in `<app data>/activity.db`, keyed by the repository key the
//! memos use (`memo::repo`), so every clone and worktree shares one log. They
//! sync through the account like memos (`users/{uid}/activity/{id}`), with
//! prompt text shortened and secrets masked before it is stored (`redact`).
//! Nothing here writes to a project or to a tool's own files.

pub mod git_log;
pub mod redact;
pub mod sessions;
pub mod store;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::tools::ToolId;

pub use store::{Event, Settings, Store};

/// Emitted (payload `{ repo }`) when a repository's logs changed here or by sync.
pub const ACTIVITY_EVENT: &str = "activity-changed";
/// Local commits not on GitHub read per look.
const LOCAL_LIMIT: usize = 200;

/// The store opens lazily, so a broken database never blocks app start.
pub struct Activity {
    path: PathBuf,
    store: OnceLock<Store>,
}

impl Activity {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let path = app.path().app_data_dir()?.join("activity.db");
        app.manage(Activity {
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

/// What happened in the GitHub log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitKind {
    /// A commit on a branch (`parents` link it into the tree).
    Commit,
    /// Commits reached the remote branch (`sha` = the new remote tip).
    Push,
    /// A merge commit, or a branch merged into another one.
    Merge,
    /// A branch was created (first seen) or deleted (`deleted` detail).
    Branch,
    PrOpened,
    PrMerged,
    PrClosed,
    /// A CI run finished on `sha` (`state`: success / failure / cancelled).
    Ci,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitEvent {
    /// Stable across devices: derived from the repo key, kind and ref
    /// (`git:<repo>:<kind>:<sha or pr number>[:<extra>]`), so re-collecting is idempotent.
    pub id: String,
    pub repo: String,
    pub kind: GitKind,
    /// Milliseconds since the epoch (commit time, PR event time, …).
    pub at: i64,
    pub branch: Option<String>,
    /// Commit sha (full) for commits, pushes, merges and CI.
    pub sha: Option<String>,
    /// Parent shas of a commit, for drawing the tree.
    #[serde(default)]
    pub parents: Vec<String>,
    /// PR number for PR events.
    pub pr: Option<u64>,
    /// Commit subject, PR title, CI workflow name.
    pub title: String,
    /// success / failure / cancelled / pending for CI; base branch for merges.
    pub state: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptEvent {
    /// `prompt:<tool>:<session id>:<prompt id or line>`.
    pub id: String,
    pub repo: String,
    pub tool: ToolId,
    pub session: String,
    /// When the prompt was sent (ms since the epoch).
    pub at: i64,
    /// Branch checked out when the prompt was sent.
    pub branch: Option<String>,
    /// Shortened (≤ 500 chars) and secret-masked prompt text.
    pub text: String,
    /// Project-relative paths the tool changed while working on this prompt.
    #[serde(default)]
    pub files: Vec<String>,
    /// Shas of commits made while working on this prompt (links to the GitHub log).
    #[serde(default)]
    pub commits: Vec<String>,
    /// The tool pushed while working on this prompt.
    #[serde(default)]
    pub pushed: bool,
}

/// A page of one log for one repository, newest first.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLog {
    pub repo: String,
    pub events: Vec<GitEvent>,
    /// Branch names with their current local / remote tips, for lanes.
    pub branches: Vec<BranchTip>,
}

/// What the local repository has that GitHub does not, drawn as its own line
/// beside the GitHub log.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLog {
    pub repo: String,
    /// The checked out branch (`None` when detached).
    pub branch: Option<String>,
    /// The commit checked out.
    pub head: Option<String>,
    /// Commits no remote-tracking branch has, newest first.
    pub commits: Vec<GitEvent>,
    /// Files with uncommitted changes (untracked included).
    pub changes: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptLog {
    pub repo: String,
    pub events: Vec<PromptEvent>,
    /// Sessions in the page, for lanes (tool, id, first/last time, branch).
    pub sessions: Vec<SessionInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchTip {
    pub name: String,
    pub local: Option<String>,
    pub remote: Option<String>,
    pub is_default: bool,
    pub is_current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub tool: ToolId,
    pub id: String,
    pub started: i64,
    pub ended: i64,
    pub branch: Option<String>,
    pub prompts: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    /// GitHub-log events that were new or changed.
    pub git: usize,
    /// Prompt events that were new or changed.
    pub prompts: usize,
    /// Something was left out but the refresh went on (for example the CI
    /// results could not be read from GitHub).
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Changed {
    repo: String,
}

pub(crate) fn notify(app: &AppHandle, repo: String) {
    crate::sync::changed(app);
    let _ = app.emit(ACTIVITY_EVENT, Changed { repo });
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Store) -> store::Result<T> + Send + 'static,
) -> store::Result<T> {
    tauri::async_runtime::spawn_blocking(move || f(app.state::<Activity>().store()?))
        .await
        .map_err(|e| e.to_string())?
}

/// Paths in `git worktree list --porcelain [-z]` output (the main worktree first).
fn parse_worktree_dirs(raw: &[u8]) -> Vec<PathBuf> {
    let sep = if raw.contains(&0) { 0 } else { b'\n' };
    raw.split(|b| *b == sep)
        .filter_map(|field| {
            let field = String::from_utf8_lossy(field);
            let field = field.trim_end_matches('\r');
            field.strip_prefix("worktree ").map(PathBuf::from)
        })
        .collect()
}

/// Every worktree of the repository at `root` (at least `root` itself).
fn worktree_dirs(root: &Path) -> Vec<PathBuf> {
    let raw = crate::git::run_raw(root, &["worktree", "list", "--porcelain", "-z"])
        .or_else(|_| crate::git::run_raw(root, &["worktree", "list", "--porcelain"]))
        .unwrap_or_default();
    let mut dirs = parse_worktree_dirs(&raw);
    if dirs.is_empty() {
        dirs.push(root.to_path_buf());
    }
    dirs
}

/// Stores freshly collected events: nothing past retention, prompt text
/// masked again (whatever the reader did). Returns (git, prompts) changed.
fn ingest(
    store: &Store,
    settings: &Settings,
    now: i64,
    git: Vec<GitEvent>,
    prompts: Vec<PromptEvent>,
) -> store::Result<(usize, usize)> {
    let cutoff = settings.cutoff(now);
    let fresh = |at: i64| cutoff.is_none_or(|c| at >= c);
    let git: Vec<Event> = git
        .into_iter()
        .filter(|e| fresh(e.at))
        .map(Event::Git)
        .collect();
    let prompts: Vec<Event> = prompts
        .into_iter()
        .filter(|e| fresh(e.at))
        .map(|mut e| {
            e.text = redact::prompt_text(&e.text);
            Event::Prompt(e)
        })
        .collect();
    Ok((store.upsert(&git)?, store.upsert(&prompts)?))
}

fn refresh(
    app: &AppHandle,
    store: &Store,
    project: &Path,
    github: bool,
) -> store::Result<RefreshResult> {
    let info = crate::memo::repo::inspect(project)?;
    if !info.is_git {
        return Ok(RefreshResult::default());
    }
    let repo = info.key;
    let root = PathBuf::from(crate::git::run(project, &["rev-parse", "--show-toplevel"])?);
    let dirs = worktree_dirs(&root);

    let mut git_events = Vec::new();
    let mut warning = None;
    if github {
        match git_log::collect_github(app, &repo) {
            Ok(Some(log)) => {
                git_events = log.events;
                warning = log.warning;
                let current = crate::git::run(&root, &["branch", "--show-current"]).ok();
                let tips: Vec<BranchTip> = log
                    .tips
                    .into_iter()
                    .map(|mut t| {
                        t.is_current = current.as_deref() == Some(t.name.as_str());
                        t
                    })
                    .collect();
                store.set_tips(&repo, &tips)?;
            }
            Ok(None) => {}
            // Offline: what is stored stays; the prompts are still read.
            Err(e) => warning = Some(e),
        }
    }

    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let roots = sessions::Roots::detect(&home);
    let mut state = store.scan_state(&repo)?;
    let prompts = sessions::scan(&roots, &dirs, &repo, &mut state);

    let settings = store.settings()?;
    let (git, prompt_count) = ingest(store, &settings, store::now_ms(), git_events, prompts)?;
    store.set_scan_state(&repo, &state)?;
    let pruned = match settings.cutoff(store::now_ms()) {
        Some(cutoff) => store.prune(cutoff)?,
        None => 0,
    };
    if git + prompt_count + pruned > 0 {
        notify(app, repo);
    }
    Ok(RefreshResult {
        git,
        prompts: prompt_count,
        warning,
    })
}

/// Reads GitHub (when `github`) and tool session files into the logs. Safe to call often; unchanged events are not rewritten.
#[tauri::command]
pub async fn activity_refresh(
    app: AppHandle,
    project: String,
    github: bool,
) -> Result<RefreshResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let activity = app.state::<Activity>();
        refresh(&app, activity.store()?, Path::new(&project), github)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn repo_of(project: &str) -> store::Result<String> {
    Ok(crate::memo::repo::inspect(Path::new(project))?.key)
}

#[tauri::command]
pub async fn activity_git_log(
    app: AppHandle,
    project: String,
    limit: usize,
    before: Option<i64>,
    before_id: Option<String>,
) -> Result<GitLog, String> {
    blocking(app, move |store| {
        let repo = repo_of(&project)?;
        Ok(GitLog {
            events: store.git_page(&repo, limit, before, before_id.as_deref())?,
            branches: store.tips(&repo)?,
            repo,
        })
    })
    .await
}

/// The project's commits that are not on GitHub yet and its uncommitted
/// changes, read now from the local `git`.
#[tauri::command]
pub async fn activity_local(project: String) -> Result<LocalLog, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let project = Path::new(&project);
        let info = crate::memo::repo::inspect(project)?;
        if !info.is_git {
            return Ok(LocalLog {
                repo: info.key,
                ..Default::default()
            });
        }
        let root = PathBuf::from(crate::git::run(project, &["rev-parse", "--show-toplevel"])?);
        git_log::collect_local(&root, &info.key, LOCAL_LIMIT)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn activity_prompt_log(
    app: AppHandle,
    project: String,
    limit: usize,
    before: Option<i64>,
    before_id: Option<String>,
) -> Result<PromptLog, String> {
    blocking(app, move |store| {
        let repo = repo_of(&project)?;
        let events = store.prompt_page(&repo, limit, before, before_id.as_deref())?;
        Ok(PromptLog {
            sessions: store::sessions(&events),
            events,
            repo,
        })
    })
    .await
}

#[tauri::command]
pub async fn activity_settings(app: AppHandle) -> Result<Settings, String> {
    blocking(app, |store| store.settings()).await
}

#[tauri::command]
pub async fn activity_set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let pruned = blocking(app.clone(), move |store| {
        let before = store.settings()?;
        store.set_settings(settings)?;
        if settings.sync_prompts && !before.sync_prompts {
            // Prompts other devices uploaded meanwhile were skipped: read again.
            store.set_meta(crate::sync::ACTIVITY_CURSOR_KEY, None)?;
        }
        match settings.cutoff(store::now_ms()) {
            Some(cutoff) => store.prune(cutoff),
            None => Ok(0),
        }
    })
    .await?;
    crate::sync::changed(&app);
    if pruned > 0 {
        let _ = app.emit(
            ACTIVITY_EVENT,
            Changed {
                repo: String::new(),
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: &str, at: i64) -> GitEvent {
        GitEvent {
            id: id.into(),
            repo: "r".into(),
            kind: GitKind::Commit,
            at,
            branch: None,
            sha: None,
            parents: vec![],
            pr: None,
            title: id.into(),
            state: None,
            url: None,
            author: None,
        }
    }

    fn prompt(id: &str, at: i64, text: &str) -> PromptEvent {
        PromptEvent {
            id: id.into(),
            repo: "r".into(),
            tool: ToolId::Claude,
            session: "s".into(),
            at,
            branch: None,
            text: text.into(),
            files: vec![],
            commits: vec![],
            pushed: false,
        }
    }

    #[test]
    fn parses_worktree_list_output() {
        let z = b"worktree /a/main\0HEAD abc\0branch refs/heads/main\0\0worktree /a/wt one\0HEAD def\0detached\0\0";
        assert_eq!(
            parse_worktree_dirs(z),
            [PathBuf::from("/a/main"), PathBuf::from("/a/wt one")]
        );
        let nl = b"worktree /a/main\nHEAD abc\n\nworktree /b\r\nHEAD d\n";
        assert_eq!(
            parse_worktree_dirs(nl),
            [PathBuf::from("/a/main"), PathBuf::from("/b")]
        );
        assert!(parse_worktree_dirs(b"").is_empty());
    }

    #[test]
    fn ingest_drops_events_past_retention_and_masks_prompts() {
        let store = Store::in_memory().unwrap();
        let day = 86_400_000;
        let settings = Settings {
            retention_days: Some(90),
            sync_prompts: true,
        };
        let now = 200 * day;
        let (g, p) = ingest(
            &store,
            &settings,
            now,
            vec![commit("old", 10 * day), commit("new", 150 * day)],
            vec![
                prompt("p-old", 10 * day, "x"),
                prompt("p-new", 150 * day, "key sk-abcdefghijklmnopqrstuvwx"),
            ],
        )
        .unwrap();
        assert_eq!((g, p), (1, 1));
        let prompts = store.prompt_page("r", 10, None, None).unwrap();
        assert_eq!(prompts[0].text, format!("key {}", redact::MASK));

        // Forever: everything is kept.
        let keep = Settings {
            retention_days: None,
            ..settings
        };
        let (g, _) = ingest(&store, &keep, now, vec![commit("old", 10 * day)], vec![]).unwrap();
        assert_eq!(g, 1);
        // The same input again changes nothing.
        let (g, p) = ingest(
            &store,
            &keep,
            now,
            vec![commit("old", 10 * day)],
            vec![prompt(
                "p-new",
                150 * day,
                "key sk-abcdefghijklmnopqrstuvwx",
            )],
        )
        .unwrap();
        assert_eq!((g, p), (0, 0));
    }
}
