//! Codex adapter. Read-only map of the Codex configuration that applies to a
//! project: AGENTS.md instructions, config.toml layers, skills, custom agents,
//! MCP servers, custom prompts, command rules and hooks. Facts follow
//! docs/research.md section 7.
//! Credentials (`auth.json`) are never read.

mod extensions;
mod instructions;
mod scan;
#[cfg(test)]
mod tests;
pub(crate) mod util;

use std::path::PathBuf;

pub use scan::scan;

/// Where Codex looks for user, admin and managed configuration.
#[derive(Clone, Debug)]
pub struct Env {
    /// `CODEX_HOME`, or `~/.codex`.
    pub codex_home: PathBuf,
    /// The user's home, for `~/.agents/skills`.
    pub home: PathBuf,
    /// System config folder (`/etc/codex` on Unix).
    pub system_dir: Option<PathBuf>,
    /// Folder holding the managed `requirements.toml`.
    pub managed_dir: PathBuf,
}

impl Env {
    pub fn detect(home: PathBuf) -> Self {
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let (system_dir, managed_dir) = if cfg!(windows) {
            let program_data = std::env::var_os("ProgramData")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
            (None, program_data.join("OpenAI").join("Codex"))
        } else {
            (
                Some(PathBuf::from("/etc/codex")),
                PathBuf::from("/etc/codex"),
            )
        };
        Self {
            codex_home,
            home,
            system_dir,
            managed_dir,
        }
    }
}
