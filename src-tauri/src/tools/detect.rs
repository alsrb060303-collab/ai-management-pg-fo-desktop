//! Guesses which AI coding tool a project is set up for.
//!
//! 1. Files in the project folder (and its repository root): a tool's own
//!    folder or instructions file counts 2, a file other tools share counts 1.
//!    The higher score wins.
//! 2. On a tie (including no files at all): the one tool whose CLI is installed.
//! 3. Otherwise the user's default tool.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ToolId;

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
}

pub fn detect(project: &Path, home: &Path, fallback: ToolId) -> Detection {
    let installed: Vec<ToolId> = ToolId::ALL
        .into_iter()
        .filter(|tool| is_installed(*tool, home, &path_dirs()))
        .collect();
    decide(project, home, installed, fallback)
}

fn decide(project: &Path, home: &Path, installed: Vec<ToolId>, fallback: ToolId) -> Detection {
    let mut dirs = vec![project.to_path_buf()];
    if let Some(root) = repo_root(project).filter(|r| r != project) {
        dirs.push(root);
    }
    let mut signals: Vec<Signal> = Vec::new();
    let mut scores = [0u32; ToolId::ALL.len()];
    for (tool, marker, is_dir, weight) in MARKERS {
        let found = dirs.iter().any(|dir| {
            let path = dir.join(marker);
            if *is_dir {
                path.is_dir()
            } else {
                path.is_file()
            }
        });
        // A user's own config folder is not a project marker (project at `~`).
        if found && !is_user_folder(project, home, marker) {
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
    }
}

fn index(tool: ToolId) -> usize {
    ToolId::ALL.iter().position(|t| *t == tool).unwrap_or(0)
}

/// `~/.claude` and `~/.codex` are user folders, not project markers.
fn is_user_folder(project: &Path, home: &Path, marker: &str) -> bool {
    matches!(marker, ".claude" | ".codex") && same_dir(project, home)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

/// Executable names a tool's CLI is installed under (npm adds `.cmd` shims on
/// Windows), plus install folders that may be missing from a GUI app's PATH.
fn is_installed(tool: ToolId, home: &Path, path: &[PathBuf]) -> bool {
    let stem = match tool {
        ToolId::Claude => "claude",
        ToolId::Codex => "codex",
    };
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd"]
            .iter()
            .map(|ext| format!("{stem}.{ext}"))
            .collect()
    } else {
        vec![stem.to_string()]
    };
    let mut dirs = path.to_vec();
    // Claude Code's native installer.
    dirs.push(home.join(".local").join("bin"));
    dirs.iter()
        .any(|dir| names.iter().any(|name| dir.join(name).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
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
    fn finds_cli_on_path() {
        let dir = project();
        let name = if cfg!(windows) { "codex.cmd" } else { "codex" };
        fs::write(dir.path().join(name), "").unwrap();
        let home = project();
        assert!(is_installed(
            ToolId::Codex,
            home.path(),
            &[dir.path().to_path_buf()]
        ));
        assert!(!is_installed(
            ToolId::Claude,
            home.path(),
            &[dir.path().to_path_buf()]
        ));
    }
}
