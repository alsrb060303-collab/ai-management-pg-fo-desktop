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

/// Whether git tracks `path` (best effort: looks for it in the index file).
pub fn is_tracked(path: &Path) -> bool {
    let Some(root) = repo_root(path) else {
        return false;
    };
    let Some(index) =
        exclude_file(&root).and_then(|f| f.parent()?.parent().map(|g| g.join("index")))
    else {
        return false;
    };
    let (Ok(bytes), Ok(rel)) = (fs::read(index), path.strip_prefix(&root)) else {
        return false;
    };
    let needle = rel.to_string_lossy().replace('\\', "/");
    bytes
        .windows(needle.len() + 1)
        .any(|w| w[..needle.len()] == *needle.as_bytes() && w[needle.len()] == 0)
}

// ---------------------------------------------------------------------------
// Claude Code settings

/// What a settings toggle turns off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target<'a> {
    Skill(&'a str),
    Agent(&'a str),
}

/// Turns a skill or subagent off (`enabled = false`) or back on in one settings
/// file, keeping every other key. Skills use `skillOverrides`, subagents a
/// `permissions.deny` rule, as documented by Claude Code.
pub fn set_enabled(settings_file: &Path, target: Target, enabled: bool) -> Result<()> {
    let mut root = match fs::read(settings_file) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
            if text.trim().is_empty() {
                Map::new()
            } else {
                match serde_json::from_str::<Value>(text) {
                    Ok(Value::Object(map)) => map,
                    // Never overwrite a file we cannot parse.
                    _ => {
                        return Err(format!(
                            "{}을(를) 읽을 수 없어 바꾸지 않았습니다 (JSON 오류)",
                            settings_file.display()
                        ));
                    }
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Map::new(),
        Err(e) => return Err(io_err("읽기", settings_file, e)),
    };

    match target {
        Target::Skill(name) => {
            let overrides = object_entry(&mut root, "skillOverrides")?;
            if enabled {
                if overrides.get(name).and_then(Value::as_str) == Some("off") {
                    overrides.remove(name);
                }
            } else {
                overrides.insert(name.to_string(), Value::from("off"));
            }
            if overrides.is_empty() {
                root.remove("skillOverrides");
            }
        }
        Target::Agent(name) => {
            let rule = format!("Agent({name})");
            let permissions = object_entry(&mut root, "permissions")?;
            let deny = match permissions
                .entry("deny")
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                Value::Array(list) => list,
                _ => return Err("permissions.deny가 배열이 아니라 바꾸지 않았습니다".into()),
            };
            let legacy = format!("Task({name})");
            deny.retain(|v| {
                let s = v.as_str();
                !(enabled && (s == Some(rule.as_str()) || s == Some(legacy.as_str())))
                    && !(!enabled && s == Some(rule.as_str()))
            });
            if !enabled {
                deny.push(Value::from(rule));
            }
            if deny.is_empty() {
                permissions.remove("deny");
            }
            if permissions.is_empty() {
                root.remove("permissions");
            }
        }
    }

    let mut text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("JSON 쓰기 실패: {e}"))?;
    text.push('\n');
    write_atomic(settings_file, &text)
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
    fn toggles_skills_and_agents_keeping_other_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(".claude").join("settings.local.json");
        write_atomic(
            &file,
            "\u{feff}{\"model\": \"opus\", \"permissions\": {\"allow\": [\"Bash(ls)\"], \"deny\": [\"Task(old)\"]}}",
        )
        .unwrap();

        set_enabled(&file, Target::Skill("deploy"), false).unwrap();
        set_enabled(&file, Target::Agent("reviewer"), false).unwrap();
        set_enabled(&file, Target::Agent("reviewer"), false).unwrap();
        let v: Value = serde_json::from_str(&read(&file)).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["skillOverrides"]["deploy"], "off");
        assert_eq!(
            v["permissions"]["deny"],
            serde_json::json!(["Task(old)", "Agent(reviewer)"])
        );
        // Key order of the original file is kept.
        assert!(
            read(&file).find("\"model\"").unwrap() < read(&file).find("\"permissions\"").unwrap()
        );

        set_enabled(&file, Target::Skill("deploy"), true).unwrap();
        set_enabled(&file, Target::Agent("reviewer"), true).unwrap();
        set_enabled(&file, Target::Agent("old"), true).unwrap();
        let v: Value = serde_json::from_str(&read(&file)).unwrap();
        assert!(v.get("skillOverrides").is_none());
        assert!(v["permissions"].get("deny").is_none());
        assert_eq!(v["permissions"]["allow"], serde_json::json!(["Bash(ls)"]));
    }

    #[test]
    fn refuses_to_rewrite_broken_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        fs::write(&file, "{ broken").unwrap();
        assert!(set_enabled(&file, Target::Skill("x"), false).is_err());
        assert_eq!(read(&file), "{ broken");
    }
}
