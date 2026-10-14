//! The GitHub log's sources: the local `git` (commits on every branch, merges,
//! pushes seen through remote-tracking branches, branch tips) and the GitHub
//! API (pull requests, CI runs, through the existing sign-in).
//!
//! Event ids follow `activity::GitEvent::id`: `git:<kind>:<sha or pr number>[:<extra>]`
//! with the kind spelled as serialized (`commit`, `merge`, `push`, `prOpened`,
//! `prMerged`, `prClosed`, `ci`), so collecting again gives the same ids.
//! They do not contain the repository key; the store keys events by repo too.

use std::collections::HashMap;
use std::path::Path;

use tauri::AppHandle;

use super::sessions::parse_iso_ms;
use super::{BranchTip, GitEvent, GitKind};
use crate::git;

const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

fn kind_name(kind: GitKind) -> &'static str {
    match kind {
        GitKind::Commit => "commit",
        GitKind::Push => "push",
        GitKind::Merge => "merge",
        GitKind::Branch => "branch",
        GitKind::PrOpened => "prOpened",
        GitKind::PrMerged => "prMerged",
        GitKind::PrClosed => "prClosed",
        GitKind::Ci => "ci",
    }
}

fn event(repo: &str, kind: GitKind, at: i64, key: &str, title: String) -> GitEvent {
    GitEvent {
        id: format!("git:{}:{key}", kind_name(kind)),
        repo: repo.to_string(),
        kind,
        at,
        branch: None,
        sha: None,
        parents: Vec::new(),
        pr: None,
        title,
        state: None,
        url: None,
        author: None,
    }
}

/// `owner/repo` of a `github.com/owner/repo` repository key.
fn github_slug(repo: &str) -> Option<String> {
    repo.strip_prefix("github.com/")
        .filter(|s| s.split('/').count() == 2 && s.split('/').all(|p| !p.is_empty()))
        .map(str::to_string)
}

// ---------------------------------------------------------------- local git

/// One commit from `git log`.
#[derive(Debug, PartialEq, Eq)]
struct LogCommit {
    sha: String,
    parents: Vec<String>,
    author: String,
    /// Seconds since the epoch.
    time: i64,
    /// Branches pointing at it (local first), from the decoration.
    heads: Decoration,
    /// The ref the commit was reached from (`--source`).
    source: Option<String>,
    subject: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Decoration {
    /// `HEAD -> refs/heads/x`.
    current: Option<String>,
    locals: Vec<String>,
    /// Remote-tracking branches without the remote name (`origin/x` → `x`).
    remotes: Vec<String>,
}

const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ct%x1f%D%x1f%S%x1f%s%x1e";

/// `HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1`.
fn parse_decoration(d: &str) -> Decoration {
    let mut out = Decoration::default();
    for part in d.split(", ") {
        let (is_head, part) = match part.strip_prefix("HEAD -> ") {
            Some(rest) => (true, rest),
            None => (false, part),
        };
        if let Some(name) = part.strip_prefix("refs/heads/") {
            if is_head {
                out.current = Some(name.to_string());
            }
            out.locals.push(name.to_string());
        } else if let Some(name) = part.strip_prefix("refs/remotes/") {
            // `origin/main`, and `origin/HEAD` which is not a branch.
            if let Some((_, branch)) = name.split_once('/')
                && branch != "HEAD"
            {
                out.remotes.push(branch.to_string());
            }
        }
    }
    out
}

/// The branch a `--source` ref stands for (`refs/heads/x`, `refs/remotes/origin/x`).
fn source_branch(source: &str) -> Option<String> {
    if let Some(b) = source.strip_prefix("refs/heads/") {
        return Some(b.to_string());
    }
    let b = source.strip_prefix("refs/remotes/")?;
    b.split_once('/')
        .map(|(_, b)| b.to_string())
        .filter(|b| b != "HEAD")
}

fn parse_log(out: &str) -> Vec<LogCommit> {
    out.split(RECORD)
        .filter_map(|record| {
            let mut f = record.trim_start_matches('\n').split(FIELD);
            let sha = f.next().filter(|h| !h.is_empty())?.to_string();
            let parents = f.next()?.split_whitespace().map(str::to_string).collect();
            let author = f.next()?.to_string();
            let time = f.next()?.trim().parse().ok()?;
            let heads = parse_decoration(f.next()?);
            let source = f.next().and_then(source_branch);
            let subject = f.next().unwrap_or_default().to_string();
            Some(LogCommit {
                sha,
                parents,
                author,
                time,
                heads,
                source,
                subject,
            })
        })
        .collect()
}

/// A branch or remote-tracking ref from `git for-each-ref`.
#[derive(Debug, PartialEq, Eq)]
struct RefRow {
    name: String,
    sha: String,
    /// Full name of the upstream, for local branches.
    upstream: Option<String>,
    current: bool,
}

const REF_FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(upstream)%1f%(HEAD)";

fn parse_refs(out: &str) -> Vec<RefRow> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.split(FIELD);
            let name = f.next().filter(|n| !n.is_empty())?.to_string();
            let sha = f.next().filter(|s| !s.is_empty())?.to_string();
            let upstream = f.next().filter(|u| !u.is_empty()).map(str::to_string);
            let current = f.next().is_some_and(|h| h.trim() == "*");
            Some(RefRow {
                name,
                sha,
                upstream,
                current,
            })
        })
        .collect()
}

/// Local branches with their upstream tips, remote-only branches, the default
/// and the current branch.
fn branch_tips(refs: &[RefRow], default_ref: Option<&str>) -> Vec<BranchTip> {
    let remote_sha: HashMap<&str, &str> = refs
        .iter()
        .filter(|r| r.name.starts_with("refs/remotes/"))
        .map(|r| (r.name.as_str(), r.sha.as_str()))
        .collect();
    let mut tips: Vec<BranchTip> = Vec::new();
    for r in refs {
        if let Some(name) = r.name.strip_prefix("refs/heads/") {
            let remote = r
                .upstream
                .as_deref()
                .and_then(|u| remote_sha.get(u))
                .or_else(|| remote_sha.get(format!("refs/remotes/origin/{name}").as_str()))
                .map(|s| s.to_string());
            tips.push(BranchTip {
                name: name.to_string(),
                local: Some(r.sha.clone()),
                remote,
                is_default: false,
                is_current: r.current,
            });
        }
    }
    for r in refs {
        let Some(rest) = r.name.strip_prefix("refs/remotes/") else {
            continue;
        };
        let Some((_, name)) = rest.split_once('/') else {
            continue;
        };
        if name == "HEAD" || tips.iter().any(|t| t.name == name) {
            continue;
        }
        tips.push(BranchTip {
            name: name.to_string(),
            local: None,
            remote: Some(r.sha.clone()),
            is_default: false,
            is_current: false,
        });
    }

    let default = default_ref
        .and_then(|d| d.strip_prefix("refs/remotes/"))
        .and_then(|d| d.split_once('/').map(|(_, b)| b.to_string()))
        .filter(|b| tips.iter().any(|t| &t.name == b))
        .or_else(|| {
            ["main", "master"]
                .iter()
                .find(|b| tips.iter().any(|t| &t.name == *b))
                .map(|b| b.to_string())
        })
        .or_else(|| tips.iter().find(|t| t.is_current).map(|t| t.name.clone()));
    for t in &mut tips {
        t.is_default = default.as_deref() == Some(t.name.as_str());
    }
    tips.sort_by(|a, b| a.name.cmp(&b.name));
    tips
}

/// Which branch each commit belongs to, then the events. Commits arrive
/// newest first, so a commit's children are seen before it: a branch tip
/// passes its name down its first parents; commits no branch points at or
/// reaches (a merged and deleted branch) fall back to `--source`.
fn local_events(
    repo: &str,
    commits: &[LogCommit],
    tips: &[BranchTip],
) -> (Vec<GitEvent>, HashMap<String, usize>) {
    let default = tips.iter().find(|t| t.is_default).map(|t| t.name.as_str());
    let url_base = github_slug(repo).map(|s| format!("https://github.com/{s}/commit/"));
    let mut inherit: HashMap<&str, String> = HashMap::new();
    let mut events = Vec::with_capacity(commits.len());
    let mut index: HashMap<String, usize> = HashMap::new();

    for c in commits {
        let h = &c.heads;
        let own = h
            .current
            .as_ref()
            .or_else(|| default.and_then(|d| h.locals.iter().find(|l| l.as_str() == d)))
            .or_else(|| h.locals.first())
            .or_else(|| h.remotes.first());
        let branch = match inherit.get(c.sha.as_str()) {
            // A tip on the way down the same branch keeps the lane.
            Some(b) if own.is_none() || h.locals.contains(b) || h.remotes.contains(b) => {
                Some(b.clone())
            }
            Some(b) => own.cloned().or_else(|| Some(b.clone())),
            None => own.cloned().or_else(|| c.source.clone()),
        };
        if let (Some(b), Some(first)) = (&branch, c.parents.first()) {
            inherit.entry(first.as_str()).or_insert_with(|| b.clone());
        }

        let kind = if c.parents.len() > 1 {
            GitKind::Merge
        } else {
            GitKind::Commit
        };
        let mut e = event(repo, kind, c.time * 1000, &c.sha, c.subject.clone());
        e.branch = branch.clone();
        e.sha = Some(c.sha.clone());
        e.parents = c.parents.clone();
        e.author = Some(c.author.clone()).filter(|a| !a.is_empty());
        e.url = url_base.as_ref().map(|u| format!("{u}{}", c.sha));
        if kind == GitKind::Merge {
            e.state = branch;
        }
        index.insert(c.sha.clone(), events.len());
        events.push(e);
    }
    (events, index)
}

/// A push per branch whose remote-tracking tip is a commit we know. Git keeps
/// no record of when a push happened, so the event carries the time of the
/// commit that is now the remote tip.
fn push_events(
    root: &Path,
    repo: &str,
    tips: &[BranchTip],
    events: &[GitEvent],
    index: &HashMap<String, usize>,
) -> Vec<GitEvent> {
    let mut out = Vec::new();
    for tip in tips {
        let Some(sha) = &tip.remote else { continue };
        let (at, title) = match index.get(sha) {
            Some(&i) => (events[i].at, events[i].title.clone()),
            None => {
                // Older than the commits read: ask git for its time.
                let Ok(t) = git::run(root, &["show", "-s", "--format=%ct%x1f%s", sha]) else {
                    continue;
                };
                let (ct, subject) = t.split_once(FIELD).unwrap_or((&t, ""));
                let Ok(secs) = ct.trim().parse::<i64>() else {
                    continue;
                };
                (secs * 1000, subject.to_string())
            }
        };
        let mut e = event(
            repo,
            GitKind::Push,
            at,
            &format!("{}:{sha}", tip.name),
            title,
        );
        e.branch = Some(tip.name.clone());
        e.sha = Some(sha.clone());
        out.push(e);
    }
    out
}

/// Commits (newest first, at most `limit`), merges and pushes of every
/// branch, and the branch tips.
pub fn collect_local(
    root: &Path,
    repo: &str,
    limit: usize,
) -> Result<(Vec<GitEvent>, Vec<BranchTip>), String> {
    let refs = parse_refs(&git::run(
        root,
        &["for-each-ref", REF_FORMAT, "refs/heads", "refs/remotes"],
    )?);
    if refs.is_empty() {
        // No commits yet (or a detached HEAD only): nothing to draw.
        return Ok((Vec::new(), Vec::new()));
    }
    let default_ref = git::run(root, &["symbolic-ref", "-q", "refs/remotes/origin/HEAD"]).ok();
    let tips = branch_tips(&refs, default_ref.as_deref());

    let n = format!("-n{}", limit.max(1));
    let log = git::run_raw(
        root,
        &[
            "log",
            "--all",
            "--date-order",
            "--decorate=full",
            "--source",
            &n,
            LOG_FORMAT,
        ],
    )?;
    let commits = parse_log(&String::from_utf8_lossy(&log));
    let (mut events, index) = local_events(repo, &commits, &tips);
    let pushes = push_events(root, repo, &tips, &events, &index);
    events.extend(pushes);
    Ok((events, tips))
}

// ---------------------------------------------------------------- GitHub

/// Pull requests (opened, merged, closed) and CI results from GitHub. Empty
/// when not signed in to GitHub or when the repository is not on github.com.
pub fn collect_github(app: &AppHandle, _root: &Path, repo: &str) -> Result<Vec<GitEvent>, String> {
    let Some(slug) = github_slug(repo) else {
        return Ok(Vec::new());
    };
    let Some(remote) = crate::github::activity(app, &slug)? else {
        return Ok(Vec::new());
    };
    Ok(github_events(repo, &remote))
}

fn ci_state(conclusion: &str) -> Option<&'static str> {
    match conclusion {
        "success" => Some("success"),
        "failure" | "timed_out" | "startup_failure" | "action_required" => Some("failure"),
        "cancelled" => Some("cancelled"),
        // Skipped, neutral and stale runs say nothing about the commit.
        _ => None,
    }
}

fn github_events(repo: &str, remote: &crate::github::ActivityRemote) -> Vec<GitEvent> {
    let mut out = Vec::new();
    for p in &remote.pulls {
        let number = p.number.to_string();
        let base = |kind: GitKind, at: &str| {
            let at = parse_iso_ms(at)?;
            let mut e = event(repo, kind, at, &number, p.title.clone());
            e.branch = Some(p.head.clone());
            e.sha = Some(p.head_sha.clone()).filter(|s| !s.is_empty());
            e.pr = Some(p.number);
            e.url = Some(p.url.clone());
            e.author = Some(p.author.clone()).filter(|a| !a.is_empty());
            Some(e)
        };
        if let Some(e) = base(GitKind::PrOpened, &p.created_at) {
            out.push(e);
        }
        match (&p.merged_at, &p.closed_at, p.state.as_str()) {
            (Some(at), _, _) => out.extend(base(GitKind::PrMerged, at).map(|mut e| {
                e.state = Some(p.base.clone());
                e
            })),
            (None, Some(at), "closed") => out.extend(base(GitKind::PrClosed, at)),
            _ => {}
        }
    }
    for r in &remote.runs {
        if r.status != "completed" {
            continue;
        }
        let Some(state) = r.conclusion.as_deref().and_then(ci_state) else {
            continue;
        };
        let Some(at) = parse_iso_ms(&r.updated_at) else {
            continue;
        };
        let mut e = event(
            repo,
            GitKind::Ci,
            at,
            &format!("{}:{}", r.sha, r.id),
            r.name.clone(),
        );
        e.branch = r.branch.clone();
        e.sha = Some(r.sha.clone());
        e.state = Some(state.to_string());
        e.url = Some(r.url.clone());
        out.push(e);
    }
    out
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{ActivityRemote, PullRecord, RunRecord};
    use std::path::PathBuf;

    #[test]
    fn decoration_is_parsed() {
        let d = parse_decoration(
            "HEAD -> refs/heads/feat, tag: refs/tags/v1, refs/remotes/origin/feat, refs/remotes/origin/HEAD, refs/heads/other",
        );
        assert_eq!(d.current.as_deref(), Some("feat"));
        assert_eq!(d.locals, ["feat", "other"]);
        assert_eq!(d.remotes, ["feat"]);
        assert_eq!(parse_decoration(""), Decoration::default());
        assert_eq!(
            source_branch("refs/remotes/origin/a/b").as_deref(),
            Some("a/b")
        );
        assert_eq!(source_branch("refs/tags/v1"), None);
        assert_eq!(source_branch("refs/stash"), None);
    }

    fn rec(sha: &str, parents: &str, time: i64, deco: &str, source: &str, subject: &str) -> String {
        format!("{sha}\x1f{parents}\x1fAda\x1f{time}\x1f{deco}\x1f{source}\x1f{subject}\x1e\n")
    }

    #[test]
    fn log_is_parsed_into_lanes_and_merges() {
        let out = [
            rec(
                "m1",
                "a2 f2",
                400,
                "HEAD -> refs/heads/main, refs/remotes/origin/main",
                "refs/heads/main",
                "Merge feat",
            ),
            rec(
                "f2",
                "f1",
                350,
                "refs/heads/feat",
                "refs/heads/feat",
                "feat two",
            ),
            rec("a2", "a1", 300, "", "refs/heads/main", "main two"),
            rec("f1", "a1", 250, "", "refs/heads/feat", "feat one"),
            rec("a1", "", 100, "", "refs/heads/main", "init"),
            rec(
                "x1",
                "a1",
                90,
                "",
                "refs/heads/gone",
                "orphan branch commit",
            ),
            "garbage".to_string(),
        ]
        .concat();
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 6);
        assert_eq!(commits[0].parents, ["a2", "f2"]);
        assert_eq!(commits[0].subject, "Merge feat");

        let refs = vec![
            RefRow {
                name: "refs/heads/main".into(),
                sha: "m1".into(),
                upstream: Some("refs/remotes/origin/main".into()),
                current: true,
            },
            RefRow {
                name: "refs/heads/feat".into(),
                sha: "f2".into(),
                upstream: None,
                current: false,
            },
            RefRow {
                name: "refs/remotes/origin/main".into(),
                sha: "a2".into(),
                upstream: None,
                current: false,
            },
            RefRow {
                name: "refs/remotes/origin/HEAD".into(),
                sha: "a2".into(),
                upstream: None,
                current: false,
            },
            RefRow {
                name: "refs/remotes/origin/other".into(),
                sha: "a1".into(),
                upstream: None,
                current: false,
            },
        ];
        let tips = branch_tips(&refs, Some("refs/remotes/origin/main"));
        let names: Vec<_> = tips.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["feat", "main", "other"]);
        let main = &tips[1];
        assert!(main.is_default && main.is_current);
        assert_eq!(main.local.as_deref(), Some("m1"));
        assert_eq!(main.remote.as_deref(), Some("a2"));
        assert_eq!(tips[2].local, None);
        assert_eq!(tips[2].remote.as_deref(), Some("a1"));

        let (events, index) = local_events("github.com/o/r", &commits, &tips);
        let lane: Vec<_> = events
            .iter()
            .map(|e| {
                (
                    e.sha.as_deref().unwrap(),
                    e.branch.as_deref().unwrap(),
                    e.kind,
                )
            })
            .collect();
        assert_eq!(
            lane,
            [
                ("m1", "main", GitKind::Merge),
                ("f2", "feat", GitKind::Commit),
                ("a2", "main", GitKind::Commit),
                ("f1", "feat", GitKind::Commit),
                ("a1", "main", GitKind::Commit),
                ("x1", "gone", GitKind::Commit),
            ]
        );
        assert_eq!(events[0].id, "git:merge:m1");
        assert_eq!(events[0].state.as_deref(), Some("main"));
        assert_eq!(events[1].id, "git:commit:f2");
        assert_eq!(events[1].at, 350_000);
        assert_eq!(
            events[1].url.as_deref(),
            Some("https://github.com/o/r/commit/f2")
        );
        assert_eq!(index["a1"], 4);
    }

    #[test]
    fn github_events_cover_prs_and_ci() {
        let pull = |number, state: &str, merged: Option<&str>, closed: Option<&str>| PullRecord {
            number,
            title: format!("PR {number}"),
            state: state.into(),
            url: format!("https://github.com/o/r/pull/{number}"),
            author: "ada".into(),
            head: format!("feat-{number}"),
            base: "main".into(),
            head_sha: format!("sha{number}"),
            created_at: "2026-10-14T01:00:00Z".into(),
            merged_at: merged.map(str::to_string),
            closed_at: closed.map(str::to_string),
        };
        let run = |id, status: &str, conclusion: Option<&str>| RunRecord {
            id,
            name: "CI".into(),
            branch: Some("feat-1".into()),
            sha: "sha1".into(),
            status: status.into(),
            conclusion: conclusion.map(str::to_string),
            url: format!("https://github.com/o/r/actions/runs/{id}"),
            updated_at: "2026-10-14T01:30:00Z".into(),
        };
        let remote = ActivityRemote {
            pulls: vec![
                pull(1, "open", None, None),
                pull(
                    2,
                    "merged",
                    Some("2026-10-14T03:00:00Z"),
                    Some("2026-10-14T03:00:00Z"),
                ),
                pull(3, "closed", None, Some("2026-10-14T04:00:00Z")),
            ],
            runs: vec![
                run(10, "completed", Some("success")),
                run(11, "completed", Some("timed_out")),
                run(12, "completed", Some("skipped")),
                run(13, "in_progress", None),
            ],
        };
        let events = github_events("github.com/o/r", &remote);
        let ids: Vec<_> = events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "git:prOpened:1",
                "git:prOpened:2",
                "git:prMerged:2",
                "git:prOpened:3",
                "git:prClosed:3",
                "git:ci:sha1:10",
                "git:ci:sha1:11",
            ]
        );
        let merged = &events[2];
        assert_eq!(merged.pr, Some(2));
        assert_eq!(merged.state.as_deref(), Some("main"));
        assert_eq!(merged.at, parse_iso_ms("2026-10-14T03:00:00Z").unwrap());
        assert_eq!(events[5].state.as_deref(), Some("success"));
        assert_eq!(events[6].state.as_deref(), Some("failure"));
        assert_eq!(events[6].sha.as_deref(), Some("sha1"));
    }

    #[test]
    fn github_slug_needs_a_github_key() {
        assert_eq!(github_slug("github.com/o/r").as_deref(), Some("o/r"));
        assert_eq!(github_slug("gitlab.com/o/r"), None);
        assert_eq!(github_slug("github.com/o"), None);
        assert_eq!(github_slug("file:/x/y"), None);
    }

    fn git_in(dir: &Path, args: &[&str]) {
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

    fn commit(root: &Path, file: &str, msg: &str) {
        std::fs::write(root.join(file), msg).unwrap();
        git_in(root, &["add", "."]);
        git_in(root, &["commit", "-q", "-m", msg]);
    }

    fn repo_dir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git_in(&root, &["init", "-q", "-b", "main"]);
        (tmp, root)
    }

    #[test]
    fn an_empty_repository_has_no_events() {
        let (_tmp, root) = repo_dir();
        let (events, tips) = collect_local(&root, "k", 50).unwrap();
        assert!(events.is_empty() && tips.is_empty());
    }

    #[test]
    fn local_history_is_collected_from_a_real_repository() {
        let (_tmp, root) = repo_dir();
        commit(&root, "a.txt", "init");
        git_in(&root, &["checkout", "-q", "-b", "feat"]);
        commit(&root, "b.txt", "feat one");
        commit(&root, "c.txt", "feat two");
        git_in(&root, &["checkout", "-q", "main"]);
        commit(&root, "d.txt", "main two");
        git_in(
            &root,
            &["merge", "-q", "--no-ff", "-m", "Merge feat", "feat"],
        );
        // A remote-tracking ref as a fetch or push would leave it.
        let main_sha = git::run(&root, &["rev-parse", "main~1"]).unwrap();
        git_in(
            &root,
            &["update-ref", "refs/remotes/origin/main", &main_sha],
        );
        git_in(&root, &["config", "branch.main.remote", "origin"]);
        git_in(&root, &["config", "branch.main.merge", "refs/heads/main"]);
        git_in(
            &root,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );

        let (events, tips) = collect_local(&root, "github.com/o/r", 50).unwrap();

        let by_title = |t: &str| events.iter().find(|e| e.title == t).unwrap();
        assert_eq!(
            events.iter().filter(|e| e.kind == GitKind::Commit).count(),
            4
        );
        let merge = by_title("Merge feat");
        assert_eq!(merge.kind, GitKind::Merge);
        assert_eq!(merge.parents.len(), 2);
        assert_eq!(merge.branch.as_deref(), Some("main"));
        assert_eq!(by_title("feat two").branch.as_deref(), Some("feat"));
        assert_eq!(by_title("feat one").branch.as_deref(), Some("feat"));
        assert_eq!(by_title("main two").branch.as_deref(), Some("main"));
        assert_eq!(by_title("init").branch.as_deref(), Some("main"));
        assert_eq!(
            merge.id,
            format!("git:merge:{}", merge.sha.as_deref().unwrap())
        );

        let main = tips.iter().find(|t| t.name == "main").unwrap();
        assert!(main.is_default && main.is_current);
        assert_eq!(main.remote.as_deref(), Some(main_sha.as_str()));
        let feat = tips.iter().find(|t| t.name == "feat").unwrap();
        assert!(!feat.is_default && feat.remote.is_none());

        let pushes: Vec<_> = events.iter().filter(|e| e.kind == GitKind::Push).collect();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].id, format!("git:push:main:{main_sha}"));
        assert_eq!(pushes[0].title, "main two");
        assert_eq!(pushes[0].at, by_title("main two").at);

        // The limit counts commits.
        let (events, _) = collect_local(&root, "k", 2).unwrap();
        assert_eq!(events.iter().filter(|e| e.kind != GitKind::Push).count(), 2);
    }
}
