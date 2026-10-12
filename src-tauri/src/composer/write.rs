//! Writes compiled files into a project without clobbering the user's work.
//!
//! - Section files (CLAUDE.md): only the text between our markers is ours.
//!   The rest of the file is left as is; without markers the section is
//!   appended. Claude Code strips block-level HTML comments, so the markers
//!   cost no context.
//! - Whole files (rules, skills, agents): written when new, or when the file
//!   still has the content we last wrote.
//!
//! - Hooks (settings.json): the file is read, the hooks we wrote last time
//!   are taken out, ours are added, and every other key is kept. Since hooks
//!   run commands, each change needs the user's confirmation.
//!
//! Anything the user changed since our last write, or a file we did not write,
//! is only overwritten after the user confirms that path.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::catalog::normalize;
use super::compile::{OutFile, Target, add_hook, import_tokens};
use crate::library::ops;

pub const BEGIN: &str = "<!-- ai-management:begin (managed by AI Management; edits here are replaced on the next write) -->";
pub const END: &str = "<!-- ai-management:end -->";
const BEGIN_PREFIX: &str = "<!-- ai-management:begin";

/// What we last wrote to a path: the whole file's hash, or the section's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    pub project: String,
    pub path: String,
    pub hash: String,
    /// Hooks outputs: the `{"hooks": ...}` we merged in, so the next write
    /// can take exactly those out again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "reason")]
pub enum Status {
    New,
    Update,
    Unchanged,
    /// We wrote it before, and the user has edited it since.
    Modified,
    /// Exists, but we did not write it.
    Exists,
    /// Adds or changes hook commands, which Claude Code runs by itself.
    Commands,
    /// Cannot be written; the reason says why.
    Blocked(String),
}

impl Status {
    pub fn needs_confirm(&self) -> bool {
        matches!(self, Status::Modified | Status::Exists | Status::Commands)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub path: String,
    pub status: Status,
    pub personal: bool,
    /// Current and resulting full file text, for the diff.
    pub old: Option<String>,
    pub new: String,
    /// Hash of `old`; a confirmation to overwrite is only valid for this text.
    pub old_hash: Option<String>,
    /// Something the user should know before writing.
    pub note: Option<String>,
}

pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn read(path: &Path) -> ops::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            crate::tr!(
                "UTF-8 텍스트가 아니라 건드리지 않았어요: {}",
                "Left untouched because it isn't UTF-8 text: {}",
                path.display()
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::tr!(
            "읽지 못했어요: {} ({e})",
            "Couldn't read {} ({e})",
            path.display()
        )),
    }
}

/// Byte range of the managed section (marker lines included, through the end
/// marker's line break), or `None` without markers. Broken markers are an error.
fn section_range(text: &str) -> Result<Option<(usize, usize)>, String> {
    let mut begin = None;
    let mut end = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        // An editor may have added a BOM in front of the first line.
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.starts_with(BEGIN_PREFIX) {
            if begin.is_some() {
                return Err(crate::tr!(
                    "관리 구역 시작 표시가 두 번 있어요",
                    "The managed section's begin marker appears twice"
                ));
            }
            begin = Some(offset);
        } else if trimmed == END {
            if begin.is_none() || end.is_some() {
                return Err(crate::tr!(
                    "관리 구역 표시 순서가 맞지 않아요",
                    "The managed section's markers are out of order"
                ));
            }
            end = Some(offset + line.len());
        }
        offset += line.len();
    }
    match (begin, end) {
        (None, None) => Ok(None),
        (Some(b), Some(e)) => Ok(Some((b, e))),
        _ => Err(crate::tr!(
            "관리 구역 끝 표시가 없어요",
            "The managed section's end marker is missing"
        )),
    }
}

/// The section body between the markers, as `compile` produces it.
fn section_body(section: &str) -> String {
    let mut lines: Vec<&str> = section.lines().collect();
    lines.remove(0);
    lines.pop();
    lines.join("\n").trim().to_string()
}

fn wrap(body: &str, newline: &str) -> String {
    format!("{BEGIN}{newline}{body}{newline}{END}{newline}").replace('\n', newline)
}

/// The full file after putting `body` into its managed section.
fn merge(existing: Option<&str>, body: &str) -> Result<String, String> {
    let Some(text) = existing else {
        return Ok(wrap(body, "\n"));
    };
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let section = wrap(&body.replace("\r\n", "\n"), "\n");
    let section = if newline == "\n" {
        section
    } else {
        section.replace('\n', newline)
    };
    Ok(match section_range(text)? {
        Some((b, e)) => format!("{}{section}{}", &text[..b], &text[e..]),
        None if text.trim().is_empty() => section,
        None => {
            let sep = if text.ends_with(newline) {
                newline.to_string()
            } else {
                format!("{newline}{newline}")
            };
            format!("{text}{sep}{section}")
        }
    })
}

fn record<'a>(records: &'a [Written], project: &str, path: &str) -> Option<&'a Written> {
    records
        .iter()
        .find(|r| r.project == project && r.path == path)
}

/// Claude Code reads AGENTS.md only while no CLAUDE.md-family file exists, so
/// creating the first one would silently stop it loading. Import it instead.
fn keeps_agents_md(root: &Path) -> bool {
    root.join("AGENTS.md").is_file()
        && !["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"]
            .iter()
            .any(|f| root.join(f).exists())
}

/// Bare `@word` imports that resolve to a file in the project, which Claude
/// Code would load into every session.
fn bare_imports(root: &Path, content: &str) -> Vec<String> {
    import_tokens(content)
        .into_iter()
        .filter(|t| !t.contains(['/', '.']) && root.join(t).is_file())
        .collect()
}

/// What writing `file` into `root` would do.
pub fn plan_file(root: &Path, project: &str, file: &OutFile, records: &[Written]) -> PlanItem {
    let full = root.join(&file.path);
    let content = normalize(&file.content);
    let item = |status, old: Option<String>, new: String, note: Option<String>| PlanItem {
        path: file.path.clone(),
        status,
        personal: file.personal,
        old_hash: old.as_deref().map(hash),
        old,
        new,
        note,
    };
    let existing = match read(&full) {
        Ok(text) => text,
        Err(reason) => return item(Status::Blocked(reason), None, String::new(), None),
    };
    if file.personal && existing.is_some() && ops::is_tracked(&full) {
        return item(
            Status::Blocked(crate::tr!(
                "git에 올라간 파일이라 '나만'으로 쓸 수 없어요",
                "This file is tracked by git, so it can't be written as 'Just me'"
            )),
            existing,
            String::new(),
            None,
        );
    }
    let last = record(records, project, &file.path);

    if file.target == Target::Hooks {
        let previous = last.and_then(|r| r.hooks.as_ref());
        return match merge_hooks(existing.as_deref(), previous, &file.content) {
            Err(reason) => item(Status::Blocked(reason), existing, String::new(), None),
            Ok(None) => {
                let text = existing.unwrap_or_default();
                item(Status::Unchanged, Some(text.clone()), text, None)
            }
            Ok(Some(new)) => item(
                Status::Commands,
                existing,
                new,
                Some(crate::tr!(
                    "Claude Code가 이 명령을 직접 실행해요. 내용을 확인해 주세요",
                    "Claude Code runs these commands by itself. Check them first"
                )),
            ),
        };
    }

    if file.section {
        let mut notes = Vec::new();
        let imports = bare_imports(root, &content);
        if !imports.is_empty() {
            notes.push(crate::tr!(
                "@{}는 Claude Code가 그 파일을 매번 불러와요. 의도한 게 아니면 백틱으로 감싸 주세요",
                "Claude Code loads @{} in every session. If that's not intended, wrap it in backticks",
                imports.join(", @")
            ));
        }
        let mut new = match merge(existing.as_deref(), &content) {
            Ok(new) => new,
            Err(reason) => return item(Status::Blocked(reason), existing, String::new(), None),
        };
        let Some(text) = existing else {
            if keeps_agents_md(root) {
                new = format!("@AGENTS.md\n\n{new}");
                notes.push(crate::tr!(
                    "AGENTS.md가 계속 읽히도록 맨 위에 @AGENTS.md를 넣어요",
                    "Adds @AGENTS.md at the top so AGENTS.md keeps being read"
                ));
            }
            return item(Status::New, None, new, join_notes(notes));
        };
        let status = match section_range(&text) {
            Ok(Some((b, e))) => {
                let current = section_body(&normalize(&text[b..e]));
                if current == content.trim() {
                    Status::Unchanged
                } else if last.is_some_and(|r| r.hash == hash(&current)) {
                    Status::Update
                } else {
                    Status::Modified
                }
            }
            // Appending a new section leaves the user's text alone.
            _ => Status::Update,
        };
        return item(status, Some(text), new, join_notes(notes));
    }

    let Some(text) = existing else {
        return item(Status::New, None, content, None);
    };
    let current = normalize(&text);
    let status = if current == content {
        Status::Unchanged
    } else {
        match last {
            Some(r) if r.hash == hash(&current) => Status::Update,
            Some(_) => Status::Modified,
            None => Status::Exists,
        }
    };
    item(status, Some(text), content, None)
}

/// The settings file with `ours` in place of `previous`, or `None` when
/// nothing would change. Other keys and the user's own hooks are kept.
fn merge_hooks(
    existing: Option<&str>,
    previous: Option<&serde_json::Value>,
    ours: &str,
) -> Result<Option<String>, String> {
    use serde_json::{Map, Value};
    let not_json = || {
        crate::tr!(
            "설정 파일이 올바른 JSON 객체가 아니라 건드리지 않았어요",
            "Left untouched: the settings file isn't a valid JSON object"
        )
    };
    let base: Value = match existing {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| not_json())?
        }
        _ => Value::Object(Map::new()),
    };
    let Value::Object(mut settings) = base.clone() else {
        return Err(not_json());
    };
    let mut hooks = match settings.remove("hooks") {
        None => Map::new(),
        Some(Value::Object(h)) => h,
        Some(_) => return Err(not_json()),
    };

    // Take out what we wrote last time; the user's own entries stay.
    let entries = |v: Option<&Value>| -> Vec<(String, Option<String>, Value)> {
        let mut out = Vec::new();
        let Some(events) = v.and_then(|v| v.get("hooks")).and_then(Value::as_object) else {
            return out;
        };
        for (event, groups) in events {
            for group in groups.as_array().into_iter().flatten() {
                let matcher = group
                    .get("matcher")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                for hook in group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    out.push((event.clone(), matcher.clone(), hook.clone()));
                }
            }
        }
        out
    };
    for (event, matcher, hook) in entries(previous) {
        let Some(groups) = hooks.get_mut(&event).and_then(Value::as_array_mut) else {
            continue;
        };
        for group in groups.iter_mut() {
            if group.get("matcher").and_then(Value::as_str) != matcher.as_deref() {
                continue;
            }
            if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                list.retain(|h| h != &hook);
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|l| !l.is_empty())
        });
        if groups.is_empty() {
            hooks.remove(&event);
        }
    }
    let ours: Value = serde_json::from_str(ours).map_err(|e| e.to_string())?;
    for (event, matcher, hook) in entries(Some(&ours)) {
        if let Some(command) = hook.get("command").and_then(Value::as_str) {
            add_hook(&mut hooks, &event, matcher.as_deref(), command);
        }
    }
    if !hooks.is_empty() {
        settings.insert("hooks".into(), Value::Object(hooks));
    }
    let merged = Value::Object(settings);
    if existing.is_some() && merged == base {
        return Ok(None);
    }
    let text = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())? + "\n";
    Ok(Some(match existing {
        Some(old) if old.contains("\r\n") => text.replace('\n', "\r\n"),
        _ => text,
    }))
}

fn join_notes(notes: Vec<String>) -> Option<String> {
    (!notes.is_empty()).then(|| notes.join(" · "))
}

/// Writes one planned file and returns the record to keep. Git excludes are
/// synced separately (`sync_exclude`), also for files that did not change.
pub fn write_file(
    root: &Path,
    project: &str,
    file: &OutFile,
    item: &PlanItem,
) -> ops::Result<Written> {
    ops::write_atomic(&root.join(&file.path), &item.new)?;
    if file.target == Target::Hooks {
        return Ok(Written {
            project: project.to_string(),
            path: file.path.clone(),
            hash: hash(&file.content),
            hooks: serde_json::from_str(&file.content).ok(),
        });
    }
    let hash = if file.section {
        hash(normalize(&file.content).trim())
    } else {
        hash(&item.new)
    };
    Ok(Written {
        project: project.to_string(),
        path: file.path.clone(),
        hash,
        hooks: None,
    })
}

/// Keeps `.git/info/exclude` in line with the output's scope: personal files
/// are excluded, team files are not (only our own tagged lines are removed).
pub fn sync_exclude(root: &Path, file: &OutFile) -> ops::Result<()> {
    let full = root.join(&file.path);
    if file.personal {
        ops::exclude_add(&full, false)
    } else {
        ops::exclude_remove(&full, false)
    }
}

#[cfg(test)]
mod tests {
    use super::super::compile::Target;
    use super::*;

    fn file(path: &str, content: &str, section: bool) -> OutFile {
        OutFile {
            path: path.into(),
            content: content.into(),
            section,
            personal: false,
            target: if section {
                Target::ClaudeMd
            } else {
                Target::Rules
            },
            node: "o".into(),
            blocks: vec![],
            lines: content.lines().count(),
        }
    }

    fn write(root: &Path, f: &OutFile, records: &mut Vec<Written>) -> PlanItem {
        let item = plan_file(root, "p", f, records);
        let w = write_file(root, "p", f, &item).unwrap();
        sync_exclude(root, f).unwrap();
        records.retain(|r| r.path != w.path);
        records.push(w);
        item
    }

    #[test]
    fn section_is_added_replaced_and_keeps_user_text() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("CLAUDE.md"), "# My project\nKeep me.").unwrap();
        let mut records = Vec::new();

        let f = file("CLAUDE.md", "## A", true);
        assert_eq!(write(root, &f, &mut records).status, Status::Update);
        let text = fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        assert_eq!(
            text,
            format!("# My project\nKeep me.\n\n{BEGIN}\n## A\n{END}\n")
        );

        // User adds text after the section; a new body replaces only the section.
        fs::write(root.join("CLAUDE.md"), format!("{text}\nAfter.\n")).unwrap();
        let f = file("CLAUDE.md", "## B", true);
        assert_eq!(plan_file(root, "p", &f, &records).status, Status::Update);
        write(root, &f, &mut records);
        assert_eq!(
            fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
            format!("# My project\nKeep me.\n\n{BEGIN}\n## B\n{END}\n\nAfter.\n")
        );
        assert_eq!(plan_file(root, "p", &f, &records).status, Status::Unchanged);
    }

    #[test]
    fn edits_inside_the_section_need_confirmation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut records = Vec::new();
        write(root, &file("CLAUDE.md", "## A", true), &mut records);
        let edited = fs::read_to_string(root.join("CLAUDE.md"))
            .unwrap()
            .replace("## A", "## A (mine)");
        fs::write(root.join("CLAUDE.md"), edited).unwrap();
        assert_eq!(
            plan_file(root, "p", &file("CLAUDE.md", "## C", true), &records).status,
            Status::Modified
        );
    }

    #[test]
    fn broken_markers_block_the_write() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), format!("{BEGIN}\nno end\n")).unwrap();
        let item = plan_file(tmp.path(), "p", &file("CLAUDE.md", "x", true), &[]);
        assert!(matches!(item.status, Status::Blocked(_)));
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "Top\r\n").unwrap();
        let item = plan_file(tmp.path(), "p", &file("CLAUDE.md", "a\nb", true), &[]);
        assert_eq!(
            item.new,
            format!("Top\r\n\r\n{BEGIN}\r\na\r\nb\r\n{END}\r\n")
        );
    }

    #[test]
    fn whole_files_are_protected_unless_we_wrote_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut records = Vec::new();
        let f = file(".claude/rules/ts.md", "# TS\n", false);
        assert_eq!(write(root, &f, &mut records).status, Status::New);

        let f2 = file(".claude/rules/ts.md", "# TS 2\n", false);
        assert_eq!(plan_file(root, "p", &f2, &records).status, Status::Update);

        fs::write(root.join(".claude/rules/ts.md"), "# edited\n").unwrap();
        assert_eq!(plan_file(root, "p", &f2, &records).status, Status::Modified);

        let other = file(".claude/rules/go.md", "# Go\n", false);
        fs::write(root.join(".claude/rules/go.md"), "# theirs\n").unwrap();
        let item = plan_file(root, "p", &other, &records);
        assert_eq!(item.status, Status::Exists);
        assert!(item.status.needs_confirm());
    }

    #[test]
    fn crlf_and_bom_do_not_look_like_user_edits() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut records = Vec::new();
        // Content compiled on a CRLF checkout.
        let f = file("CLAUDE.md", "## A\r\n- one", true);
        write(root, &f, &mut records);
        assert_eq!(plan_file(root, "p", &f, &records).status, Status::Unchanged);
        // An editor re-saves the file with a BOM and CRLF.
        let text = fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        fs::write(
            root.join("CLAUDE.md"),
            format!("\u{feff}{}", text.replace('\n', "\r\n")),
        )
        .unwrap();
        assert_eq!(plan_file(root, "p", &f, &records).status, Status::Unchanged);
        let f2 = file("CLAUDE.md", "## B", true);
        assert_eq!(plan_file(root, "p", &f2, &records).status, Status::Update);

        let r = file(".claude/rules/a.md", "# A\n", false);
        write(root, &r, &mut records);
        fs::write(root.join(".claude/rules/a.md"), "# A\r\n").unwrap();
        assert_eq!(plan_file(root, "p", &r, &records).status, Status::Unchanged);
    }

    #[test]
    fn first_claude_md_keeps_agents_md_loading() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("AGENTS.md"), "# Shared").unwrap();
        let item = plan_file(root, "p", &file("CLAUDE.local.md", "## Me", true), &[]);
        assert!(item.new.starts_with("@AGENTS.md\n\n"), "{}", item.new);
        assert!(item.note.is_some());
        fs::write(root.join("CLAUDE.md"), "# Team").unwrap();
        let item = plan_file(root, "p", &file("CLAUDE.local.md", "## Me", true), &[]);
        assert!(!item.new.contains("@AGENTS.md"));
    }

    #[test]
    fn notes_bare_imports_of_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:").unwrap();
        let item = plan_file(
            tmp.path(),
            "p",
            &file("CLAUDE.md", "Use @Makefile and @nobody", true),
            &[],
        );
        let note = item.note.unwrap();
        assert!(
            note.contains("@Makefile") && !note.contains("nobody"),
            "{note}"
        );
    }

    #[test]
    fn switching_scope_moves_the_exclude_line() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git").join("info")).unwrap();
        let mut f = file(".claude/rules/a.md", "# A\n", false);
        f.personal = true;
        write(root, &f, &mut Vec::new());
        let exclude = || fs::read_to_string(root.join(".git/info/exclude")).unwrap_or_default();
        assert!(exclude().contains("/.claude/rules/a.md"));
        f.personal = false;
        sync_exclude(root, &f).unwrap();
        assert!(!exclude().contains("/.claude/rules/a.md"));
    }

    #[test]
    fn hooks_merge_into_settings_and_replace_only_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".claude")).unwrap();
        let path = root.join(".claude/settings.json");
        fs::write(
            &path,
            r#"{"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mine"}]}]}}"#,
        )
        .unwrap();
        let hooks = |cmd: &str| OutFile {
            target: Target::Hooks,
            ..file(
                ".claude/settings.json",
                &format!(
                    r#"{{"hooks": {{"Stop": [{{"hooks": [{{"type": "command", "command": "{cmd}"}}]}}]}}}}"#
                ),
                false,
            )
        };
        let mut records = Vec::new();
        let item = write(root, &hooks("ours 1"), &mut records);
        assert_eq!(item.status, Status::Commands);
        assert!(item.status.needs_confirm());
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"],
            serde_json::json!([{"type": "command", "command": "mine"}, {"type": "command", "command": "ours 1"}])
        );
        assert_eq!(
            plan_file(root, "p", &hooks("ours 1"), &records).status,
            Status::Unchanged
        );

        write(root, &hooks("ours 2"), &mut records);
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"],
            serde_json::json!([{"type": "command", "command": "mine"}, {"type": "command", "command": "ours 2"}])
        );

        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            plan_file(root, "p", &hooks("ours 3"), &records).status,
            Status::Blocked(_)
        ));
    }

    #[test]
    fn personal_files_are_git_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git").join("info")).unwrap();
        let mut f = file("CLAUDE.local.md", "## Me", true);
        f.personal = true;
        write(root, &f, &mut Vec::new());
        let exclude = fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(exclude.contains("/CLAUDE.local.md\n"), "{exclude}");
    }
}
