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
}
