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
        Err(crate::tr!(
            "'{name}' 브랜치가 없어요",
            "There's no branch named '{name}'"
        ))
    }
}

/// A new branch name git accepts (`check-ref-format` also rejects a leading
/// dash, so names never read as options).
fn valid_branch_name(root: &Path, name: &str) -> git::Result<()> {
    if name.trim().is_empty() {
        return Err(crate::tr!(
            "브랜치 이름을 입력해 주세요",
            "Enter a branch name"
        ));
    }
    if name.starts_with('-') || git::run(root, &["check-ref-format", "--branch", name]).is_err() {
        return Err(crate::tr!(
            "'{name}'은(는) 브랜치 이름으로 쓸 수 없어요",
            "'{name}' can't be used as a branch name"
        ));
    }
    if local_branches(root)?.iter().any(|b| b == name) {
        return Err(crate::tr!(
            "'{name}' 브랜치가 이미 있어요",
            "A branch named '{name}' already exists"
        ));
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
        return Err(crate::tr!(
            "'{name}' 브랜치에 병합되지 않은 커밋이 있어요",
            "The '{name}' branch has unmerged commits"
        ));
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
        Err(crate::tr!(
            "worktree 이름은 영문·숫자·-·_·. 만 쓸 수 있어요 (64자까지)",
            "A worktree name can only use letters, digits, -, _ and . (up to 64 characters)"
        ))
    }
}

/// Adds `<main>/.claude/worktrees/<name>` on a new branch `worktree-<name>`,
/// like Claude Code's own worktrees. Returns its path.
pub fn add_worktree(root: &Path, name: &str, base: Option<&str>) -> git::Result<String> {
    valid_worktree_name(name)?;
    let overview = read::overview(root, false)?;
    if overview.head.commit.is_none() {
        return Err(crate::tr!(
            "첫 커밋을 만든 뒤에 worktree를 추가할 수 있어요",
            "Make the first commit before adding a worktree"
        ));
    }
    let branch = format!("worktree-{name}");
    valid_branch_name(root, &branch)?;
    if let Some(base) = base {
        require_branch(root, base)?;
    }
    let dir = PathBuf::from(&overview.main_root).join(WORKTREES_DIR);
    let path = dir.join(name);
    if path.exists() {
        return Err(crate::tr!(
            "{} 폴더가 이미 있어요",
            "The folder {} already exists",
            path.display()
        ));
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
        return Err(crate::tr!(
            "목록에 없는 worktree예요",
            "This worktree isn't in the list"
        ));
    };
    if wt.is_main {
        return Err(crate::tr!(
            "기본 worktree는 지울 수 없어요",
            "The main worktree can't be deleted"
        ));
    }
    if wt.is_current {
        return Err(crate::tr!(
            "지금 열려 있는 worktree는 지울 수 없어요. 다른 폴더를 연 뒤 지워 주세요",
            "The open worktree can't be deleted. Open another folder first"
        ));
    }
    if wt.locked {
        return Err(crate::tr!(
            "잠긴 worktree예요. `git worktree unlock`으로 푼 뒤 지워 주세요",
            "This worktree is locked. Unlock it with `git worktree unlock` first"
        ));
    }
    if wt.prunable {
        return forget_missing(root, &wt.path);
    }
    if !force {
        let dir = Path::new(&wt.path);
        let raw = git::run_raw(dir, &["status", "--porcelain=v2", "-z"])?;
        if read::parse_status(&raw).2 > 0 {
            return Err(crate::tr!(
                "커밋하지 않은 변경이 있는 worktree예요",
                "This worktree has uncommitted changes"
            ));
        }
        // A detached HEAD's commits live only in its reflog, which goes too.
        if wt.branch.is_none() {
            let args = [
                "rev-list",
                "--count",
                "HEAD",
                "--not",
                "--branches",
                "--remotes",
                "--tags",
            ];
            let lost: u32 = git::run(dir, &args)?.trim().parse().unwrap_or(0);
            if lost > 0 {
                return Err(crate::tr!(
                    "어느 브랜치에도 없는 커밋이 {lost}개 있는 worktree예요",
                    "This worktree has {lost} commit(s) that aren't on any branch"
                ));
            }
        }
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&wt.path);
    git::run(root, &args).map(drop)
}

/// Drops the record of one worktree whose folder is gone (`git worktree
/// prune` would drop every missing one, e.g. all on an unplugged drive).
fn forget_missing(root: &Path, path: &str) -> git::Result<()> {
    let common = git::run(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let admin = Path::new(&common).join("worktrees");
    let key = read::path_key(path);
    let entries = std::fs::read_dir(&admin).map_err(|e| {
        crate::tr!(
            "worktree 기록을 읽을 수 없어요: {e}",
            "Couldn't read the worktree records: {e}"
        )
    })?;
    for entry in entries.flatten() {
        // `gitdir` holds `<worktree>/.git`.
        let Ok(gitdir) = std::fs::read_to_string(entry.path().join("gitdir")) else {
            continue;
        };
        let gitdir = gitdir.trim();
        let folder = gitdir.strip_suffix(".git").unwrap_or(gitdir);
        if read::path_key(folder) == key {
            return std::fs::remove_dir_all(entry.path()).map_err(|e| {
                crate::tr!(
                    "worktree 기록을 지울 수 없어요: {e}",
                    "Couldn't delete the worktree record: {e}"
                )
            });
        }
    }
    Err(crate::tr!(
        "worktree 기록을 찾지 못했어요",
        "Couldn't find the worktree record"
    ))
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
    // Split on `\n` only, so a CRLF change still shows its `\r`.
    let text = text.strip_suffix('\n').unwrap_or(&text);
    for line in text.split('\n') {
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
    use std::path::Component;
    let normal = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !normal {
        return Err(crate::tr!("잘못된 경로예요", "Invalid path"));
    }
    if untracked {
        let file = root.join(path);
        let meta = std::fs::symlink_metadata(&file)
            .map_err(|e| crate::tr!("파일을 읽을 수 없어요: {e}", "Couldn't read the file: {e}"))?;
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&file)
                .map(|t| t.display().to_string())
                .unwrap_or_default();
            return Ok(Diff {
                text: crate::tr!("심볼릭 링크예요 → {target}\n", "Symbolic link → {target}\n"),
                ..Diff::default()
            });
        }
        if meta.is_dir() {
            return Ok(Diff {
                text: crate::tr!(
                    "새 폴더예요. 안의 파일은 목록에 따로 나오지 않아요.\n",
                    "New folder. The files inside aren't listed separately.\n"
                ),
                ..Diff::default()
            });
        }
        // Only what can be shown; a huge file is never read whole.
        let mut bytes = Vec::new();
        std::fs::File::open(&file)
            .and_then(|f| {
                std::io::Read::read_to_end(
                    &mut std::io::Read::take(f, MAX_DIFF as u64 + 1),
                    &mut bytes,
                )
            })
            .map_err(|e| crate::tr!("파일을 읽을 수 없어요: {e}", "Couldn't read the file: {e}"))?;
        if bytes.iter().take(8000).any(|&b| b == 0) {
            return Ok(Diff {
                binary: true,
                ..Diff::default()
            });
        }
        let cut = bytes.len() > MAX_DIFF;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_DIFF)]);
        let text = text.strip_suffix('\n').unwrap_or(&text);
        let lines: Vec<&str> = text.split('\n').collect();
        let mut body = format!(
            "--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
            lines.len()
        );
        for line in lines {
            body.push('+');
            body.push_str(line);
            body.push('\n');
        }
        let mut diff = clean_diff(body.as_bytes());
        diff.truncated |= cut;
        return Ok(diff);
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

/// Commits with `message`: every change (`all`, like `git add -A`) or only
/// what is staged. Returns the new commit's short hash.
pub fn commit(root: &Path, message: &str, all: bool) -> git::Result<String> {
    if message.trim().is_empty() {
        return Err(crate::tr!(
            "커밋 메시지를 입력해 주세요",
            "Enter a commit message"
        ));
    }
    if all {
        git::run(root, &["add", "-A"])?;
    }
    if git::run(root, &["diff", "--cached", "--quiet"]).is_ok() {
        return Err(crate::tr!(
            "커밋할 변경이 없어요",
            "There's nothing to commit"
        ));
    }
    git::run(root, &["commit", "-m", message.trim()])?;
    git::run(root, &["rev-parse", "--short", "HEAD"])
}

fn current_branch(root: &Path) -> git::Result<String> {
    git::run(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).map_err(|_| {
        crate::tr!(
            "브랜치를 체크아웃한 상태에서 할 수 있어요",
            "Check out a branch first"
        )
    })
}

/// Pushes the current branch, setting its upstream on `origin` the first time.
pub fn push(root: &Path, auth: &[(String, String)]) -> git::Result<()> {
    let branch = current_branch(root)?;
    let has_upstream = git::run(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .is_ok();
    if has_upstream {
        git::run_env(root, &["push"], auth)?;
    } else {
        git::run_env(root, &["push", "-u", "origin", "--", &branch], auth)?;
    }
    Ok(())
}

/// Brings in the upstream's commits, only when no merge is needed.
pub fn pull(root: &Path, auth: &[(String, String)]) -> git::Result<()> {
    current_branch(root)?;
    git::run_env(root, &["pull", "--ff-only"], auth).map(drop)
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
        assert!(
            read::path_key(&path).ends_with(".claude/worktrees/task"),
            "{path}"
        );

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
    fn detached_commits_and_other_missing_worktrees_are_kept() {
        let (tmp, root) = repo();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let detached = base.join("detached");
        let (gone_a, gone_b) = (base.join("gone-a"), base.join("gone-b"));
        for (dir, args) in [
            (&detached, vec!["--detach"]),
            (&gone_a, vec!["-b", "a"]),
            (&gone_b, vec!["-b", "b"]),
        ] {
            let mut cmd = vec!["worktree", "add", "-q"];
            cmd.extend(args);
            cmd.push(dir.to_str().unwrap());
            git(&root, &cmd);
        }
        git(
            &detached,
            &["commit", "-q", "--allow-empty", "-m", "only here"],
        );
        let d = detached.to_str().unwrap();
        let err = remove_worktree(&root, d, false).unwrap_err();
        assert!(err.contains("1개"), "{err}");
        remove_worktree(&root, d, true).unwrap();

        std::fs::remove_dir_all(&gone_a).unwrap();
        std::fs::remove_dir_all(&gone_b).unwrap();
        remove_worktree(&root, gone_a.to_str().unwrap(), false).unwrap();
        let left: Vec<_> = read::overview(&root, false)
            .unwrap()
            .worktrees
            .into_iter()
            .filter(|w| w.prunable)
            .map(|w| w.branch)
            .collect();
        assert_eq!(left, [Some("b".to_string())]);
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
        assert!(diff(&root, "./a.txt", None, true).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hostname", root.join("link")).unwrap();
            let d = diff(&root, "link", None, true).unwrap();
            assert!(d.text.starts_with("심볼릭 링크"), "{}", d.text);
        }
        std::fs::write(root.join("crlf.txt"), "x\r\n").unwrap();
        assert!(
            diff(&root, "crlf.txt", None, true)
                .unwrap()
                .text
                .contains("+x\r\n")
        );

        let ov = read::overview(&root, false).unwrap();
        assert_eq!(ov.change_count, if cfg!(unix) { 5 } else { 4 });
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
    #[test]
    fn commits_push_and_pull_through_a_remote() {
        let (tmp, root) = repo();
        git(&root, &["config", "user.email", "t@t"]);
        git(&root, &["config", "user.name", "t"]);
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "-q", "--bare", "-b", "main", "remote.git"],
        );
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );

        // Nothing to commit, no message: refused.
        assert!(commit(&root, "x", true).is_err());
        std::fs::write(root.join("a.txt"), "changed\n").unwrap();
        assert!(commit(&root, "  ", true).is_err());
        // Only staged, with nothing staged: refused; everything: committed.
        assert!(commit(&root, "x", false).is_err());
        let hash = commit(&root, "Change a", true).unwrap();
        assert!(!hash.is_empty());
        // Nothing is on the remote yet.
        let before = super::super::read::overview(&root, false).unwrap();
        assert_eq!(before.unpushed as usize, before.log.len());
        assert_eq!(before.log[0].subject, "Change a");
        assert!(!before.log[0].pushed);

        // The first push sets the upstream.
        push(&root, &[]).unwrap();
        let after = super::super::read::overview(&root, false).unwrap();
        assert_eq!(after.unpushed, 0);
        assert!(after.log.iter().all(|c| c.pushed));
        assert!(after.base.is_none(), "main is its own base");

        // A feature branch is measured against origin/main.
        git(&root, &["switch", "-q", "-c", "feat"]);
        std::fs::write(root.join("c.txt"), "c").unwrap();
        commit(&root, "Add c", true).unwrap();
        let feat = super::super::read::overview(&root, false).unwrap();
        let base = feat.base.unwrap();
        assert_eq!(
            (base.name.as_str(), base.ahead, base.behind),
            ("origin/main", 1, 0)
        );
        assert_eq!(feat.unpushed, 1);
        git(&root, &["switch", "-q", "main"]);
        let upstream = git::run(&root, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap();
        assert_eq!(upstream, "origin/main");

        // Another clone pushes; pull fast-forwards.
        let other = tmp.path().join("other");
        git(
            tmp.path(),
            &["clone", "-q", remote.to_str().unwrap(), "other"],
        );
        std::fs::write(other.join("b.txt"), "b").unwrap();
        git(&other, &["add", "."]);
        git(&other, &["commit", "-q", "-m", "b"]);
        git(&other, &["push", "-q"]);
        pull(&root, &[]).unwrap();
        assert!(root.join("b.txt").exists());
    }
}
