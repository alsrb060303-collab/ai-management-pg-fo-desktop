//! STUB: replaced by the log readers' real implementation on merge.

use std::path::Path;

use super::{BranchTip, GitEvent};

pub fn collect_local(
    _root: &Path,
    _repo: &str,
    _limit: usize,
) -> Result<(Vec<GitEvent>, Vec<BranchTip>), String> {
    Ok((Vec::new(), Vec::new()))
}

pub fn collect_github(
    _app: &tauri::AppHandle,
    _root: &Path,
    _repo: &str,
) -> Result<Vec<GitEvent>, String> {
    Ok(Vec::new())
}
