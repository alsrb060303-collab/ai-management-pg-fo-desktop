//! Read-only map of the Claude Code configuration that applies to a project:
//! instructions (CLAUDE.md), rules, skills, commands, subagents, settings, MCP
//! servers and plugins, with the scope each comes from and whether it is active.
//!
//! Precedence and toggle rules follow docs/research.md section 1. Nothing here
//! writes files, and credentials (`.credentials.json`, `~/.claude.json`) are
//! never read.

pub(crate) mod frontmatter;
mod scan;

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub use scan::scan;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Instructions,
    Rule,
    Skill,
    Command,
    Agent,
    Settings,
    Mcp,
    Plugin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Organization policy, cannot be overridden.
    Managed,
    /// `~/.claude`, applies to every project.
    User,
    /// A directory above the project (CLAUDE.md files are inherited).
    Parent,
    /// Shared with the team, usually committed.
    Project,
    /// Personal to this checkout, usually gitignored.
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Loaded and usable.
    Active,
    /// Loaded only when matching files are touched (path-scoped rules).
    Conditional,
    /// Loaded with restrictions (listed by name only, user-invocable only, needs approval).
    Limited,
    /// Turned off by a setting or deny rule.
    Off,
    /// Hidden by a same-named item that takes precedence.
    Shadowed,
    /// Present but not read by Claude Code (e.g. AGENTS.md next to a CLAUDE.md).
    NotLoaded,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: Kind,
    pub scope: Scope,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub state: State,
    /// Why the item has its state, or other facts worth showing (Korean UI text).
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMap {
    pub project: String,
    pub repo_root: Option<String>,
    pub items: Vec<Item>,
    /// Caveats about what the map does not cover (Korean UI text).
    pub notes: Vec<String>,
}

/// Where Claude Code looks for user and managed configuration.
#[derive(Clone, Debug)]
pub struct Env {
    /// `CLAUDE_CONFIG_DIR`, or `~/.claude`.
    pub config_dir: PathBuf,
    /// Managed policy directory for this OS.
    pub managed_dir: PathBuf,
}

impl Env {
    pub fn detect(home: PathBuf) -> Self {
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        let managed_dir = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\ClaudeCode")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("/Library/Application Support/ClaudeCode")
        } else {
            PathBuf::from("/etc/claude-code")
        };
        Self {
            config_dir,
            managed_dir,
        }
    }
}

#[tauri::command]
pub async fn claude_config_scan(app: AppHandle, path: String) -> Result<ConfigMap, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let env = Env::detect(home);
    let project = PathBuf::from(path);
    // All filesystem access, including this check, stays off the async workers:
    // a mapped or network drive can block for a long time.
    tauri::async_runtime::spawn_blocking(move || {
        if project.is_dir() {
            Ok(scan(&project, &env))
        } else {
            Err(format!("폴더를 찾을 수 없습니다: {}", project.display()))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
