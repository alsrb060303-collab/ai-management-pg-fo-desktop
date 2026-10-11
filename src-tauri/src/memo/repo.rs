//! Which repository and branch a project folder is, as memo keys.
//!
//! A repository is identified by its normalized `origin` remote, so clones
//! and worktrees of the same repository (and, later, other devices) share
//! memos. Without a remote the shared git directory is used; outside git,
//! the folder itself.

use std::path::{Path, PathBuf};

use crate::git;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub key: String,
    pub name: String,
    pub is_git: bool,
    /// Checked-out branch; `None` outside git or on a detached HEAD.
    pub branch: Option<String>,
    /// Short commit of a detached HEAD.
    pub detached_at: Option<String>,
    pub branches: Vec<String>,
}

/// `https://github.com/Owner/Repo.git`, `git@github.com:owner/repo` and
/// `ssh://git@github.com:22/owner/repo/` all become `github.com/owner/repo`.
pub fn normalize_remote(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let rest = if let Some((_, after)) = url.split_once("://") {
        // scheme://[user[:pass]@]host[:port]/path
        let after = after.rsplit_once('@').map_or(after, |(_, host)| host);
        let (host, path) = after.split_once('/').unwrap_or((after, ""));
        let host = host.split(':').next().unwrap_or(host);
        format!("{host}/{path}")
    } else if let Some((user_host, path)) = url.split_once(':').filter(|(h, _)| {
        // scp-like `[user@]host:path`, but not a Windows drive (`C:\`).
        h.len() > 1 && !h.contains(['/', '\\'])
    }) {
        let host = user_host.rsplit_once('@').map_or(user_host, |(_, h)| h);
        format!("{host}/{}", path.trim_start_matches('/'))
    } else {
        // A local path used as a remote.
        return format!("file:{}", url.replace('\\', "/"));
    };
    rest.trim_end_matches('/').to_lowercase()
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn remote_url(root: &Path) -> Option<String> {
    if let Ok(url) = git::run(root, &["remote", "get-url", "origin"]) {
        return Some(url).filter(|u| !u.is_empty());
    }
    let first = git::run(root, &["remote"]).ok()?;
    let name = first.lines().next()?.trim().to_string();
    git::run(root, &["remote", "get-url", &name])
        .ok()
        .filter(|u| !u.is_empty())
}

pub fn inspect(project: &Path) -> git::Result<RepoInfo> {
    let dir: PathBuf =
        std::fs::canonicalize(project).map_err(|e| format!("프로젝트 폴더를 열 수 없어요: {e}"))?;
    if !dir.is_dir() {
        return Err("프로젝트 폴더가 아니에요".into());
    }

    let Ok(root) = git::run(&dir, &["rev-parse", "--show-toplevel"]) else {
        // Not a git repository (or git is missing): one memo for the folder.
        return Ok(RepoInfo {
            key: format!("path:{}", dir.display()),
            name: folder_name(&dir),
            is_git: false,
            branch: None,
            detached_at: None,
            branches: Vec::new(),
        });
    };
    let root = PathBuf::from(root);

    let (key, name) = match remote_url(&root) {
        Some(url) => {
            let key = normalize_remote(&url);
            let name = key.rsplit('/').next().unwrap_or(&key).to_string();
            (key, name)
        }
        None => {
            // Worktrees share the common git dir, so they share memos.
            let common = git::run(
                &root,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .map(PathBuf::from)
            .unwrap_or_else(|_| root.join(".git"));
            let base = std::fs::canonicalize(&common).unwrap_or(common);
            let main = base.parent().map(Path::to_path_buf).unwrap_or(base.clone());
            (format!("path:{}", main.display()), folder_name(&main))
        }
    };

    let branch = git::run(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let detached_at = if branch.is_none() {
        git::run(&root, &["rev-parse", "--short", "HEAD"]).ok()
    } else {
        None
    };
    let branches = git::run(
        &root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .map(|out| {
        out.lines()
            .map(str::to_string)
            .filter(|b| !b.is_empty())
            .collect()
    })
    .unwrap_or_default();

    Ok(RepoInfo {
        key,
        name,
        is_git: true,
        branch,
        detached_at,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_of_the_same_repo_normalize_alike() {
        let expected = "github.com/owner/repo";
        for url in [
            "https://github.com/Owner/Repo.git",
            "https://user:token@github.com/owner/repo/",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com:22/owner/repo",
            "git://github.com/owner/repo",
        ] {
            assert_eq!(normalize_remote(url), expected, "{url}");
        }
        assert_eq!(normalize_remote("/srv/git/repo.git"), "file:/srv/git/repo");
        assert_eq!(normalize_remote(r"C:\repos\x.git"), "file:C:/repos/x");
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn inspects_branches_remotes_and_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&repo, &["branch", "feature/x"]);

        let info = inspect(&repo).unwrap();
        assert!(info.is_git);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert_eq!(info.branches, ["feature/x", "main"]);
        assert!(info.key.starts_with("path:"), "{}", info.key);

        // A worktree without a remote shares the key; a remote takes over.
        let wt = tmp.path().join("wt");
        git(
            &repo,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "feature/x"],
        );
        let wt_info = inspect(&wt).unwrap();
        assert_eq!(wt_info.key, info.key);
        assert_eq!(wt_info.branch.as_deref(), Some("feature/x"));

        git(
            &repo,
            &["remote", "add", "origin", "git@github.com:Me/Proj.git"],
        );
        let info = inspect(&repo).unwrap();
        assert_eq!(info.key, "github.com/me/proj");
        assert_eq!(info.name, "proj");

        git(&repo, &["checkout", "-q", "--detach"]);
        let info = inspect(&repo).unwrap();
        assert_eq!(info.branch, None);
        assert!(info.detached_at.is_some());
    }

    #[test]
    fn a_plain_folder_is_its_own_key() {
        let tmp = tempfile::tempdir().unwrap();
        let info = inspect(tmp.path()).unwrap();
        assert!(!info.is_git);
        assert!(info.key.starts_with("path:"));
        assert_eq!(info.branch, None);
    }
}
