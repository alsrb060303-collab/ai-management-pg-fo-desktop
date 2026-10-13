//! Reads a repository's state for the work panel: the checked-out branch,
//! changed files, local branches and worktrees. All from `git` porcelain
//! output, which is stable across versions and locales.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git;

/// Changed files beyond this are counted but not listed.
const MAX_CHANGES: usize = 1000;
/// Worktrees whose changes are counted (each costs a `git status`).
const MAX_COUNTED_WORKTREES: usize = 20;
/// Commits shown in the history.
const LOG_LIMIT: usize = 40;
/// Folder Claude Code (and this app) creates worktrees in.
pub const WORKTREES_DIR: &str = ".claude/worktrees";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    /// Short commit; `None` before the first commit.
    pub commit: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Relative to the worktree root, `/`-separated.
    pub path: String,
    /// Source of a rename or copy.
    pub orig_path: Option<String>,
    pub kind: ChangeKind,
    /// Some of the change is staged.
    pub staged: bool,
    /// Some of the change is not staged (always for untracked files).
    pub unstaged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream branch was deleted on the remote.
    pub gone: bool,
    pub commit: String,
    pub subject: String,
    /// Committer time, seconds since the epoch.
    pub time: i64,
    /// Worktree the branch is checked out in, if any.
    pub worktree: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub is_main: bool,
    /// The worktree of the open project.
    pub is_current: bool,
    /// Under `.claude/worktrees` (made by Claude Code or this app).
    pub managed: bool,
    pub locked: bool,
    /// Its folder is gone; `git worktree prune` would drop it.
    pub prunable: bool,
    /// Changed files; `None` when not counted.
    pub changes: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    /// Short name: `main`, `origin/main`, `v1.0`.
    pub name: String,
    pub kind: RefKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    /// Committer time, seconds since the epoch.
    pub time: i64,
    /// Has more than one parent.
    pub merge: bool,
    /// On the remote (the upstream, or any remote branch without one).
    pub pushed: bool,
    pub refs: Vec<RefLabel>,
}

/// How the checked-out branch relates to the repository's main branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    /// `main`, or `origin/main` when only the remote one exists.
    pub name: String,
    /// Commits on this branch that the base lacks.
    pub ahead: u32,
    /// Commits on the base that this branch lacks.
    pub behind: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// Root of the open project's worktree.
    pub root: String,
    /// Root of the main worktree.
    pub main_root: String,
    pub head: Head,
    /// "rebase", "merge", "cherry-pick", "revert" or "bisect" in progress.
    pub operation: Option<String>,
    pub changes: Vec<Change>,
    /// Changed files in total (more than `changes` when capped).
    pub change_count: u32,
    /// Any remote is configured (the setup guide offers to add one).
    pub has_remote: bool,
    pub branches: Vec<Branch>,
    pub worktrees: Vec<Worktree>,
    /// The checked-out branch's recent history (first parents only).
    pub log: Vec<Commit>,
    /// Commits not on the remote yet.
    pub unpushed: u32,
    /// `None` on the main branch itself, or when there is none.
    pub base: Option<Base>,
}

/// Compares paths from git (`C:/a/b`) with OS paths (`\\?\C:\a\b`).
pub fn path_key(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = p.strip_prefix("//?/").unwrap_or(&p).trim_end_matches('/');
    if cfg!(windows) {
        p.to_lowercase()
    } else {
        p.to_string()
    }
}

/// Splits `-z` output into fields.
fn fields(raw: &[u8]) -> Vec<String> {
    raw.split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect()
}

fn strip_heads(name: &str) -> Option<String> {
    name.strip_prefix("refs/heads/")
        .filter(|b| !b.is_empty())
        .map(str::to_string)
}

/// Parses `git status --porcelain=v2 -z --branch`.
pub fn parse_status(raw: &[u8]) -> (Head, Vec<Change>, u32) {
    let mut head = Head::default();
    let mut changes = Vec::new();
    let mut count = 0u32;
    let mut items = fields(raw).into_iter();
    while let Some(item) = items.next() {
        if let Some(header) = item.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => {
                    head.commit = Some(value.chars().take(7).collect());
                }
                "branch.head" if value != "(detached)" => head.branch = Some(value.to_string()),
                "branch.upstream" => head.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            head.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            head.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let change = match item.as_bytes().first() {
            // 1 XY sub mH mI mW hH hI path
            Some(b'1') => item.splitn(9, ' ').collect::<Vec<_>>().get(8).map(|path| {
                let xy = &item[2..4];
                tracked(path, None, xy)
            }),
            // 2 XY sub mH mI mW hH hI Xscore path, then the source as its own field
            Some(b'2') => {
                let orig = items.next();
                item.splitn(10, ' ').collect::<Vec<_>>().get(9).map(|path| {
                    let xy = &item[2..4];
                    tracked(path, orig, xy)
                })
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            Some(b'u') => item
                .splitn(11, ' ')
                .collect::<Vec<_>>()
                .get(10)
                .map(|path| Change {
                    path: path.to_string(),
                    orig_path: None,
                    kind: ChangeKind::Conflicted,
                    staged: false,
                    unstaged: true,
                }),
            Some(b'?') => item.get(2..).map(|path| Change {
                path: path.to_string(),
                orig_path: None,
                kind: ChangeKind::Untracked,
                staged: false,
                unstaged: true,
            }),
            _ => None,
        };
        if let Some(change) = change {
            count += 1;
            if changes.len() < MAX_CHANGES {
                changes.push(change);
            }
        }
    }
    (head, changes, count)
}

fn tracked(path: &str, orig: Option<String>, xy: &str) -> Change {
    let mut chars = xy.chars();
    let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
    // A file deleted from the worktree is gone whatever was staged; otherwise
    // the staged side names the change unless only the worktree changed.
    let kind = match if y == 'D' {
        y
    } else if x != '.' {
        x
    } else {
        y
    } {
        'A' => ChangeKind::Added,
        'D' => ChangeKind::Deleted,
        'R' => ChangeKind::Renamed,
        'C' => ChangeKind::Copied,
        'T' => ChangeKind::TypeChanged,
        _ => ChangeKind::Modified,
    };
    Change {
        path: path.to_string(),
        orig_path: orig.filter(|o| !o.is_empty()),
        kind,
        staged: x != '.',
        unstaged: y != '.',
    }
}

/// Field separator for `for-each-ref` output.
const SEP: &str = "\x1f";

const BRANCH_FORMAT: &str = "--format=%(refname)\x1f%(upstream:short)\x1f%(upstream:track,nobracket)\x1f%(objectname:short)\x1f%(committerdate:unix)\x1f%(worktreepath)\x1f%(contents:subject)";

fn parse_branches(out: &str, current: Option<&str>) -> Vec<Branch> {
    let mut branches: Vec<Branch> = out
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(SEP).collect();
            let name = strip_heads(f.first()?)?;
            let track = f.get(2).copied().unwrap_or("");
            let mut ahead = 0;
            let mut behind = 0;
            for part in track.split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    behind = n.parse().unwrap_or(0);
                }
            }
            Some(Branch {
                current: current == Some(name.as_str()),
                name,
                upstream: f.get(1).filter(|u| !u.is_empty()).map(|u| u.to_string()),
                ahead,
                behind,
                gone: track == "gone",
                commit: f.get(3).unwrap_or(&"").to_string(),
                time: f.get(4).and_then(|t| t.parse().ok()).unwrap_or(0),
                worktree: f.get(5).filter(|w| !w.is_empty()).map(|w| w.to_string()),
                // Free text last: a separator inside it cannot shift other fields.
                subject: f[6.min(f.len())..].join(SEP),
            })
        })
        .collect();
    // Current first, then the most recently committed.
    branches.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then(b.time.cmp(&a.time))
            .then_with(|| a.name.cmp(&b.name))
    });
    branches
}

/// Parses `git worktree list --porcelain -z`.
fn parse_worktrees(raw: &[u8], current_root: &str, main_root: &str) -> Vec<Worktree> {
    let managed_dir = format!("{}/{WORKTREES_DIR}/", path_key(main_root));
    let mut list = Vec::new();
    let mut entry: Option<Worktree> = None;
    for field in fields(raw) {
        if field.is_empty() {
            list.extend(entry.take());
            continue;
        }
        let (key, value) = field.split_once(' ').unwrap_or((&field, ""));
        if key == "worktree" {
            list.extend(entry.take());
            let k = path_key(value);
            entry = Some(Worktree {
                path: value.to_string(),
                branch: None,
                commit: None,
                is_main: list.is_empty(),
                is_current: k == path_key(current_root),
                managed: k.starts_with(&managed_dir),
                locked: false,
                prunable: false,
                changes: None,
            });
            continue;
        }
        let Some(wt) = entry.as_mut() else { continue };
        match key {
            "HEAD" => wt.commit = Some(value.chars().take(7).collect()),
            "branch" => wt.branch = strip_heads(value),
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            _ => {}
        }
    }
    list.extend(entry);
    // A repository's first commit is all zeros before it exists.
    for wt in &mut list {
        if wt.commit.as_deref() == Some("0000000") {
            wt.commit = None;
        }
    }
    list
}

/// The operation a worktree is in the middle of, from its git dir.
pub(super) fn operation(git_dir: &Path) -> Option<String> {
    [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ]
    .iter()
    .find(|(file, _)| git_dir.join(file).exists())
    .map(|(_, name)| name.to_string())
}

const LOG_FORMAT: &str = "--format=%H%x1f%h%x1f%P%x1f%an%x1f%ct%x1f%D%x1f%s%x1e";

/// `HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1`.
fn parse_refs(decoration: &str) -> Vec<RefLabel> {
    decoration
        .split(", ")
        .filter_map(|d| {
            let d = d.strip_prefix("HEAD -> ").unwrap_or(d);
            let d = d.strip_prefix("tag: ").unwrap_or(d);
            if let Some(name) = d.strip_prefix("refs/heads/") {
                Some(RefLabel {
                    name: name.into(),
                    kind: RefKind::Local,
                })
            } else if let Some(name) = d.strip_prefix("refs/remotes/") {
                (!name.ends_with("/HEAD")).then(|| RefLabel {
                    name: name.into(),
                    kind: RefKind::Remote,
                })
            } else {
                d.strip_prefix("refs/tags/").map(|name| RefLabel {
                    name: name.into(),
                    kind: RefKind::Tag,
                })
            }
        })
        .collect()
}

fn parse_log(out: &str, unpushed: &[&str]) -> Vec<Commit> {
    out.split('\x1e')
        .filter_map(|record| {
            let mut f = record.trim_start_matches('\n').split('\x1f');
            let hash = f.next().filter(|h| !h.is_empty())?.to_string();
            let short = f.next()?.to_string();
            let merge = f.next()?.split_whitespace().count() > 1;
            let author = f.next()?.to_string();
            let time = f.next()?.parse().unwrap_or(0);
            let refs = parse_refs(f.next()?);
            let subject = f.next().unwrap_or_default().to_string();
            Some(Commit {
                pushed: !unpushed.contains(&hash.as_str()),
                hash,
                short,
                subject,
                author,
                time,
                merge,
                refs,
            })
        })
        .collect()
}

/// What "not pushed" is measured against: the upstream, or every remote branch.
fn remote_side(head: &Head) -> Vec<&'static str> {
    if head.upstream.is_some() {
        vec!["@{upstream}"]
    } else {
        vec!["--remotes"]
    }
}

/// The branch the repository treats as main: `origin/HEAD`, else `main`/`master`.
fn base(root: &Path, head: &Head) -> Option<Base> {
    let current = head.branch.as_deref()?;
    let name = git::run(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .and_then(|r| r.strip_prefix("origin/").map(str::to_string))
    .or_else(|| {
        ["main", "master"]
            .into_iter()
            .map(str::to_string)
            .find(|b| {
                git::run(root, &["rev-parse", "--verify", "--quiet", b]).is_ok()
                    || git::run(
                        root,
                        &["rev-parse", "--verify", "--quiet", &format!("origin/{b}")],
                    )
                    .is_ok()
            })
    })?;
    if name == current {
        return None;
    }
    // The remote one is what a pull request compares against.
    let reference = [format!("origin/{name}"), name.clone()]
        .into_iter()
        .find(|r| git::run(root, &["rev-parse", "--verify", "--quiet", r]).is_ok())?;
    let counts = git::run(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{reference}"),
        ],
    )
    .ok()?;
    let mut n = counts.split_whitespace().map(|c| c.parse().unwrap_or(0));
    Some(Base {
        name: reference,
        ahead: n.next()?,
        behind: n.next()?,
    })
}

/// Number of changed files in another worktree.
fn count_changes(path: &str) -> Option<u32> {
    let raw = git::run_raw(Path::new(path), &["status", "--porcelain=v2", "-z"]).ok()?;
    Some(parse_status(&raw).2)
}

/// The repository root of `project`, or `None` outside git.
pub fn root(project: &Path) -> git::Result<Option<PathBuf>> {
    if !project.is_dir() {
        return Err(crate::tr!(
            "프로젝트 폴더를 찾을 수 없어요",
            "Couldn't find the project folder"
        ));
    }
    match git::run(project, &["rev-parse", "--show-toplevel"]) {
        Ok(root) => Ok(Some(PathBuf::from(root))),
        Err(e) if git::inside_git(project) => Err(crate::tr!(
            "git 저장소를 읽지 못했어요: {e}",
            "Couldn't read the git repository: {e}"
        )),
        Err(_) => Ok(None),
    }
}

/// Everything the work panel shows. `count_worktrees` also counts the changes
/// in other worktrees (one `git status` each), so polls can skip it.
pub fn overview(root: &Path, count_worktrees: bool) -> git::Result<Overview> {
    let raw = git::run_raw(root, &["status", "--porcelain=v2", "-z", "--branch"])?;
    let (head, changes, change_count) = parse_status(&raw);

    let git_dir = git::run(root, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from)?;
    let operation = operation(&git_dir);

    let refs = git::run(root, &["for-each-ref", BRANCH_FORMAT, "refs/heads"])?;
    let branches = parse_branches(&refs, head.branch.as_deref());

    let has_remote = git::run(root, &["remote"]).is_ok_and(|out| !out.trim().is_empty());

    let root_str = root.to_string_lossy().into_owned();
    // `-z` needs git 2.36; older ones get newline-separated output, which
    // only differs for paths containing newlines.
    let raw = git::run_raw(root, &["worktree", "list", "--porcelain", "-z"]).or_else(|_| {
        git::run_raw(root, &["worktree", "list", "--porcelain"]).map(|out| {
            out.into_iter()
                .map(|b| if b == b'\n' { 0 } else { b })
                .collect()
        })
    })?;
    // The first entry is always the main worktree.
    let first = fields(&raw)
        .into_iter()
        .find_map(|f| f.strip_prefix("worktree ").map(str::to_string))
        .unwrap_or_else(|| root_str.clone());
    let mut worktrees = parse_worktrees(&raw, &root_str, &first);
    let mut counted = 0;
    for wt in &mut worktrees {
        if wt.is_current {
            wt.changes = Some(change_count);
        } else if count_worktrees && !wt.prunable && counted < MAX_COUNTED_WORKTREES {
            counted += 1;
            wt.changes = count_changes(&wt.path);
        }
    }

    let (log, unpushed, base) = if head.commit.is_some() {
        let mut args = vec!["rev-list", "HEAD", "--not"];
        args.extend(remote_side(&head));
        let out = if has_remote {
            git::run(root, &args).unwrap_or_default()
        } else {
            String::new()
        };
        let unpushed: Vec<&str> = out.lines().collect();
        let count = unpushed.len() as u32;
        let limit = format!("-n{LOG_LIMIT}");
        let raw = git::run(
            root,
            &[
                "log",
                &limit,
                "--first-parent",
                "--decorate=full",
                LOG_FORMAT,
                "HEAD",
            ],
        )
        .unwrap_or_default();
        (parse_log(&raw, &unpushed), count, base(root, &head))
    } else {
        (Vec::new(), 0, None)
    };

    Ok(Overview {
        root: root_str,
        main_root: first,
        head,
        operation,
        changes,
        change_count,
        has_remote,
        branches,
        worktrees,
        log,
        unpushed,
        base,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_v2_is_parsed() {
        let raw = b"# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb src/a.rs\0\
1 A. N... 000000 100644 100644 000 bbb new file.txt\0\
2 R. N... 100644 100644 100644 aaa bbb R100 b.rs\0old b.rs\0\
u UU N... 100644 100644 100644 100644 a b c conflict.rs\0\
1 MD N... 100644 100644 000000 aaa bbb gone.rs\0\
? notes/\0";
        let (head, changes, count) = parse_status(raw);
        assert_eq!(
            head,
            Head {
                branch: Some("main".into()),
                commit: Some("1234567".into()),
                upstream: Some("origin/main".into()),
                ahead: 2,
                behind: 1,
            }
        );
        assert_eq!(count, 6);
        let summary: Vec<_> = changes
            .iter()
            .map(|c| {
                (
                    c.path.as_str(),
                    c.orig_path.as_deref(),
                    &c.kind,
                    c.staged,
                    c.unstaged,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("src/a.rs", None, &ChangeKind::Modified, false, true),
                ("new file.txt", None, &ChangeKind::Added, true, false),
                ("b.rs", Some("old b.rs"), &ChangeKind::Renamed, true, false),
                ("conflict.rs", None, &ChangeKind::Conflicted, false, true),
                ("gone.rs", None, &ChangeKind::Deleted, true, true),
                ("notes/", None, &ChangeKind::Untracked, false, true),
            ]
        );

        let (head, _, _) = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(head, Head::default());
    }

    #[test]
    fn worktree_list_is_parsed() {
        let raw = b"worktree /r\0HEAD 1111111111\0branch refs/heads/main\0\0\
worktree /r/.claude/worktrees/x\0HEAD 2222222222\0branch refs/heads/worktree-x\0locked\0\0\
worktree /gone\0HEAD 3333333333\0detached\0prunable gitdir file points to non-existent location\0\0";
        let list = parse_worktrees(raw, "/r/.claude/worktrees/x", "/r");
        let summary: Vec<_> = list
            .iter()
            .map(|w| {
                (
                    w.path.as_str(),
                    w.branch.as_deref(),
                    w.is_main,
                    w.is_current,
                    w.managed,
                    w.locked,
                    w.prunable,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("/r", Some("main"), true, false, false, false, false),
                (
                    "/r/.claude/worktrees/x",
                    Some("worktree-x"),
                    false,
                    true,
                    true,
                    true,
                    false
                ),
                ("/gone", None, false, false, false, false, true),
            ]
        );
    }

    #[test]
    fn log_is_parsed_with_refs_and_pushed_state() {
        let raw = "aaaa\x1fa\x1fp1\x1fMe\x1f10\x1fHEAD -> refs/heads/feat, refs/remotes/origin/HEAD, tag: refs/tags/v1\x1fAdd x\x1e\n\
bbbb\x1fb\x1fp1 p2\x1fYou\x1f5\x1frefs/remotes/origin/main\x1fMerge y\x1e\n";
        let log = parse_log(raw, &["aaaa"]);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].subject, "Add x");
        assert!(!log[0].pushed && !log[0].merge);
        assert_eq!(
            log[0].refs,
            [
                RefLabel {
                    name: "feat".into(),
                    kind: RefKind::Local
                },
                RefLabel {
                    name: "v1".into(),
                    kind: RefKind::Tag
                },
            ]
        );
        assert!(log[1].pushed && log[1].merge);
        assert_eq!(log[1].refs[0].kind, RefKind::Remote);
    }

    #[test]
    fn windows_paths_compare_alike() {
        assert_eq!(path_key(r"\\?\C:\a\b\"), path_key("C:/a/b"));
    }
}
