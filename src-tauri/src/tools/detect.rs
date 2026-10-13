//! Guesses which AI coding tool a project is set up for.
//!
//! 1. Files in the project folder (and its repository root, and for a linked
//!    worktree the main worktree): a tool's own folder or instructions file
//!    counts 2, a file other tools share counts 1. The higher score wins.
//! 2. On a tie (including no files at all): the one tool whose CLI is installed.
//! 3. Otherwise the user's default tool.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use super::{ToolId, cli};

/// File markers, relative to the project: (tool, path, is a directory, weight).
const MARKERS: &[(ToolId, &str, bool, u32)] = &[
    (ToolId::Claude, ".claude", true, 2),
    (ToolId::Claude, "CLAUDE.md", false, 2),
    (ToolId::Claude, "CLAUDE.local.md", false, 2),
    // Also read by Copilot and others.
    (ToolId::Claude, ".mcp.json", false, 1),
    (ToolId::Codex, ".codex", true, 2),
    (ToolId::Codex, "AGENTS.override.md", false, 2),
    // Shared with Copilot, Antigravity and Claude Code (as a fallback).
    (ToolId::Codex, "AGENTS.md", false, 1),
    (ToolId::Codex, ".agents/skills", true, 1),
];

/// Markers that, found in the home folder, are the user's own config.
const USER_FOLDERS: &[&str] = &[".claude", ".codex", ".agents/skills"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Decided by files in the project.
    Files,
    /// No deciding files; only this tool's CLI is installed.
    Installed,
    /// Nothing decided; the user's default tool.
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub tool: ToolId,
    /// The marker, relative to the folder it was found in.
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub tool: ToolId,
    pub reason: Reason,
    pub signals: Vec<Signal>,
    /// Tools whose CLI was found.
    pub installed: Vec<ToolId>,
    /// The main worktree, when the project is in a linked worktree.
    pub main_root: Option<String>,
}

pub fn detect(project: &Path, home: &Path, fallback: ToolId) -> Detection {
    let path = cli::path_dirs();
    let installed: Vec<ToolId> = ToolId::ALL
        .into_iter()
        .filter(|tool| cli::find_with(*tool, &path, Some(home)).is_some())
        .collect();
    decide(project, home, installed, fallback)
}

fn decide(project: &Path, home: &Path, installed: Vec<ToolId>, fallback: ToolId) -> Detection {
    let mut dirs = vec![project.to_path_buf()];
    let root = repo_root(project);
    let main = root.as_deref().and_then(main_worktree);
    for dir in [root, main.clone()].into_iter().flatten() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let mut signals: Vec<Signal> = Vec::new();
    let mut scores = [0u32; ToolId::ALL.len()];
    for (tool, marker, is_dir, weight) in MARKERS {
        let found = dirs.iter().any(|dir| {
            let path = dir.join(marker);
            let exists = if *is_dir {
                path.is_dir()
            } else {
                path.is_file()
            };
            // A user's own config folder (`~/.claude`) is not a project marker,
            // whether the project or its repository is at home.
            exists && !(USER_FOLDERS.contains(marker) && same_dir(dir, home))
        });
        if found {
            scores[index(*tool)] += weight;
            signals.push(Signal {
                tool: *tool,
                path: (*marker).to_string(),
            });
        }
    }

    let best = *scores.iter().max().unwrap_or(&0);
    let leaders: Vec<ToolId> = ToolId::ALL
        .into_iter()
        .filter(|t| scores[index(*t)] == best)
        .collect();
    let (tool, reason) = if best > 0 && leaders.len() == 1 {
        (leaders[0], Reason::Files)
    } else {
        // Among the tied tools (all of them when no files matched).
        let candidates: Vec<ToolId> = installed
            .iter()
            .copied()
            .filter(|t| leaders.contains(t))
            .collect();
        match candidates.as_slice() {
            [only] => (*only, Reason::Installed),
            _ if leaders.contains(&fallback) || best == 0 => (fallback, Reason::Fallback),
            _ => (leaders[0], Reason::Fallback),
        }
    };
    Detection {
        tool,
        reason,
        signals,
        installed,
        main_root: main.map(|m| m.display().to_string()),
    }
}

fn index(tool: ToolId) -> usize {
    ToolId::ALL.iter().position(|t| *t == tool).unwrap_or(0)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Nearest ancestor of `start` (itself included) with a `.git`.
pub(crate) fn repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// For a linked worktree at `root` (its `.git` is a file pointing into
/// `<main>/.git/worktrees/<name>`), the main worktree `<main>`. `None` for a
/// main worktree, a submodule or a worktree of a bare repository.
pub(crate) fn main_worktree(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if !dot_git.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
    let git_dir = clean(&root.join(target));
    // Only worktrees have a `commondir` (submodules don't).
    let common = std::fs::read_to_string(git_dir.join("commondir")).ok()?;
    let common = clean(&git_dir.join(common.trim()));
    if common.file_name() != Some(OsStr::new(".git")) {
        return None;
    }
    let main = common.parent()?.to_path_buf();
    main.is_dir().then_some(main)
}

/// `path` with `.` and `..` resolved without touching the disk.
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// `<main>/.git` plus a linked worktree at `<wt>` the way git lays it out.
    fn link_worktree(main: &Path, wt: &Path) {
        let wt_git = main.join(".git").join("worktrees").join("wt");
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        fs::create_dir_all(wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
    }

    #[test]
    fn files_decide() {
        let dir = project();
        fs::create_dir(dir.path().join(".codex")).unwrap();
        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            vec![ToolId::Claude],
            ToolId::Claude,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Codex, Reason::Files));
        assert_eq!(d.signals[0].path, ".codex");
        assert_eq!(d.main_root, None);
    }

    #[test]
    fn own_folder_beats_shared_file() {
        let dir = project();
        fs::write(dir.path().join("AGENTS.md"), "x").unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "x").unwrap();
        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            vec![],
            ToolId::Codex,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Claude, Reason::Files));
    }

    #[test]
    fn tie_goes_to_the_one_installed_cli_then_the_default() {
        let dir = project();
        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            vec![ToolId::Codex],
            ToolId::Claude,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Codex, Reason::Installed));

        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            ToolId::ALL.to_vec(),
            ToolId::Codex,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Codex, Reason::Fallback));

        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            vec![],
            ToolId::Claude,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Claude, Reason::Fallback));

        // Both set up equally: the installed one among them.
        fs::create_dir(dir.path().join(".claude")).unwrap();
        fs::create_dir(dir.path().join(".codex")).unwrap();
        let d = decide(
            dir.path(),
            Path::new("/nonexistent-home"),
            vec![ToolId::Codex],
            ToolId::Claude,
        );
        assert_eq!((d.tool, d.reason), (ToolId::Codex, Reason::Installed));
    }

    #[test]
    fn repository_root_markers_count_for_a_subfolder() {
        let dir = project();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir(dir.path().join(".codex")).unwrap();
        let sub = dir.path().join("app");
        fs::create_dir(&sub).unwrap();
        let d = decide(&sub, Path::new("/nonexistent-home"), vec![], ToolId::Claude);
        assert_eq!(d.tool, ToolId::Codex);
    }

    #[test]
    fn user_folders_at_home_are_not_markers() {
        let home = project();
        fs::create_dir(home.path().join(".claude")).unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::create_dir_all(home.path().join(".agents").join("skills")).unwrap();

        // The project is home.
        let d = decide(home.path(), home.path(), vec![], ToolId::Claude);
        assert!(d.signals.is_empty(), "{:?}", d.signals);

        // A dotfiles repository rooted at home.
        fs::create_dir(home.path().join(".git")).unwrap();
        let sub = home.path().join("code");
        fs::create_dir(&sub).unwrap();
        let d = decide(&sub, home.path(), vec![], ToolId::Claude);
        assert!(d.signals.is_empty(), "{:?}", d.signals);
        assert_eq!(d.reason, Reason::Fallback);
    }

    #[test]
    fn linked_worktree_uses_main_worktree_markers() {
        let dir = project();
        let main = dir.path().join("repo");
        fs::create_dir_all(main.join(".codex")).unwrap();
        let wt = main.join(".codex").join("worktrees").join("feature");
        link_worktree(&main, &wt);
        assert_eq!(main_worktree(&wt), Some(main.clone()));
        assert_eq!(main_worktree(&main), None);

        let d = decide(&wt, Path::new("/nonexistent-home"), vec![], ToolId::Claude);
        assert_eq!((d.tool, d.reason), (ToolId::Codex, Reason::Files));
        assert_eq!(d.main_root, Some(main.display().to_string()));
    }

    #[test]
    fn submodule_has_no_main_worktree() {
        let dir = project();
        let modules = dir.path().join(".git").join("modules").join("sub");
        fs::create_dir_all(&modules).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(main_worktree(&sub), None);
    }
}
