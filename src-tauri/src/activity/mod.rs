//! Work trees: two separate logs per repository, drawn as time-ordered trees.
//!
//! - GitHub log (`GitEvent`): commits on every branch, pushes, merges, and
//!   GitHub pull requests and CI results, collected from the local `git` and
//!   the GitHub API (`collect_git`).
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

use serde::{Deserialize, Serialize};

use crate::tools::ToolId;

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
    /// (`git:<kind>:<sha or pr number>[:<extra>]`), so re-collecting is idempotent.
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
