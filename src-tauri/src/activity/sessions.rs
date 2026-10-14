//! STUB: replaced by the log readers' real implementation on merge.

use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct Roots {
    pub claude_projects: PathBuf,
    pub codex_home: PathBuf,
}

impl Roots {
    pub fn detect(home: &Path) -> Self {
        Roots {
            claude_projects: home.join(".claude").join("projects"),
            codex_home: home.join(".codex"),
        }
    }
}

/// Prompts from sessions whose cwd is inside one of `dirs` (the repo's worktrees), new since `state` (opaque JSON the function updates).
pub fn scan(
    _roots: &Roots,
    _dirs: &[PathBuf],
    _repo: &str,
    _state: &mut serde_json::Value,
) -> Vec<super::PromptEvent> {
    Vec::new()
}
