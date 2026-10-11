//! Writes compiled files into a project without clobbering the user's work.
//!
//! - Section files (CLAUDE.md): only the text between our markers is ours.
//!   The rest of the file is left as is; without markers the section is
//!   appended. Claude Code strips block-level HTML comments, so the markers
//!   cost no context.
//! - Whole files (rules, skills, agents): written when new, or when the file
//!   still has the content we last wrote.
//!
//! Anything the user changed since our last write, or a file we did not write,
//! is only overwritten after the user confirms that path.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::compile::OutFile;
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
    /// Cannot be written; the reason says why.
    Blocked(String),
}

impl Status {
    pub fn needs_confirm(&self) -> bool {
        matches!(self, Status::Modified | Status::Exists)
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
}

pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn read(path: &Path) -> ops::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            format!(
                "UTF-8 텍스트가 아니라 건드리지 않았어요: {}",
                path.display()
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("읽지 못했어요: {} ({e})", path.display())),
    }
}

/// Byte range of the managed section (marker lines included, through the end
/// marker's line break), or `None` without markers. Broken markers are an error.
fn section_range(text: &str) -> Result<Option<(usize, usize)>, String> {
    let mut begin = None;
    let mut end = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with(BEGIN_PREFIX) {
            if begin.is_some() {
                return Err("관리 구역 시작 표시가 두 번 있어요".into());
            }
            begin = Some(offset);
        } else if trimmed == END {
            if begin.is_none() || end.is_some() {
                return Err("관리 구역 표시 순서가 맞지 않아요".into());
            }
            end = Some(offset + line.len());
        }
        offset += line.len();
    }
    match (begin, end) {
        (None, None) => Ok(None),
        (Some(b), Some(e)) => Ok(Some((b, e))),
        _ => Err("관리 구역 끝 표시가 없어요".into()),
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

/// What writing `file` into `root` would do.
pub fn plan_file(root: &Path, project: &str, file: &OutFile, records: &[Written]) -> PlanItem {
    let full = root.join(&file.path);
    let item = |status, old: Option<String>, new: String| PlanItem {
        path: file.path.clone(),
        status,
        personal: file.personal,
        old,
        new,
    };
    let existing = match read(&full) {
        Ok(text) => text,
        Err(reason) => return item(Status::Blocked(reason), None, String::new()),
    };
    if file.personal && existing.is_some() && ops::is_tracked(&full) {
        return item(
            Status::Blocked("git에 올라간 파일이라 '나만'으로 쓸 수 없어요".into()),
            existing,
            String::new(),
        );
    }
    let last = record(records, project, &file.path);

    if file.section {
        let new = match merge(existing.as_deref(), &file.content) {
            Ok(new) => new,
            Err(reason) => return item(Status::Blocked(reason), existing, String::new()),
        };
        let Some(text) = existing else {
            return item(Status::New, None, new);
        };
        let status = match section_range(&text) {
            Ok(Some((b, e))) => {
                let current = section_body(&text[b..e].replace("\r\n", "\n"));
                if current == file.content.trim() {
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
        return item(status, Some(text), new);
    }

    let new = file.content.clone();
    let Some(text) = existing else {
        return item(Status::New, None, new);
    };
    let status = if text == new {
        Status::Unchanged
    } else {
        match last {
            Some(r) if r.hash == hash(&text) => Status::Update,
            Some(_) => Status::Modified,
            None => Status::Exists,
        }
    };
    item(status, Some(text), new)
}

/// Writes one planned file and returns the record to keep.
pub fn write_file(
    root: &Path,
    project: &str,
    file: &OutFile,
    item: &PlanItem,
) -> ops::Result<Written> {
    let full = root.join(&file.path);
    ops::write_atomic(&full, &item.new)?;
    if file.personal {
        ops::exclude_add(&full, false)?;
    }
    let hash = if file.section {
        hash(file.content.trim())
    } else {
        hash(&item.new)
    };
    Ok(Written {
        project: project.to_string(),
        path: file.path.clone(),
        hash,
    })
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
