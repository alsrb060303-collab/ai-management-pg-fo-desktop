//! Changes the work panel makes: switching, creating and deleting branches,
//! adding and removing worktrees, and reading one file's diff. Anything that
//! can lose work is refused unless the caller passes `force` (the panel asks
//! twice first).

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::read::{self, WORKTREES_DIR};
use crate::git;

/// Diffs beyond this are cut (the panel is not an editor).
const MAX_DIFF: usize = 512 * 1024;
/// Git's empty tree, to diff a repository that has no commit yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// Unified diff text without the `diff --git` header lines.
    pub text: String,
    pub binary: bool,
    pub truncated: bool,
}

/// What deleting a branch would lose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCheck {
    /// Commits on the branch that are in neither HEAD nor its upstream.
    pub unmerged: u32,
}

fn local_branches(root: &Path) -> git::Result<Vec<String>> {
    let out = git::run(root, &["for-each-ref", "--format=%(refname)", "refs/heads"])?;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .collect())
}

fn require_branch(root: &Path, name: &str) -> git::Result<()> {
    if local_branches(root)?.iter().any(|b| b == name) {
        Ok(())
    } else {
        Err(format!("'{name}' 브랜치가 없어요"))
    }
}

/// A new branch name git accepts (`check-ref-format` also rejects a leading
/// dash, so names never read as options).
fn valid_branch_name(root: &Path, name: &str) -> git::Result<()> {
    if name.trim().is_empty() {
        return Err("브랜치 이름을 입력해 주세요".into());
    }
    if name.starts_with('-') || git::run(root, &["check-ref-format", "--branch", name]).is_err() {
        return Err(format!("'{name}'은(는) 브랜치 이름으로 쓸 수 없어요"));
    }
    if local_branches(root)?.iter().any(|b| b == name) {
        return Err(format!("'{name}' 브랜치가 이미 있어요"));
    }
    Ok(())
}

pub fn switch(root: &Path, branch: &str) -> git::Result<()> {
    require_branch(root, branch)?;
    git::run(root, &["switch", "--no-guess", branch]).map(drop)
}

/// Creates `name` from `base` (a local branch) or HEAD, switching to it when
/// `switch_to` is set.
pub fn create_branch(
    root: &Path,
    name: &str,
    base: Option<&str>,
    switch_to: bool,
) -> git::Result<()> {
    valid_branch_name(root, name)?;
    if let Some(base) = base {
        require_branch(root, base)?;
    }
    let mut args = if switch_to {
        vec!["switch", "--no-guess", "-c", name]
    } else {
        vec!["branch", name]
    };
    args.extend(base);
    git::run(root, &args).map(drop)
}

pub fn check_branch(root: &Path, name: &str) -> git::Result<BranchCheck> {
    require_branch(root, name)?;
    let full = format!("refs/heads/{name}");
    // What `git branch -d` checks: merged into HEAD, or into the upstream.
    let mut range = vec![full.clone(), "--not".into(), "HEAD".into()];
    let upstream = format!("{full}@{{upstream}}");
    if git::run(root, &["rev-parse", "--verify", "--quiet", &upstream]).is_ok() {
        range.push(upstream);
    }
    let mut args = vec!["rev-list", "--count"];
    args.extend(range.iter().map(String::as_str));
    // An unborn HEAD has nothing to compare against: count everything.
    let count =
        git::run(root, &args).or_else(|_| git::run(root, &["rev-list", "--count", &full]))?;
    Ok(BranchCheck {
        unmerged: count.trim().parse().unwrap_or(0),
    })
}

pub fn delete_branch(root: &Path, name: &str, force: bool) -> git::Result<()> {
    if !force && check_branch(root, name)?.unmerged > 0 {
        return Err(format!("'{name}' 브랜치에 병합되지 않은 커밋이 있어요"));
    }
    require_branch(root, name)?;
    git::run(root, &["branch", if force { "-D" } else { "-d" }, name]).map(drop)
}

/// A worktree name: one path segment of plain characters.
fn valid_worktree_name(name: &str) -> git::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err("worktree 이름은 영문·숫자·-·_·. 만 쓸 수 있어요 (64자까지)".into())
    }
}

/// Adds `<main>/.claude/worktrees/<name>` on a new branch `worktree-<name>`,
/// like Claude Code's own worktrees. Returns its path.
pub fn add_worktree(root: &Path, name: &str, base: Option<&str>) -> git::Result<String> {
    valid_worktree_name(name)?;
    let overview = read::overview(root, false)?;
    if overview.head.commit.is_none() {
        return Err("첫 커밋을 만든 뒤에 worktree를 추가할 수 있어요".into());
    }
    let branch = format!("worktree-{name}");
    valid_branch_name(root, &branch)?;
    if let Some(base) = base {
        require_branch(root, base)?;
    }
    let dir = PathBuf::from(&overview.main_root).join(WORKTREES_DIR);
    let path = dir.join(name);
    if path.exists() {
        return Err(format!("{} 폴더가 이미 있어요", path.display()));
    }
    // Keep the nested worktrees out of the main worktree's status.
    crate::library::ops::exclude_add(&dir, true)?;
    let path_str = path.to_string_lossy().into_owned();
    let mut args = vec!["worktree", "add", "-b", &branch, &path_str];
    args.extend(base);
    git::run(root, &args)?;
    Ok(path_str)
}

/// Removes a worktree (never the main one or the open one). Its branch stays.
pub fn remove_worktree(root: &Path, path: &str, force: bool) -> git::Result<()> {
    let overview = read::overview(root, false)?;
    let key = read::path_key(path);
    let Some(wt) = overview
        .worktrees
        .iter()
        .find(|w| read::path_key(&w.path) == key)
    else {
        return Err("목록에 없는 worktree예요".into());
    };
    if wt.is_main {
        return Err("기본 worktree는 지울 수 없어요".into());
    }
    if wt.is_current {
        return Err(
            "지금 열려 있는 worktree는 지울 수 없어요. 다른 폴더를 연 뒤 지워 주세요".into(),
        );
    }
    if wt.locked {
        return Err("잠긴 worktree예요. `git worktree unlock`으로 푼 뒤 지워 주세요".into());
    }
    if wt.prunable {
        return git::run(root, &["worktree", "prune"]).map(drop);
    }
    if !force {
        let raw = git::run_raw(Path::new(&wt.path), &["status", "--porcelain=v2", "-z"])?;
        if read::parse_status(&raw).2 > 0 {
            return Err("커밋하지 않은 변경이 있는 worktree예요".into());
        }
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&wt.path);
    git::run(root, &args).map(drop)
}

pub fn prune_worktrees(root: &Path) -> git::Result<()> {
    git::run(root, &["worktree", "prune"]).map(drop)
}

/// Keeps the diff body: drops `diff --git`/`index` lines, cuts at `MAX_DIFF`.
fn clean_diff(raw: &[u8]) -> Diff {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::new();
    let mut binary = false;
    let mut truncated = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") || line.starts_with("index ") {
            continue;
        }
        if line.starts_with("Binary files ") {
            binary = true;
            continue;
        }
        if out.len() + line.len() > MAX_DIFF {
            truncated = true;
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    Diff {
        text: out,
        binary,
        truncated,
    }
}

/// The diff of one changed file against HEAD (staged and unstaged together).
pub fn diff(
    root: &Path,
    path: &str,
    orig_path: Option<&str>,
    untracked: bool,
) -> git::Result<Diff> {
    if Path::new(path).is_absolute() || path.split(['/', '\\']).any(|p| p == "..") {
        return Err("잘못된 경로예요".into());
    }
    if untracked {
        let file = root.join(path);
        if file.is_dir() {
            return Ok(Diff {
                text: "새 폴더예요. 안의 파일은 목록에 따로 나오지 않아요.\n".into(),
                ..Diff::default()
            });
        }
        let bytes = std::fs::read(&file).map_err(|e| format!("파일을 읽을 수 없어요: {e}"))?;
        if bytes.iter().take(8000).any(|&b| b == 0) {
            return Ok(Diff {
                binary: true,
                ..Diff::default()
            });
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines = text.lines().count();
        let mut body = format!("--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{lines} @@\n");
        for line in text.lines() {
            body.push('+');
            body.push_str(line);
            body.push('\n');
        }
        return Ok(clean_diff(body.as_bytes()));
    }
    let has_head = git::run(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
    let base = if has_head { "HEAD" } else { EMPTY_TREE };
    let mut args = vec![
        "--literal-pathspecs",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        base,
        "--",
        path,
    ];
    args.extend(orig_path);
    let raw = git::run_raw(root, &args)?;
    Ok(clean_diff(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn repo() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "init"]);
        (tmp, root)
    }

    #[test]
    fn branches_are_created_switched_and_deleted_safely() {
        let (_tmp, root) = repo();
        assert!(create_branch(&root, "-x", None, false).is_err());
        assert!(create_branch(&root, "a..b", None, false).is_err());
        create_branch(&root, "feature", None, true).unwrap();
        assert!(create_branch(&root, "feature", None, false).is_err());

        std::fs::write(root.join("b.txt"), "b").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "b"]);
        switch(&root, "main").unwrap();
        assert!(switch(&root, "nope").is_err());

        assert_eq!(check_branch(&root, "feature").unwrap().unmerged, 1);
        assert!(delete_branch(&root, "feature", false).is_err());
        delete_branch(&root, "feature", true).unwrap();

        create_branch(&root, "merged", Some("main"), false).unwrap();
        assert_eq!(check_branch(&root, "merged").unwrap().unmerged, 0);
        delete_branch(&root, "merged", false).unwrap();
    }

    #[test]
    fn worktrees_follow_claude_code_and_refuse_losing_work() {
        let (_tmp, root) = repo();
        assert!(add_worktree(&root, "../x", None).is_err());
        let path = add_worktree(&root, "task", None).unwrap();
        assert!(path.ends_with(".claude/worktrees/task"), "{path}");

        let ov = read::overview(&root, true).unwrap();
        let wt = ov.worktrees.iter().find(|w| w.managed).unwrap();
        assert_eq!(wt.branch.as_deref(), Some("worktree-task"));
        assert_eq!(wt.changes, Some(0));
        // The nested worktree is excluded, so the main one stays clean.
        assert_eq!(ov.change_count, 0, "{:?}", ov.changes);

        std::fs::write(Path::new(&path).join("new.txt"), "x").unwrap();
        assert!(remove_worktree(&root, &path, false).is_err());
        assert!(remove_worktree(&root, &ov.main_root, true).is_err());
        // Not from inside the worktree being removed.
        assert!(remove_worktree(Path::new(&path), &path, true).is_err());
        remove_worktree(&root, &path, true).unwrap();
        assert!(!Path::new(&path).exists());
        // The branch stays.
        assert!(
            local_branches(&root)
                .unwrap()
                .contains(&"worktree-task".to_string())
        );
    }

    #[test]
    fn diffs_cover_tracked_untracked_and_unsafe_paths() {
        let (_tmp, root) = repo();
        std::fs::write(root.join("a.txt"), "one\n2\n").unwrap();
        std::fs::write(root.join("new.txt"), "hello\n").unwrap();
        std::fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();

        let d = diff(&root, "a.txt", None, false).unwrap();
        assert!(d.text.contains("-two\n+2\n"), "{}", d.text);
        assert!(!d.text.contains("diff --git"));

        let d = diff(&root, "new.txt", None, true).unwrap();
        assert!(d.text.ends_with("@@ -0,0 +1,1 @@\n+hello\n"), "{}", d.text);
        assert!(diff(&root, "bin.dat", None, true).unwrap().binary);
        assert!(diff(&root, "../etc/passwd", None, true).is_err());
        assert!(diff(&root, "/etc/passwd", None, true).is_err());

        let ov = read::overview(&root, false).unwrap();
        assert_eq!(ov.change_count, 3);
        assert_eq!(ov.head.branch.as_deref(), Some("main"));
    }

    #[test]
    fn an_unborn_repository_still_diffs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "x\n").unwrap();
        git(root, &["add", "."]);
        let d = diff(root, "a.txt", None, false).unwrap();
        assert!(d.text.contains("+x"), "{}", d.text);
        assert!(add_worktree(root, "t", None).is_err());
    }
}
