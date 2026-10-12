//! What the work panel shows from GitHub: the current branch's pull request
//! and its checks, open pull requests and open issues; and opening a PR.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::http::{Api, Result};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub number: u64,
    pub title: String,
    /// "open", "closed" or "merged".
    pub state: String,
    pub draft: bool,
    pub url: String,
    pub author: String,
    pub head: String,
    pub base: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub labels: Vec<String>,
    pub comments: u64,
    pub updated_at: String,
}

/// Check runs and commit statuses of one commit, summed up.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub skipped: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    /// `owner/repo`.
    pub slug: String,
    pub url: String,
    pub default_branch: String,
    /// The checked-out branch exists on GitHub.
    pub pushed: bool,
    /// The newest pull request from the checked-out branch (open ones first).
    pub current: Option<Pull>,
    /// Checks of the current pull request's head, or of the pushed branch.
    pub checks: Option<Checks>,
    pub pulls: Vec<Pull>,
    pub issues: Vec<Issue>,
}

// GitHub's JSON, only the fields used.
#[derive(Deserialize)]
struct RawUser {
    login: String,
}

#[derive(Deserialize)]
struct RawRef {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
}

#[derive(Deserialize)]
struct RawPull {
    number: u64,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    merged_at: Option<String>,
    html_url: String,
    user: Option<RawUser>,
    head: RawRef,
    base: RawRef,
    updated_at: String,
}

impl RawPull {
    fn into_pull(self) -> (Pull, String) {
        let state = if self.merged_at.is_some() {
            "merged".into()
        } else {
            self.state
        };
        (
            Pull {
                number: self.number,
                title: self.title,
                state,
                draft: self.draft,
                url: self.html_url,
                author: self.user.map(|u| u.login).unwrap_or_default(),
                head: self.head.name,
                base: self.base.name,
                updated_at: self.updated_at,
            },
            self.head.sha,
        )
    }
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    html_url: String,
    user: Option<RawUser>,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    comments: u64,
    updated_at: String,
    /// Present when the "issue" is a pull request.
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawRepo {
    full_name: String,
    html_url: String,
    default_branch: String,
    /// The upstream when this repository is a fork.
    parent: Option<Box<RawRepo>>,
}

#[derive(Deserialize)]
struct RawCheckRun {
    status: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct RawCheckRuns {
    total_count: u32,
    check_runs: Vec<RawCheckRun>,
}

#[derive(Deserialize)]
struct RawStatus {
    state: String,
}

#[derive(Deserialize)]
struct RawCombined {
    statuses: Vec<RawStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

pub fn account(api: &Api) -> Result<Account> {
    api.get("/user")
}

/// Percent-encodes a ref or branch for a URL path or query.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Like `enc`, but keeps the slashes of a ref path (`git/refs/heads/a/b`).
fn enc_path(s: &str) -> String {
    s.split('/').map(enc).collect::<Vec<_>>().join("/")
}

fn checks(api: &Api, slug: &str, git_ref: &str) -> Result<Option<Checks>> {
    let r = enc(git_ref);
    // Up to 500 runs: big matrices spill past the first page.
    let mut all = Vec::new();
    for page in 1..=5 {
        let runs: Option<RawCheckRuns> = api.get_opt(&format!(
            "/repos/{slug}/commits/{r}/check-runs?per_page=100&page={page}"
        ))?;
        let Some(runs) = runs else { break };
        let total = runs.total_count as usize;
        all.extend(runs.check_runs);
        if all.len() >= total {
            break;
        }
    }
    let statuses: Option<RawCombined> =
        api.get_opt(&format!("/repos/{slug}/commits/{r}/status?per_page=100"))?;
    let mut c = Checks::default();
    for run in all {
        match (run.status.as_str(), run.conclusion.as_deref()) {
            ("completed", Some("success" | "neutral")) => c.passed += 1,
            ("completed", Some("skipped")) => c.skipped += 1,
            ("completed", _) => c.failed += 1,
            _ => c.pending += 1,
        }
    }
    for status in statuses.map(|s| s.statuses).unwrap_or_default() {
        match status.state.as_str() {
            "success" => c.passed += 1,
            "pending" => c.pending += 1,
            _ => c.failed += 1,
        }
    }
    Ok((c != Checks::default()).then_some(c))
}

/// The repository PRs go to: the upstream of a fork, else `origin` itself.
/// Returns (base repo, owner of the branch's repository).
fn target(api: &Api, slug: &str) -> Result<(RawRepo, String)> {
    let repo: RawRepo = api.get(&format!("/repos/{slug}"))?;
    let owner = repo
        .full_name
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    Ok(match repo.parent {
        Some(parent) => (*parent, owner),
        None => (repo, owner),
    })
}

/// Everything for `slug` (`origin`, `owner/repo`) with `branch` checked out.
/// When `origin` is a fork, PRs and issues come from its upstream, and the
/// branch's PR is the one from the fork.
pub fn repo_view(api: &Api, slug: &str, branch: Option<&str>) -> Result<RepoView> {
    let (base, owner) = target(api, slug)?;
    let base_slug = base.full_name.clone();

    let pulls: Vec<RawPull> = api.get(&format!(
        "/repos/{base_slug}/pulls?state=open&sort=updated&direction=desc&per_page=30"
    ))?;
    let pulls: Vec<Pull> = pulls.into_iter().map(|p| p.into_pull().0).collect();

    // `/issues` lists pull requests too; ask for more and keep 30 issues.
    let issues: Vec<RawIssue> = api.get(&format!(
        "/repos/{base_slug}/issues?state=open&sort=updated&direction=desc&per_page=100"
    ))?;
    let issues = issues
        .into_iter()
        .filter(|i| i.pull_request.is_none())
        .take(30)
        .map(|i| Issue {
            number: i.number,
            title: i.title,
            url: i.html_url,
            author: i.user.map(|u| u.login).unwrap_or_default(),
            labels: i.labels.into_iter().map(|l| l.name).collect(),
            comments: i.comments,
            updated_at: i.updated_at,
        })
        .collect();

    let (mut pushed, mut current, mut check) = (false, None, None);
    if let Some(branch) = branch {
        pushed = api
            .get_opt::<serde_json::Value>(&format!("/repos/{slug}/branches/{}", enc(branch)))?
            .is_some();
        let mut mine: Vec<RawPull> = api.get(&format!(
            "/repos/{base_slug}/pulls?state=all&head={}&sort=updated&direction=desc&per_page=10",
            enc(&format!("{owner}:{branch}"))
        ))?;
        // An open one wins over older closed or merged ones.
        mine.sort_by_key(|p| p.state != "open");
        if let Some(pull) = mine.into_iter().next() {
            let (pull, sha) = pull.into_pull();
            check = checks(api, &base_slug, &sha)?;
            current = Some(pull);
        } else if pushed {
            check = checks(api, slug, branch)?;
        }
    }

    Ok(RepoView {
        slug: base_slug,
        url: base.html_url,
        default_branch: base.default_branch,
        pushed,
        current,
        checks: check,
        pulls,
        issues,
    })
}

pub fn create_pull(
    api: &Api,
    slug: &str,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> Result<Pull> {
    if title.trim().is_empty() {
        return Err(crate::tr!("PR 제목을 입력해 주세요", "Enter a PR title").into());
    }
    if api
        .get_opt::<serde_json::Value>(&format!("/repos/{slug}/branches/{}", enc(head)))?
        .is_none()
    {
        return Err(crate::tr!(
            "'{head}' 브랜치가 GitHub에 없어요. 먼저 push 해 주세요",
            "Branch '{head}' isn't on GitHub. Push it first"
        )
        .into());
    }
    let (target_repo, owner) = target(api, slug)?;
    // From a fork, the head names the fork's owner.
    let head_ref = if target_repo.full_name.eq_ignore_ascii_case(slug) {
        head.to_string()
    } else {
        format!("{owner}:{head}")
    };
    let raw: RawPull = api.post(
        &format!("/repos/{}/pulls", target_repo.full_name),
        &json!({ "title": title.trim(), "head": head_ref, "base": base, "body": body, "draft": draft }),
    )?;
    Ok(raw.into_pull().0)
}

/// A repository the signed-in account can work on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    /// `owner/repo`.
    pub slug: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub default_branch: String,
    pub url: String,
    pub pushed_at: Option<String>,
}

#[derive(Deserialize)]
struct RawRepoSummary {
    full_name: String,
    description: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    fork: bool,
    default_branch: String,
    html_url: String,
    pushed_at: Option<String>,
    #[serde(default)]
    archived: bool,
}

/// The account's repositories (own, collaborator and organization ones),
/// recently pushed first. Archived ones are left out.
pub fn list_repos(api: &Api) -> Result<Vec<RepoSummary>> {
    let mut repos = Vec::new();
    for page in 1..=3 {
        let batch: Vec<RawRepoSummary> = api.get(&format!(
            "/user/repos?affiliation=owner,collaborator,organization_member&sort=pushed&per_page=100&page={page}"
        ))?;
        let full = batch.len() == 100;
        repos.extend(
            batch
                .into_iter()
                .filter(|r| !r.archived)
                .map(|r| RepoSummary {
                    slug: r.full_name,
                    description: r.description.filter(|d| !d.trim().is_empty()),
                    private: r.private,
                    fork: r.fork,
                    default_branch: r.default_branch,
                    url: r.html_url,
                    pushed_at: r.pushed_at,
                }),
        );
        if !full {
            break;
        }
    }
    Ok(repos)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranch {
    pub name: String,
    pub protected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    /// The first line of the message.
    pub subject: String,
    pub author: String,
    pub date: String,
    pub url: String,
}

/// A repository seen on GitHub only (not cloned): its branches, the chosen
/// branch's recent commits, PR and checks, and open PRs and issues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteView {
    /// `owner/repo` as asked (the view's own slug is the upstream for a fork).
    pub origin: String,
    pub branch: String,
    pub branches: Vec<RemoteBranch>,
    pub commits: Vec<Commit>,
    pub view: RepoView,
}

#[derive(Deserialize)]
struct RawBranch {
    name: String,
    #[serde(default)]
    protected: bool,
}

#[derive(Deserialize)]
struct RawCommitPerson {
    name: Option<String>,
    date: Option<String>,
}

#[derive(Deserialize)]
struct RawCommitDetail {
    message: String,
    author: Option<RawCommitPerson>,
}

#[derive(Deserialize)]
struct RawCommit {
    sha: String,
    html_url: String,
    commit: RawCommitDetail,
    author: Option<RawUser>,
}

/// `branch` defaults to the repository's default branch.
pub fn remote_view(api: &Api, slug: &str, branch: Option<&str>) -> Result<RemoteView> {
    let repo: RawRepo = api.get(&format!("/repos/{slug}"))?;
    let branch = branch
        .map(str::to_string)
        .unwrap_or_else(|| repo.default_branch.clone());
    let mut branches = Vec::new();
    for page in 1..=3 {
        let batch: Vec<RawBranch> =
            api.get(&format!("/repos/{slug}/branches?per_page=100&page={page}"))?;
        let full = batch.len() == 100;
        branches.extend(batch.into_iter().map(|b| RemoteBranch {
            name: b.name,
            protected: b.protected,
        }));
        if !full {
            break;
        }
    }
    // The default branch first, the rest by name.
    branches.sort_by(|a, b| {
        (a.name != repo.default_branch, &a.name).cmp(&(b.name != repo.default_branch, &b.name))
    });
    let commits: Vec<RawCommit> = api
        .get_opt(&format!(
            "/repos/{slug}/commits?sha={}&per_page=15",
            enc(&branch)
        ))?
        // An empty repository has no commits (409) or branch (404).
        .unwrap_or_default();
    let commits = commits
        .into_iter()
        .map(|c| {
            let author = c.commit.author.as_ref();
            Commit {
                sha: c.sha.chars().take(7).collect(),
                subject: c
                    .commit
                    .message
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                author: c
                    .author
                    .map(|u| u.login)
                    .or_else(|| author.and_then(|a| a.name.clone()))
                    .unwrap_or_default(),
                date: author.and_then(|a| a.date.clone()).unwrap_or_default(),
                url: c.html_url,
            }
        })
        .collect();
    let view = repo_view(api, slug, Some(&branch))?;
    Ok(RemoteView {
        origin: repo.full_name,
        branch,
        branches,
        commits,
        view,
    })
}

#[derive(Deserialize)]
struct RawGitRef {
    object: RawGitObject,
}

#[derive(Deserialize)]
struct RawGitObject {
    sha: String,
}

/// Creates `name` on GitHub at the tip of `from`.
pub fn create_branch(api: &Api, slug: &str, name: &str, from: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::tr!("브랜치 이름을 입력해 주세요", "Enter a branch name").into());
    }
    let from_ref: RawGitRef = api
        .get_opt(&format!("/repos/{slug}/git/ref/heads/{}", enc_path(from)))?
        .ok_or_else(|| {
            crate::tr!(
                "'{from}' 브랜치를 GitHub에서 찾지 못했어요",
                "Couldn't find branch '{from}' on GitHub"
            )
        })?;
    let _: serde_json::Value = api.post(
        &format!("/repos/{slug}/git/refs"),
        &json!({ "ref": format!("refs/heads/{name}"), "sha": from_ref.object.sha }),
    )?;
    Ok(())
}

/// Deletes branch `name` on GitHub (never the default branch).
pub fn delete_branch(api: &Api, slug: &str, name: &str) -> Result<()> {
    let repo: RawRepo = api.get(&format!("/repos/{slug}"))?;
    if repo.default_branch == name {
        return Err(crate::tr!(
            "기본 브랜치는 지울 수 없어요",
            "The default branch can't be deleted"
        )
        .into());
    }
    api.delete(&format!("/repos/{slug}/git/refs/heads/{}", enc_path(name)))
}

/// A file in a branch's tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeFile {
    pub path: String,
    /// Blob id, to read the file.
    pub sha: String,
    /// `100644`, or `100755` for an executable.
    pub mode: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTree {
    /// The branch's commit the tree belongs to; a commit is made on top of it.
    pub head: String,
    pub files: Vec<TreeFile>,
    /// GitHub cut the listing (a very large repository).
    pub truncated: bool,
}

#[derive(Deserialize)]
struct RawBranchHead {
    commit: RawBranchCommit,
}

#[derive(Deserialize)]
struct RawBranchCommit {
    sha: String,
    commit: RawCommitTree,
}

#[derive(Deserialize)]
struct RawCommitTree {
    tree: RawGitObject,
}

#[derive(Deserialize)]
struct RawTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    mode: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct RawTree {
    tree: Vec<RawTreeEntry>,
    #[serde(default)]
    truncated: bool,
}

fn branch_head(api: &Api, slug: &str, branch: &str) -> Result<RawBranchHead> {
    api.get(&format!("/repos/{slug}/branches/{}", enc(branch)))
}

pub fn remote_tree(api: &Api, slug: &str, branch: &str) -> Result<RemoteTree> {
    let head = branch_head(api, slug, branch)?;
    let tree: RawTree = api.get(&format!(
        "/repos/{slug}/git/trees/{}?recursive=1",
        head.commit.commit.tree.sha
    ))?;
    Ok(RemoteTree {
        head: head.commit.sha,
        truncated: tree.truncated,
        files: tree
            .tree
            .into_iter()
            .filter(|e| e.kind == "blob")
            .map(|e| TreeFile {
                path: e.path,
                sha: e.sha,
                mode: e.mode,
                size: e.size,
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct RawBlob {
    content: String,
    encoding: String,
}

/// A file's bytes by blob id (base64 from GitHub).
pub fn blob(api: &Api, slug: &str, sha: &str) -> Result<Vec<u8>> {
    let blob: RawBlob = api.get(&format!("/repos/{slug}/git/blobs/{sha}"))?;
    if blob.encoding != "base64" {
        return Ok(blob.content.into_bytes());
    }
    super::base64_decode(&blob.content).ok_or_else(|| {
        crate::tr!(
            "GitHub에서 받은 파일을 읽지 못했어요",
            "Couldn't read the file from GitHub"
        )
        .into()
    })
}

/// One file in a commit: new text, or `None` to delete it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub text: Option<String>,
    /// Kept from the old file; `100644` for a new one.
    pub mode: Option<String>,
}

#[derive(Deserialize)]
struct RawSha {
    sha: String,
}

/// Commits `changes` on top of `expected_head` (the commit the edits were
/// made against), to `branch` or to a new branch `new_branch` made there.
/// Returns the new commit. Refused when `branch` moved meanwhile.
pub fn commit_files(
    api: &Api,
    slug: &str,
    branch: &str,
    expected_head: &str,
    changes: &[FileChange],
    message: &str,
    new_branch: Option<&str>,
) -> Result<String> {
    if message.trim().is_empty() {
        return Err(crate::tr!("커밋 메시지를 입력해 주세요", "Enter a commit message").into());
    }
    if changes.is_empty() {
        return Err(crate::tr!("커밋할 변경이 없어요", "There's nothing to commit").into());
    }
    let new_branch = new_branch.map(str::trim).filter(|b| !b.is_empty());
    let head = branch_head(api, slug, branch)?;
    if head.commit.sha != expected_head && new_branch.is_none() {
        return Err(crate::tr!(
            "그사이 '{branch}' 브랜치에 새 커밋이 생겼어요. 파일을 다시 불러온 뒤 커밋하거나 새 브랜치로 커밋해 주세요",
            "Branch '{branch}' got new commits meanwhile. Reload the files before committing, or commit to a new branch"
        )
        .into());
    }
    let entries: Vec<serde_json::Value> = changes
        .iter()
        .map(|c| {
            let mode = c.mode.clone().unwrap_or_else(|| "100644".into());
            match &c.text {
                Some(text) => {
                    json!({ "path": c.path, "mode": mode, "type": "blob", "content": text })
                }
                None => json!({ "path": c.path, "mode": mode, "type": "blob", "sha": null }),
            }
        })
        .collect();
    // Built on the commit the edits saw, so nothing in between is overwritten.
    let base: RawCommitTree = api
        .get::<RawCommitDetailTree>(&format!("/repos/{slug}/git/commits/{expected_head}"))?
        .into();
    let tree: RawSha = api.post(
        &format!("/repos/{slug}/git/trees"),
        &json!({ "base_tree": base.tree.sha, "tree": entries }),
    )?;
    let commit: RawSha = api.post(
        &format!("/repos/{slug}/git/commits"),
        &json!({ "message": message.trim(), "tree": tree.sha, "parents": [expected_head] }),
    )?;
    match new_branch {
        Some(name) => {
            let _: serde_json::Value = api.post(
                &format!("/repos/{slug}/git/refs"),
                &json!({ "ref": format!("refs/heads/{name}"), "sha": commit.sha }),
            )?;
        }
        None => {
            let _: serde_json::Value = api.patch(
                &format!("/repos/{slug}/git/refs/heads/{}", enc_path(branch)),
                &json!({ "sha": commit.sha, "force": false }),
            )?;
        }
    }
    Ok(commit.sha)
}

#[derive(Deserialize)]
struct RawCommitDetailTree {
    tree: RawGitObject,
}

impl From<RawCommitDetailTree> for RawCommitTree {
    fn from(c: RawCommitDetailTree) -> Self {
        RawCommitTree { tree: c.tree }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::auth::tests::serve;

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into()
    }

    #[test]
    fn repo_view_combines_pulls_issues_and_checks() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"full_name":"o/r","html_url":"https://github.com/o/r","default_branch":"main"}"#,
            ),
            (
                200,
                r#"[{"number":3,"title":"Open","state":"open","draft":true,"merged_at":null,"html_url":"u3","user":{"login":"a"},"head":{"ref":"feat","sha":"s3"},"base":{"ref":"main","sha":"b"},"updated_at":"t"}]"#,
            ),
            (
                200,
                r#"[{"number":9,"title":"Bug","html_url":"u9","user":{"login":"b"},"labels":[{"name":"bug"}],"comments":2,"updated_at":"t"},{"number":3,"title":"Open","html_url":"u3","updated_at":"t","pull_request":{}}]"#,
            ),
            (200, r#"{"name":"feat/x"}"#),
            (
                200,
                r#"[{"number":2,"title":"Old","state":"closed","merged_at":"m","html_url":"u2","user":{"login":"a"},"head":{"ref":"feat/x","sha":"s2"},"base":{"ref":"main","sha":"b"},"updated_at":"t"}]"#,
            ),
            (
                200,
                r#"{"total_count":4,"check_runs":[{"status":"completed","conclusion":"success"},{"status":"completed","conclusion":"failure"},{"status":"in_progress","conclusion":null},{"status":"completed","conclusion":"skipped"}]}"#,
            ),
            (200, r#"{"statuses":[{"state":"success"}]}"#),
        ]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let view = repo_view(&api, "o/r", Some("feat/x")).unwrap();
        assert_eq!(view.default_branch, "main");
        assert!(view.pushed);
        assert_eq!(view.pulls.len(), 1);
        assert!(view.pulls[0].draft);
        assert_eq!(view.issues.len(), 1);
        assert_eq!(view.issues[0].labels, ["bug"]);
        let current = view.current.unwrap();
        assert_eq!((current.number, current.state.as_str()), (2, "merged"));
        assert_eq!(
            view.checks,
            Some(Checks {
                passed: 2,
                failed: 1,
                pending: 1,
                skipped: 1
            })
        );
        let seen = seen.lock().unwrap();
        assert!(seen[3].contains("/branches/feat%2Fx"), "{}", seen[3]);
        assert!(seen[4].contains("head=o%3Afeat%2Fx"), "{}", seen[4]);
        assert!(seen[5].contains("/commits/s2/check-runs"), "{}", seen[5]);
    }

    #[test]
    fn a_fork_targets_its_upstream() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"full_name":"me/r","html_url":"h","default_branch":"main","parent":{"full_name":"up/r","html_url":"u","default_branch":"dev"}}"#,
            ),
            (200, "[]"),
            (200, "[]"),
            (200, "{}"),
            (200, "[]"),
            (200, r#"{"total_count":0,"check_runs":[]}"#),
            (200, r#"{"statuses":[]}"#),
        ]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let view = repo_view(&api, "me/r", Some("fix")).unwrap();
        assert_eq!(
            (view.slug.as_str(), view.default_branch.as_str()),
            ("up/r", "dev")
        );
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains("/repos/up/r/pulls"), "{}", seen[1]);
        assert!(seen[3].contains("/repos/me/r/branches/fix"), "{}", seen[3]);
        assert!(
            seen[4].contains("/repos/up/r/pulls?state=all&head=me%3Afix"),
            "{}",
            seen[4]
        );
        assert!(
            seen[5].contains("/repos/me/r/commits/fix/check-runs"),
            "{}",
            seen[5]
        );
    }

    #[test]
    fn a_pull_needs_a_pushed_branch() {
        let (base, _) = serve(vec![(404, r#"{"message":"Branch not found"}"#)]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let err =
            String::from(create_pull(&api, "o/r", "feat", "main", "Title", "", false).unwrap_err());
        assert!(err.contains("push"), "{err}");
        assert!(create_pull(&api, "o/r", "feat", "main", " ", "", false).is_err());
    }

    #[test]
    fn list_repos_skips_archived_ones() {
        let (base, seen) = serve(vec![(
            200,
            r#"[{"full_name":"me/a","description":"","private":true,"fork":false,"default_branch":"main","html_url":"ha","pushed_at":"p"},{"full_name":"me/old","default_branch":"main","html_url":"ho","archived":true}]"#,
        )]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let repos = list_repos(&api).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].slug, "me/a");
        assert!(repos[0].private);
        assert_eq!(repos[0].description, None);
        assert!(seen.lock().unwrap()[0].contains("/user/repos?affiliation="));
    }

    #[test]
    fn remote_view_lists_branches_default_first_and_commits() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"full_name":"me/r","html_url":"h","default_branch":"main"}"#,
            ),
            (
                200,
                r#"[{"name":"a-feature"},{"name":"main","protected":true},{"name":"b/fix"}]"#,
            ),
            (
                200,
                r#"[{"sha":"0123456789","html_url":"c1","commit":{"message":"Fix it\n\nbody","author":{"name":"Me","date":"2026-10-11T00:00:00Z"}},"author":{"login":"me"}}]"#,
            ),
            // repo_view: repo, pulls, issues, branch, branch PRs, checks, statuses
            (
                200,
                r#"{"full_name":"me/r","html_url":"h","default_branch":"main"}"#,
            ),
            (200, "[]"),
            (200, "[]"),
            (200, r#"{"name":"main"}"#),
            (200, "[]"),
            (200, r#"{"total_count":0,"check_runs":[]}"#),
            (200, r#"{"statuses":[]}"#),
        ]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let remote = remote_view(&api, "me/r", None).unwrap();
        assert_eq!(remote.branch, "main");
        let names: Vec<_> = remote.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["main", "a-feature", "b/fix"]);
        assert!(remote.branches[0].protected);
        assert_eq!(remote.commits[0].sha, "0123456");
        assert_eq!(remote.commits[0].subject, "Fix it");
        assert_eq!(remote.commits[0].author, "me");
        assert!(remote.view.pushed);
        assert!(seen.lock().unwrap()[2].contains("/commits?sha=main"));
    }

    #[test]
    fn branches_are_created_from_a_tip_and_deleted_by_ref() {
        let (base, seen) = serve(vec![
            (200, r#"{"object":{"sha":"abc"}}"#),
            (201, r#"{"ref":"refs/heads/feat/x"}"#),
            (
                200,
                r#"{"full_name":"me/r","html_url":"h","default_branch":"main"}"#,
            ),
            (204, ""),
            (
                200,
                r#"{"full_name":"me/r","html_url":"h","default_branch":"main"}"#,
            ),
        ]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        create_branch(&api, "me/r", " feat/x ", "dev/1").unwrap();
        delete_branch(&api, "me/r", "feat/x").unwrap();
        assert!(delete_branch(&api, "me/r", "main").is_err());
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("/git/ref/heads/dev/1"), "{}", seen[0]);
        assert!(
            seen[1].contains("refs/heads/feat/x") && seen[1].contains("abc"),
            "{}",
            seen[1]
        );
        assert!(
            seen[3].starts_with("DELETE /repos/me/r/git/refs/heads/feat/x"),
            "{}",
            seen[3]
        );
    }

    #[test]
    fn files_are_committed_on_the_seen_head() {
        let branch = r#"{"commit":{"sha":"h1","commit":{"tree":{"sha":"t1"}}}}"#;
        let (base, seen) = serve(vec![
            // tree
            (200, branch),
            (
                200,
                r#"{"tree":[{"path":"a.md","type":"blob","sha":"b1","mode":"100644","size":3},{"path":"src","type":"tree","sha":"x","mode":"040000"}],"truncated":false}"#,
            ),
            // commit on the same head
            (200, branch),
            (200, r#"{"tree":{"sha":"t1"}}"#),
            (201, r#"{"sha":"t2"}"#),
            (201, r#"{"sha":"c2"}"#),
            (200, r#"{"ref":"refs/heads/main"}"#),
            // the branch moved: refused before any write
            (
                200,
                r#"{"commit":{"sha":"h9","commit":{"tree":{"sha":"t9"}}}}"#,
            ),
        ]);
        let agent = agent();
        let api = Api {
            agent: &agent,
            base: &base,
            token: "t",
        };
        let tree = remote_tree(&api, "me/r", "main").unwrap();
        assert_eq!(tree.head, "h1");
        assert_eq!(tree.files.len(), 1);
        assert_eq!(tree.files[0].path, "a.md");

        let changes = [
            FileChange {
                path: "a.md".into(),
                text: Some("new".into()),
                mode: Some("100644".into()),
            },
            FileChange {
                path: "old.txt".into(),
                text: None,
                mode: None,
            },
        ];
        let sha = commit_files(&api, "me/r", "main", "h1", &changes, "Edit", None).unwrap();
        assert_eq!(sha, "c2");
        assert!(commit_files(&api, "me/r", "main", "h1", &changes, "Edit", None).is_err());

        let seen: Vec<String> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.split_whitespace().collect::<String>())
            .collect();
        assert!(seen[1].contains("/git/trees/t1?recursive=1"), "{}", seen[1]);
        assert!(
            seen[4].contains("base_tree") && seen[4].contains("\"sha\":null"),
            "{}",
            seen[4]
        );
        assert!(seen[5].contains("\"parents\":[\"h1\"]"), "{}", seen[5]);
        assert!(
            seen[6].starts_with("PATCH/repos/me/r/git/refs/heads/main"),
            "{}",
            seen[6]
        );
    }
}
