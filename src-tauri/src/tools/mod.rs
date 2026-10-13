//! AI coding tool adapters. Each supported tool (Claude Code, Codex, more
//! later) reads its own files from its own places; this module holds what the
//! app needs to know about each one, behind a common shape:
//!
//! - the config map (`config_scan`): which instructions, skills, agents, MCP
//!   servers and settings apply to a project, from where, and whether active
//! - the file layout the library and the composer write into (`ToolId` methods)
//! - detection (`tool_detect`): which tool a project is set up for
//!
//! Nothing here writes files, and no tool's credentials are ever read.

pub mod claude;
pub mod codex;
mod detect;
pub(crate) mod frontmatter;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub use detect::Detection;

/// A supported AI coding tool. Mirrored in `src/tools.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolId {
    /// Claude Code. The default, and what records from before tools existed mean.
    #[default]
    Claude,
    /// OpenAI Codex (CLI, IDE extension and desktop app share `~/.codex`).
    Codex,
}

impl ToolId {
    pub const ALL: [ToolId; 2] = [ToolId::Claude, ToolId::Codex];

    /// Folder (relative to the project) a library skill `<id>` is copied into.
    pub fn skills_dir(self) -> &'static str {
        match self {
            ToolId::Claude => ".claude/skills",
            // Codex reads repository skills from `.agents/skills`, a location
            // other tools (Copilot, Antigravity) also read.
            ToolId::Codex => ".agents/skills",
        }
    }

    /// File (relative to the project) a subagent `<id>` is written to.
    pub fn agent_file(self, id: &str) -> String {
        match self {
            ToolId::Claude => format!(".claude/agents/{id}.md"),
            ToolId::Codex => format!(".codex/agents/{id}.toml"),
        }
    }

    /// Whether subagents are TOML files (converted from the library's Markdown).
    pub fn agents_are_toml(self) -> bool {
        self == ToolId::Codex
    }

    /// The always-loaded instructions file the composer writes its section to.
    pub fn instructions_file(self, personal: bool) -> &'static str {
        match (self, personal) {
            (ToolId::Claude, false) => "CLAUDE.md",
            (ToolId::Claude, true) => "CLAUDE.local.md",
            // Codex has no personal instructions file next to AGENTS.md
            // (AGENTS.override.md replaces it), so a personal section goes into
            // AGENTS.md kept out of git.
            (ToolId::Codex, _) => "AGENTS.md",
        }
    }

    /// Whether the tool has path-scoped instruction files (`.claude/rules`).
    pub fn has_rules(self) -> bool {
        self == ToolId::Claude
    }

    /// Whether a single subagent can be turned off for one project.
    pub fn can_toggle_agents(self) -> bool {
        self == ToolId::Claude
    }
}

// ---------------------------------------------------------------------------
// Config map

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Instructions,
    /// Path-scoped instructions (`.claude/rules`).
    Rule,
    Skill,
    Command,
    Agent,
    Settings,
    Mcp,
    Plugin,
    /// Command execution policy (Codex `rules/*.rules`), not instructions.
    Policy,
    Hook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Organization policy, cannot be overridden.
    Managed,
    /// The tool's user folder (`~/.claude`, `~/.codex`), applies to every project.
    User,
    /// A directory above the project (instruction files are inherited).
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
    /// Present but not read by the tool (e.g. AGENTS.md next to a CLAUDE.md).
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
    /// Why the item has its state, or other facts worth showing (UI text).
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMap {
    pub tool: ToolId,
    pub project: String,
    pub repo_root: Option<String>,
    pub items: Vec<Item>,
    /// Caveats about what the map does not cover (UI text).
    pub notes: Vec<String>,
}

/// Map of `tool`'s configuration for `project`.
pub fn scan(tool: ToolId, project: &Path, home: &Path) -> ConfigMap {
    match tool {
        ToolId::Claude => claude::scan(project, &claude::Env::detect(home.to_path_buf())),
        ToolId::Codex => codex::scan(project, &codex::Env::detect(home.to_path_buf())),
    }
}

// ---------------------------------------------------------------------------
// Commands

fn home(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().home_dir().map_err(|e| e.to_string())
}

fn folder_not_found(project: &Path) -> String {
    crate::tr!(
        "폴더를 찾을 수 없습니다: {}",
        "Folder not found: {}",
        project.display()
    )
}

#[tauri::command]
pub async fn config_scan(app: AppHandle, path: String, tool: ToolId) -> Result<ConfigMap, String> {
    let home = home(&app)?;
    let project = PathBuf::from(path);
    // All filesystem access, including this check, stays off the async workers:
    // a mapped or network drive can block for a long time.
    tauri::async_runtime::spawn_blocking(move || {
        if project.is_dir() {
            Ok(scan(tool, &project, &home))
        } else {
            Err(folder_not_found(&project))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Which tool `path` looks set up for. `fallback` (the user's default tool) is
/// used when nothing, not even an installed CLI, points to one tool.
#[tauri::command]
pub async fn tool_detect(
    app: AppHandle,
    path: String,
    fallback: ToolId,
) -> Result<Detection, String> {
    let home = home(&app)?;
    let project = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || {
        if project.is_dir() {
            Ok(detect::detect(&project, &home, fallback))
        } else {
            Err(folder_not_found(&project))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
