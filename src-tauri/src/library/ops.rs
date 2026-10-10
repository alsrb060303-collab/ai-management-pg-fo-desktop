//! File operations behind the library: hashing, copying, git excludes and
//! Claude Code settings toggles. Everything takes explicit paths so it can be
//! tested without the app.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Marks lines this app adds to `.git/info/exclude`.
const EXCLUDE_TAG: &str = "# ai-management";

pub type Result<T> = std::result::Result<T, String>;

fn io_err(context: &str, path: &Path, e: io::Error) -> String {
    format!("{context} 실패 ({}): {e}", path.display())
}

// ---------------------------------------------------------------------------
// Hashing and copying

/// SHA-256 over a file, or over every file under a directory (relative paths
/// and contents, in a stable order). `None` when the path does not exist.
pub fn hash_path(path: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    if path.is_file() {
        hasher.update(fs::read(path).ok()?);
    } else if path.is_dir() {
        for file in files_under(path) {
            let rel = file.strip_prefix(path).unwrap_or(&file);
            hasher.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
            hasher.update([0]);
            hasher.update(fs::read(&file).ok()?);
            hasher.update([0]);
        }
    } else {
        return None;
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() => out.push(path),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// Copies a file or directory tree to `dest`, which must not exist yet.
pub fn copy_new(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(format!("이미 있습니다: {}", dest.display()));
    }
    copy_over(src, dest)
}

/// Copies a file or directory tree to `dest`, replacing what is there.
pub fn copy_over(src: &Path, dest: &Path) -> Result<()> {
    remove_path(dest)?;
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err("폴더 만들기", parent, e))?;
        }
        fs::copy(src, dest).map_err(|e| io_err("복사", dest, e))?;
        return Ok(());
    }
    for file in files_under(src) {
        let target = dest.join(file.strip_prefix(src).unwrap_or(&file));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err("폴더 만들기", parent, e))?;
        }
        fs::copy(&file, &target).map_err(|e| io_err("복사", &target, e))?;
    }
    Ok(())
}

pub fn remove_path(path: &Path) -> Result<()> {
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        return Ok(());
    };
    result.map_err(|e| io_err("삭제", path, e))
}

// ---------------------------------------------------------------------------
// Git excludes

/// The repository root containing `path`, if any.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// `info/exclude` for the repository at `root`, following worktree links.
fn exclude_file(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        // Worktrees and submodules: ".git" is a file with "gitdir: <path>".
        let text = fs::read_to_string(&dot_git).ok()?;
        let target = text.trim().strip_prefix("gitdir:")?.trim();
        let git_dir = root.join(target);
        // A worktree reads excludes from the common git dir.
        match fs::read_to_string(git_dir.join("commondir")) {
            Ok(common) => git_dir.join(common.trim()),
            Err(_) => git_dir,
        }
    };
    Some(git_dir.join("info").join("exclude"))
}

/// Exclude pattern for `path` relative to the repository root.
fn exclude_pattern(root: &Path, path: &Path, is_dir: bool) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    Some(if is_dir {
        format!("/{rel}/")
    } else {
        format!("/{rel}")
    })
}

/// Adds `path` to the repository's local excludes. No-op outside a repository.
/// Gitignore files have no trailing comments, so our tag goes on its own line
/// right above the pattern.
pub fn exclude_add(path: &Path, is_dir: bool) -> Result<()> {
    let Some(root) = repo_root(path) else {
        return Ok(());
    };
    let (Some(file), Some(pattern)) = (exclude_file(&root), exclude_pattern(&root, path, is_dir))
    else {
        return Ok(());
    };
    let mut text = fs::read_to_string(&file).unwrap_or_default();
    if text.lines().any(|l| l.trim() == pattern) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{EXCLUDE_TAG}\n{pattern}\n"));
    write_atomic(&file, &text)
}

/// Removes a pattern this app added for `path` (and its tag line).
pub fn exclude_remove(path: &Path, is_dir: bool) -> Result<()> {
    let Some(root) = repo_root(path) else {
        return Ok(());
    };
    let (Some(file), Some(pattern)) = (exclude_file(&root), exclude_pattern(&root, path, is_dir))
    else {
        return Ok(());
    };
    let Ok(text) = fs::read_to_string(&file) else {
        return Ok(());
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut kept = Vec::with_capacity(lines.len());
    let mut changed = false;
    for (i, line) in lines.iter().enumerate() {
        let is_ours = |j: usize| j > 0 && lines[j - 1].trim() == EXCLUDE_TAG;
        if line.trim() == pattern && is_ours(i) {
            kept.pop();
            changed = true;
            continue;
        }
        kept.push(*line);
    }
    if !changed {
        return Ok(());
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    write_atomic(&file, &out)
}

/// Whether git tracks `path`. Asks git itself; without git the answer is "no".
pub fn is_tracked(path: &Path) -> bool {
    let Some(root) = repo_root(path) else {
        return false;
    };
    let Ok(rel) = path.strip_prefix(&root) else {
        return false;
    };
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(&root)
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(rel)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
    }
    cmd.status().is_ok_and(|s| s.success())
}

/// The shared `info/exclude` file `path` would be excluded through, if any.
/// Linked worktrees of one repository share it.
pub fn exclude_file_for(path: &Path) -> Option<PathBuf> {
    let file = exclude_file(&repo_root(path)?)?;
    // Normalize `worktrees/<name>/../..` so worktrees compare equal.
    let git_dir = file.parent()?.parent()?;
    Some(
        fs::canonicalize(git_dir)
            .map(|d| d.join("info").join("exclude"))
            .unwrap_or(file),
    )
}

// ---------------------------------------------------------------------------
// Claude Code settings

/// One edit to a Claude Code settings file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change<'a> {
    /// Set `skillOverrides[name]` to `value`, or remove the key for `None`.
    SkillOverride {
        name: &'a str,
        value: Option<&'a str>,
    },
    /// Add (`deny = true`) or remove the `Agent(name)` rule in `permissions.deny`.
    AgentDeny { name: &'a str, deny: bool },
}

/// Current `skillOverrides[name]` in a settings file.
pub fn skill_override(settings_file: &Path, name: &str) -> Result<Option<String>> {
    let root = read_settings(settings_file)?;
    Ok(root
        .get("skillOverrides")
        .and_then(|o| o.get(name))
        .and_then(Value::as_str)
        .map(str::to_string))
}

/// Applies `change` to a settings file, keeping every other key. Writes only
/// when something changed, and only removes a container this edit emptied.
/// Returns whether the file was written.
pub fn apply_change(settings_file: &Path, change: Change) -> Result<bool> {
    let mut root = read_settings(settings_file)?;
    let changed = match change {
        Change::SkillOverride { name, value } => {
            let had_container = root.contains_key("skillOverrides");
            let overrides = object_entry(&mut root, "skillOverrides")?;
            let before = overrides.get(name).cloned();
            match value {
                Some(v) => {
                    overrides.insert(name.to_string(), Value::from(v));
                }
                None => {
                    overrides.remove(name);
                }
            }
            let changed = overrides.get(name).cloned() != before;
            if overrides.is_empty() && (!had_container || changed) {
                root.remove("skillOverrides");
            }
            changed
        }
        Change::AgentDeny { name, deny } => {
            let rule = Value::from(format!("Agent({name})"));
            let had_permissions = root.contains_key("permissions");
            let permissions = object_entry(&mut root, "permissions")?;
            let had_deny = permissions.contains_key("deny");
            let list = match permissions
                .entry("deny")
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                Value::Array(list) => list,
                _ => return Err("permissions.deny가 배열이 아니라 바꾸지 않았습니다".into()),
            };
            let present = list.contains(&rule);
            let changed = present != deny;
            if deny && !present {
                list.push(rule);
            } else if !deny && present {
                list.retain(|v| *v != rule);
            }
            if list.is_empty() && (!had_deny || changed) {
                permissions.remove("deny");
            }
            if permissions.is_empty() && (!had_permissions || changed) {
                root.remove("permissions");
            }
            changed
        }
    };
    if !changed {
        return Ok(false);
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("JSON 쓰기 실패: {e}"))?;
    text.push('\n');
    write_atomic(settings_file, &text)?;
    Ok(true)
}

/// Parses a settings file; a missing or empty file is an empty object. A file
/// that does not parse is an error, so it is never overwritten.
fn read_settings(settings_file: &Path) -> Result<Map<String, Value>> {
    let bytes = match fs::read(settings_file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(io_err("읽기", settings_file, e)),
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(format!(
            "{}을(를) 읽을 수 없어 바꾸지 않았습니다 (JSON 오류)",
            settings_file.display()
        )),
    }
}

fn object_entry<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>> {
    match root.entry(key).or_insert_with(|| Value::Object(Map::new())) {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{key}가 객체가 아니라 바꾸지 않았습니다")),
    }
}

/// Writes through a temporary file so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err("폴더 만들기", parent, e))?;
    }
    let tmp = path.with_extension("ai-management.tmp");
    fs::write(&tmp, text).map_err(|e| io_err("쓰기", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err("쓰기", path, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn hash_changes_with_content_and_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("s");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("SKILL.md"), "a").unwrap();
        fs::write(dir.join("sub").join("x.sh"), "b").unwrap();
        let first = hash_path(&dir).unwrap();
        fs::write(dir.join("sub").join("x.sh"), "c").unwrap();
        assert_ne!(first, hash_path(&dir).unwrap());
        assert!(hash_path(&tmp.path().join("missing")).is_none());
    }

    #[test]
    fn copies_trees_and_refuses_to_clobber() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("SKILL.md"), "a").unwrap();
        fs::write(src.join("sub").join("x"), "b").unwrap();
        let dest = tmp.path().join("dest");
        copy_new(&src, &dest).unwrap();
        assert_eq!(hash_path(&src), hash_path(&dest));
        assert!(copy_new(&src, &dest).is_err());
    }

    #[test]
    fn manages_git_excludes_including_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(repo.join(".git").join("info")).unwrap();
        fs::write(repo.join(".git").join("info").join("exclude"), "*.log").unwrap();
        let skill = repo.join(".claude").join("skills").join("deploy");

        exclude_add(&skill, true).unwrap();
        exclude_add(&skill, true).unwrap();
        let text = read(&repo.join(".git").join("info").join("exclude"));
        assert_eq!(text, "*.log\n# ai-management\n/.claude/skills/deploy/\n");
        exclude_remove(&skill, true).unwrap();
        assert_eq!(
            read(&repo.join(".git").join("info").join("exclude")),
            "*.log\n"
        );

        // Worktree: ".git" file -> gitdir with a commondir back to the main repo.
        let wt_git = repo.join(".git").join("worktrees").join("feature");
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(wt_git.join("commondir"), "../..").unwrap();
        let wt = tmp.path().join("feature");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        exclude_add(&wt.join(".claude").join("agents").join("a.md"), false).unwrap();
        assert!(
            read(&repo.join(".git").join("info").join("exclude"))
                .contains("# ai-management\n/.claude/agents/a.md\n")
        );
    }

    #[test]
    fn changes_keep_other_keys_and_user_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(".claude").join("settings.local.json");
        write_atomic(
            &file,
            "\u{feff}{\"model\": \"opus\", \"permissions\": {\"allow\": [\"Bash(ls)\"], \"deny\": [\"Task(old)\"]}}",
        )
        .unwrap();

        let skill_off = Change::SkillOverride {
            name: "deploy",
            value: Some("off"),
        };
        let agent_off = Change::AgentDeny {
            name: "reviewer",
            deny: true,
        };
        assert!(apply_change(&file, skill_off).unwrap());
        assert!(apply_change(&file, agent_off).unwrap());
        assert!(
            !apply_change(&file, agent_off).unwrap(),
            "no rewrite when nothing changes"
        );
        let v: Value = serde_json::from_str(&read(&file)).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["skillOverrides"]["deploy"], "off");
        assert_eq!(
            v["permissions"]["deny"],
            serde_json::json!(["Task(old)", "Agent(reviewer)"])
        );
        assert!(
            read(&file).find("\"model\"").unwrap() < read(&file).find("\"permissions\"").unwrap()
        );

        apply_change(
            &file,
            Change::SkillOverride {
                name: "deploy",
                value: None,
            },
        )
        .unwrap();
        apply_change(
            &file,
            Change::AgentDeny {
                name: "reviewer",
                deny: false,
            },
        )
        .unwrap();
        let v: Value = serde_json::from_str(&read(&file)).unwrap();
        assert!(v.get("skillOverrides").is_none());
        // The user's own legacy rule stays.
        assert_eq!(v["permissions"]["deny"], serde_json::json!(["Task(old)"]));
        assert_eq!(skill_override(&file, "deploy").unwrap(), None);
    }

    #[test]
    fn untouched_file_is_not_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        let original = "{\n\t\"skillOverrides\": {},\n\t\"x\": 1\n}\n";
        fs::write(&file, original).unwrap();
        let changed = apply_change(
            &file,
            Change::SkillOverride {
                name: "lint",
                value: None,
            },
        )
        .unwrap();
        assert!(!changed);
        assert_eq!(read(&file), original);
    }

    #[test]
    fn refuses_to_rewrite_broken_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        fs::write(&file, "{ broken").unwrap();
        let change = Change::SkillOverride {
            name: "x",
            value: Some("off"),
        };
        assert!(apply_change(&file, change).is_err());
        assert_eq!(read(&file), "{ broken");
    }
}
