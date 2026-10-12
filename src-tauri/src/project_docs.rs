//! The Claude Code documents inside a project folder, to read and edit in the
//! app: every CLAUDE.md / CLAUDE.local.md / AGENTS.md (subfolders included),
//! and the Markdown and settings files under `.claude/`, plus `.mcp.json`.
//! Paths go back and forth relative to the project (`/`-separated) and are
//! checked to stay inside it.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// Folders never worth walking (build output, dependencies, VCS data).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "venv",
    "__pycache__",
    "coverage",
];
const INSTRUCTION_FILES: &[&str] = &["CLAUDE.md", "CLAUDE.local.md", "AGENTS.md"];
const MAX_DEPTH: usize = 10;
const MAX_FILES: usize = 1000;
/// Larger files are listed but not opened in the editor.
const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    /// Relative to the project, `/`-separated.
    pub rel: String,
    /// "instructions", "rule", "skill", "agent", "command", "settings",
    /// "mcp" or "other".
    pub kind: &'static str,
    pub size: u64,
    /// Epoch ms of the last change.
    pub modified: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocText {
    pub text: String,
    pub modified: i64,
}

/// What a project-relative path is, or `None` when the app doesn't edit it.
fn kind_of(rel: &str) -> Option<&'static str> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let in_claude = rel.starts_with(".claude/") || rel.contains("/.claude/");
    let under = |dir: &str| {
        rel.starts_with(&format!(".claude/{dir}/")) || rel.contains(&format!("/.claude/{dir}/"))
    };
    let md = name.ends_with(".md");
    if INSTRUCTION_FILES.contains(&name) {
        return Some("instructions");
    }
    if rel == ".mcp.json" {
        return Some("mcp");
    }
    if !in_claude {
        return None;
    }
    if name == "settings.json" || name == "settings.local.json" {
        return Some("settings");
    }
    if !md {
        return None;
    }
    Some(if under("rules") {
        "rule"
    } else if under("skills") {
        "skill"
    } else if under("agents") {
        "agent"
    } else if under("commands") {
        "command"
    } else {
        "other"
    })
}

fn modified_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn list(root: &Path) -> Result<Vec<DocEntry>> {
    if !root.is_dir() {
        return Err(crate::tr!(
            "폴더를 찾을 수 없어요: {path}",
            "Folder not found: {path}",
            path = root.display()
        ));
    }
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out.sort_by(|a, b| {
        // Root files before subfolders', then by path.
        (a.rel.matches('/').count(), &a.rel).cmp(&(b.rel.matches('/').count(), &b.rel))
    });
    Ok(out)
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<DocEntry>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        // Symlinks are not followed (no cycles, nothing outside the project).
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if kind.is_dir() {
            let hidden = name.starts_with('.') && name != ".claude";
            if !hidden && !SKIP_DIRS.contains(&name.as_str()) {
                walk(root, &path, depth + 1, out);
            }
        } else if kind.is_file() {
            let rel = rel_of(root, &path);
            if let (Some(kind), Ok(meta)) = (kind_of(&rel), entry.metadata()) {
                out.push(DocEntry {
                    rel,
                    kind,
                    size: meta.len(),
                    modified: modified_ms(&meta),
                });
            }
        }
    }
}

/// The file for `rel`, refusing paths that leave the project or that the app
/// doesn't edit.
fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim().replace('\\', "/");
    let bad = || {
        crate::tr!(
            "이 앱에서 다룰 수 없는 경로예요: {rel}",
            "This path can't be edited here: {rel}",
            rel = rel
        )
    };
    let path = Path::new(&rel);
    if rel.is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(bad());
    }
    if kind_of(&rel).is_none() {
        return Err(bad());
    }
    let full = root.join(path);
    // Whatever exists of the path must resolve inside the project (no
    // symlinked folder pointing elsewhere).
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let existing = full
        .ancestors()
        .find(|p| p.exists())
        .ok_or_else(bad)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !existing.starts_with(&root) {
        return Err(bad());
    }
    Ok(full)
}

pub fn read(root: &Path, rel: &str) -> Result<DocText> {
    let path = resolve(root, rel)?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_EDIT_BYTES {
        return Err(crate::tr!(
            "파일이 너무 커서 열 수 없어요 (2MB 초과)",
            "The file is too large to open (over 2 MB)"
        ));
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| {
        crate::tr!(
            "UTF-8 텍스트 파일이 아니라서 열 수 없어요",
            "Not a UTF-8 text file, so it can't be opened"
        )
    })?;
    Ok(DocText {
        text,
        modified: modified_ms(&meta),
    })
}

pub const CONFLICT: &str = "conflict";

/// Writes `text`. `expected` is the change time the editor loaded (`None`
/// for a new file); a file changed or created since is refused with
/// `CONFLICT` unless `force`. Returns the new change time.
pub fn write(
    root: &Path,
    rel: &str,
    text: &str,
    expected: Option<i64>,
    force: bool,
) -> Result<i64> {
    let path = resolve(root, rel)?;
    if rel.ends_with(".json")
        && !text.trim().is_empty()
        && let Err(e) = serde_json::from_str::<serde_json::Value>(text)
    {
        return Err(crate::tr!(
            "JSON 형식이 올바르지 않아서 저장하지 않았어요: {e}",
            "Not saved: the JSON isn't valid ({e})"
        ));
    }
    let current = fs::metadata(&path).ok().map(|m| modified_ms(&m));
    if !force && current != expected {
        return Err(CONFLICT.into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Write next to it, then rename: a crash never leaves half a file.
    let tmp = path.with_extension("aim-tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    fs::metadata(&path)
        .map(|m| modified_ms(&m))
        .map_err(|e| e.to_string())
}

pub fn delete(root: &Path, rel: &str) -> Result<()> {
    let path = resolve(root, rel)?;
    fs::remove_file(&path).map_err(|e| e.to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn project_docs_list(project: String) -> Result<Vec<DocEntry>> {
    blocking(move || list(Path::new(&project))).await
}

#[tauri::command]
pub async fn project_doc_read(project: String, rel: String) -> Result<DocText> {
    blocking(move || read(Path::new(&project), &rel)).await
}

#[tauri::command]
pub async fn project_doc_write(
    project: String,
    rel: String,
    text: String,
    expected: Option<i64>,
    force: bool,
) -> Result<i64> {
    blocking(move || write(Path::new(&project), &rel, &text, expected, force)).await
}

#[tauri::command]
pub async fn project_doc_delete(project: String, rel: String) -> Result<()> {
    blocking(move || delete(Path::new(&project), &rel)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aim-docs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (rel, text) in [
            ("CLAUDE.md", "root"),
            ("src/CLAUDE.md", "sub"),
            ("src/deep/AGENTS.md", "agents"),
            (".claude/rules/style.md", "rule"),
            (".claude/skills/deploy/SKILL.md", "skill"),
            (".claude/agents/reviewer.md", "agent"),
            (".claude/commands/fix.md", "cmd"),
            (".claude/settings.json", "{}"),
            (".claude/skills/deploy/run.sh", "echo"),
            (".mcp.json", "{}"),
            ("README.md", "not ours"),
            ("node_modules/pkg/CLAUDE.md", "skipped"),
            (".git/CLAUDE.md", "skipped"),
        ] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn lists_claude_docs_in_subfolders_and_skips_the_rest() {
        let dir = project("list");
        let found: Vec<_> = list(&dir)
            .unwrap()
            .into_iter()
            .map(|d| (d.rel, d.kind))
            .collect();
        assert_eq!(
            found,
            [
                (".mcp.json".to_string(), "mcp"),
                ("CLAUDE.md".into(), "instructions"),
                (".claude/settings.json".into(), "settings"),
                ("src/CLAUDE.md".into(), "instructions"),
                (".claude/agents/reviewer.md".into(), "agent"),
                (".claude/commands/fix.md".into(), "command"),
                (".claude/rules/style.md".into(), "rule"),
                ("src/deep/AGENTS.md".into(), "instructions"),
                (".claude/skills/deploy/SKILL.md".into(), "skill"),
            ]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_check_for_outside_changes() {
        let dir = project("write");
        let doc = read(&dir, "src/CLAUDE.md").unwrap();
        assert_eq!(doc.text, "sub");
        // A stale change time is a conflict; forcing overwrites.
        assert_eq!(
            write(&dir, "src/CLAUDE.md", "x", Some(doc.modified - 1), false),
            Err(CONFLICT.to_string())
        );
        let at = write(&dir, "src/CLAUDE.md", "edited", Some(doc.modified), false).unwrap();
        assert_eq!(read(&dir, "src/CLAUDE.md").unwrap().text, "edited");
        assert_eq!(read(&dir, "src/CLAUDE.md").unwrap().modified, at);
        write(&dir, "src/CLAUDE.md", "forced", None, true).unwrap();

        // New files: refused when one exists, folders created as needed.
        assert_eq!(
            write(&dir, "CLAUDE.md", "x", None, false),
            Err(CONFLICT.to_string())
        );
        write(
            &dir,
            ".claude/skills/new/SKILL.md",
            "---\nname: new\n---\n",
            None,
            false,
        )
        .unwrap();
        assert!(dir.join(".claude/skills/new/SKILL.md").is_file());

        // Settings must stay valid JSON.
        let settings = read(&dir, ".claude/settings.json").unwrap();
        assert!(
            write(
                &dir,
                ".claude/settings.json",
                "{",
                Some(settings.modified),
                false
            )
            .is_err()
        );

        delete(&dir, ".claude/skills/new/SKILL.md").unwrap();
        assert!(!dir.join(".claude/skills/new/SKILL.md").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_stay_inside_the_project_and_on_claude_docs() {
        let dir = project("paths");
        for bad in [
            "../CLAUDE.md",
            "/etc/CLAUDE.md",
            "src/../../CLAUDE.md",
            "README.md",
            ".claude/skills/deploy/run.sh",
            "",
        ] {
            assert!(read(&dir, bad).is_err(), "{bad}");
            assert!(write(&dir, bad, "x", None, true).is_err(), "{bad}");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
