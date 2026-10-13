//! Resolving merge conflicts from the work panel: a conflicted file split
//! into plain text and `<<<<<<<`/`=======`/`>>>>>>>` blocks, writing the
//! chosen result and marking it resolved, and continuing or aborting the
//! merge, rebase, cherry-pick or revert.

use std::path::{Component, Path};

use serde::Serialize;

use super::read;
use crate::git;

/// Files beyond this are left to an editor.
const MAX_FILE: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Segment {
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Conflict {
        /// What follows `<<<<<<<`: `HEAD`, or a branch.
        ours_label: String,
        ours: String,
        /// The common ancestor, with `merge.conflictStyle = diff3`.
        base: Option<String>,
        /// What follows `>>>>>>>`: a branch, or `abc1234 (subject)`.
        theirs_label: String,
        theirs: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub segments: Vec<Segment>,
    /// The file has conflict markers. Without them (e.g. deleted on one
    /// side), it can only be kept as it is or deleted.
    pub markers: bool,
    /// The file is missing from the working tree (deleted on one side).
    pub missing: bool,
    /// Binary or too large to show.
    pub unreadable: bool,
}

fn marker<'a>(line: &'a str, mark: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(mark)?;
    let rest = rest.trim_end_matches(['\n', '\r']);
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix(' ')
    }
}

/// Splits `text` at conflict markers. Line endings are kept, so joining the
/// chosen parts gives back the file byte for byte. A block that never closes
/// stays plain text.
pub fn parse(text: &str) -> Vec<Segment> {
    enum State {
        Text,
        Ours,
        Base,
        Theirs,
    }
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut raw = String::new();
    let (mut ours, mut base, mut theirs) = (String::new(), None::<String>, String::new());
    let mut ours_label = String::new();
    let mut state = State::Text;
    for line in text.split_inclusive('\n') {
        match state {
            State::Text => {
                if let Some(label) = marker(line, "<<<<<<<") {
                    ours_label = label.to_string();
                    raw = line.to_string();
                    state = State::Ours;
                } else {
                    plain.push_str(line);
                }
            }
            State::Ours | State::Base => {
                raw.push_str(line);
                if marker(line, "|||||||").is_some() && matches!(state, State::Ours) {
                    base = Some(String::new());
                    state = State::Base;
                } else if marker(line, "=======") == Some("") {
                    state = State::Theirs;
                } else if let State::Base = state {
                    base.get_or_insert_with(String::new).push_str(line);
                } else {
                    ours.push_str(line);
                }
            }
            State::Theirs => {
                raw.push_str(line);
                if let Some(label) = marker(line, ">>>>>>>") {
                    if !plain.is_empty() {
                        out.push(Segment::Text {
                            text: std::mem::take(&mut plain),
                        });
                    }
                    out.push(Segment::Conflict {
                        ours_label: std::mem::take(&mut ours_label),
                        ours: std::mem::take(&mut ours),
                        base: base.take(),
                        theirs_label: label.to_string(),
                        theirs: std::mem::take(&mut theirs),
                    });
                    raw.clear();
                    state = State::Text;
                } else {
                    theirs.push_str(line);
                }
            }
        }
    }
    // An unfinished block is not a conflict: keep it as written.
    plain.push_str(&raw);
    if !plain.is_empty() {
        out.push(Segment::Text { text: plain });
    }
    out
}

fn checked(root: &Path, path: &str) -> git::Result<std::path::PathBuf> {
    let ok = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !ok {
        return Err(crate::tr!("잘못된 경로예요", "Invalid path"));
    }
    Ok(root.join(path))
}

pub fn read(root: &Path, path: &str) -> git::Result<ConflictFile> {
    let file = checked(root, path)?;
    let Ok(meta) = std::fs::symlink_metadata(&file) else {
        return Ok(ConflictFile {
            missing: true,
            ..ConflictFile::default()
        });
    };
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Ok(ConflictFile {
            unreadable: true,
            ..ConflictFile::default()
        });
    }
    let bytes = std::fs::read(&file)
        .map_err(|e| crate::tr!("파일을 읽을 수 없어요: {e}", "Couldn't read the file: {e}"))?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(ConflictFile {
            unreadable: true,
            ..ConflictFile::default()
        });
    };
    let segments = parse(&text);
    let markers = segments
        .iter()
        .any(|s| matches!(s, Segment::Conflict { .. }));
    Ok(ConflictFile {
        segments,
        markers,
        ..ConflictFile::default()
    })
}

/// Writes the resolved `text` (or deletes the file when `None`) and marks
/// it resolved. Refuses text that still has conflict markers.
pub fn resolve(root: &Path, path: &str, text: Option<&str>) -> git::Result<()> {
    let file = checked(root, path)?;
    match text {
        Some(text) => {
            if parse(text)
                .iter()
                .any(|s| matches!(s, Segment::Conflict { .. }))
            {
                return Err(crate::tr!(
                    "아직 고르지 않은 충돌이 있어요",
                    "Some conflicts are still unresolved"
                ));
            }
            std::fs::write(&file, text).map_err(|e| {
                crate::tr!(
                    "파일을 저장하지 못했어요: {e}",
                    "Couldn't save the file: {e}"
                )
            })?;
            git::run(root, &["--literal-pathspecs", "add", "--", path]).map(drop)
        }
        None => git::run(
            root,
            &[
                "--literal-pathspecs",
                "rm",
                "-q",
                "--ignore-unmatch",
                "--",
                path,
            ],
        )
        .map(drop),
    }
}

fn current_operation(root: &Path) -> git::Result<String> {
    let git_dir = git::run(root, &["rev-parse", "--absolute-git-dir"])?;
    read::operation(Path::new(&git_dir))
        .filter(|op| op != "bisect")
        .ok_or_else(|| {
            crate::tr!(
                "진행 중인 병합·리베이스가 없어요",
                "No merge or rebase is in progress"
            )
        })
}

/// `git <op> --continue`, keeping git's own commit message (no editor).
pub fn continue_operation(root: &Path) -> git::Result<()> {
    let op = current_operation(root)?;
    git::run(root, &["-c", "core.editor=true", &op, "--continue"]).map(drop)
}

/// `git <op> --abort`: back to where the operation started.
pub fn abort_operation(root: &Path) -> git::Result<()> {
    let op = current_operation(root)?;
    git::run(root, &[&op, "--abort"]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts_are_split_and_line_endings_kept() {
        let text = "a\r\n<<<<<<< HEAD\r\nmine\r\n=======\r\ntheirs\r\n>>>>>>> feat\r\nz\n";
        let segs = parse(text);
        assert_eq!(segs.len(), 3);
        assert_eq!(
            segs[0],
            Segment::Text {
                text: "a\r\n".into()
            }
        );
        assert_eq!(
            segs[1],
            Segment::Conflict {
                ours_label: "HEAD".into(),
                ours: "mine\r\n".into(),
                base: None,
                theirs_label: "feat".into(),
                theirs: "theirs\r\n".into(),
            }
        );
        assert_eq!(segs[2], Segment::Text { text: "z\n".into() });
    }

    #[test]
    fn diff3_bases_and_unfinished_blocks() {
        let segs = parse("<<<<<<< a\nx\n||||||| base\nb\n=======\ny\n>>>>>>> c\n");
        assert!(matches!(&segs[0], Segment::Conflict { base: Some(b), .. } if b == "b\n"));
        // No closing marker: plain text, unchanged.
        let open = "<<<<<<< a\nx\n=======\ny\n";
        assert_eq!(parse(open), [Segment::Text { text: open.into() }]);
    }

    #[test]
    fn a_merge_conflict_is_resolved_and_continued() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let git = |args: &[&str]| crate::git::run(root, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(root.join("f.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "one"]);
        git(&["switch", "-q", "-c", "feat"]);
        std::fs::write(root.join("f.txt"), "feat\n").unwrap();
        git(&["commit", "-q", "-am", "feat"]);
        git(&["switch", "-q", "main"]);
        std::fs::write(root.join("f.txt"), "main\n").unwrap();
        git(&["commit", "-q", "-am", "main"]);
        assert!(crate::git::run(root, &["merge", "feat"]).is_err());

        let file = read(root, "f.txt").unwrap();
        assert!(file.markers);
        assert!(resolve(root, "f.txt", Some("<<<<<<< a\n=======\n>>>>>>> b\n")).is_err());
        resolve(root, "f.txt", Some("main\nfeat\n")).unwrap();
        continue_operation(root).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "main\nfeat\n"
        );
        assert!(current_operation(root).is_err());
        assert!(read(root, "../x").is_err());
    }
}
