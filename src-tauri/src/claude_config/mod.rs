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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_commands_list_what_the_user_can_type() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("p");
        let skill = |name: &str, fm: &str| {
            let dir = project.join(".claude").join("skills").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), format!("---\n{fm}\n---\nbody")).unwrap();
        };
        skill(
            "review",
            "name: review\ndescription: Reviews code\nargument-hint: [pr]",
        );
        skill(
            "grill",
            "name: grill\ndescription: Asks\ndisable-model-invocation: true",
        );
        skill(
            "hidden",
            "name: hidden\ndescription: Only Claude\nuser-invocable: false",
        );
        let cmds = project.join(".claude").join("commands").join("git");
        std::fs::create_dir_all(&cmds).unwrap();
        std::fs::write(cmds.join("push.md"), "---\ndescription: Push it\n---\nGo").unwrap();
        let env = Env {
            config_dir: tmp.path().join("home"),
            managed_dir: tmp.path().join("managed"),
        };
        let list = slash_commands(&scan(&project, &env));
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["git:push", "grill", "review"]);
        let review = &list[2];
        assert_eq!(review.argument_hint.as_deref(), Some("[pr]"));
        assert!(!review.manual);
        assert!(list[1].manual && list[0].manual);
        assert_eq!(list[0].kind, "command");
    }
}
