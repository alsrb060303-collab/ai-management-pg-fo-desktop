//! Claude Code adapter. Read-only map of the Claude Code configuration that applies to a project:
//! instructions (CLAUDE.md), rules, skills, commands, subagents, settings, MCP
//! servers and plugins, with the scope each comes from and whether it is active.
//!
//! Precedence and toggle rules follow docs/research.md section 1. Nothing here
//! writes files, and credentials (`.credentials.json`, `~/.claude.json`) are
//! never read.

mod scan;

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub use scan::scan;

use crate::tools::{ConfigMap, Kind, Scope, State, frontmatter};

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

/// A `/` command the user can type in Claude Code: a skill (unless it hides
/// itself from the menu) or a legacy command.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    /// Without the leading `/`.
    pub name: String,
    /// `skill` or `command`.
    pub kind: &'static str,
    pub scope: Scope,
    pub description: Option<String>,
    /// `argument-hint` frontmatter, e.g. `[issue number]`.
    pub argument_hint: Option<String>,
    /// Only runs when typed (Claude won't pick it up by itself).
    pub manual: bool,
    pub path: String,
}

/// The `/` commands of a project, from the same scan as the settings map:
/// skills and commands that are in effect and not hidden from the `/` menu.
pub fn slash_commands(map: &ConfigMap) -> Vec<SlashCommand> {
    let mut out = Vec::new();
    for item in &map.items {
        let kind = match item.kind {
            Kind::Skill => "skill",
            Kind::Command => "command",
            _ => continue,
        };
        if matches!(item.state, State::Off | State::Shadowed | State::NotLoaded) {
            continue;
        }
        let text = std::fs::read(&item.path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let fm = frontmatter::parse(text.trim_start_matches('\u{feff}')).unwrap_or_default();
        let flag = |k: &str| {
            fm.get(k).is_some_and(|v| {
                matches!(
                    v.trim().to_lowercase().as_str(),
                    "true" | "yes" | "on" | "1"
                )
            })
        };
        let off = |k: &str| {
            fm.get(k).is_some_and(|v| {
                matches!(
                    v.trim().to_lowercase().as_str(),
                    "false" | "no" | "off" | "0"
                )
            })
        };
        if kind == "skill" && off("user-invocable") {
            continue;
        }
        // As written (`[pr]` would otherwise parse as a list).
        let argument_hint = text
            .lines()
            .skip(1)
            .take_while(|l| l.trim() != "---")
            .find_map(|l| l.strip_prefix("argument-hint:"))
            .map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
            .filter(|h| !h.is_empty());
        out.push(SlashCommand {
            name: item.name.trim_start_matches('/').to_string(),
            kind,
            scope: item.scope,
            description: item.description.clone(),
            argument_hint,
            manual: kind == "command" || flag("disable-model-invocation"),
            path: item.path.clone(),
        });
    }
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

#[tauri::command]
pub async fn claude_commands(app: AppHandle, path: String) -> Result<Vec<SlashCommand>, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let env = Env::detect(home);
    let project = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || {
        if project.is_dir() {
            Ok(slash_commands(&scan(&project, &env)))
        } else {
            Err(crate::tr!(
                "폴더를 찾을 수 없습니다: {}",
                "Folder not found: {}",
                project.display()
            ))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
