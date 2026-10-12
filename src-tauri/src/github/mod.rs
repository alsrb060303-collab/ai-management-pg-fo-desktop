//! GitHub in the work panel: sign-in (OAuth device flow), the current
//! branch's pull request and checks, open pull requests and issues, and
//! opening a pull request. Reviews, merges and comments stay on github.com.

mod api;
mod auth;
mod http;

#[cfg(test)]
pub(crate) use auth::tests::serve as test_serve;

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ureq::Agent;

use api::{Account, Pull, RemoteView, RepoSummary, RepoView};
use auth::{DeviceCode, Outcome, TokenStore};
use http::{Api, Endpoints};

/// The OAuth app's client ID (public; the device flow needs no secret).
/// `AIM_GITHUB_CLIENT_ID` overrides it, e.g. for a test app.
const CLIENT_ID: &str = "Ov23liLkXnOqA1pmberI";

pub const AUTH_EVENT: &str = "github-auth";

fn client_id() -> Option<String> {
    std::env::var("AIM_GITHUB_CLIENT_ID")
        .ok()
        .or_else(|| Some(CLIENT_ID.to_string()))
        .filter(|id| !id.trim().is_empty())
}

pub struct GitHub {
    agent: Agent,
    ends: Endpoints,
    store: Box<dyn TokenStore>,
    /// `None` until read from the store.
    token: Mutex<Option<Option<String>>>,
    account: Mutex<Option<Account>>,
    /// Bumped by every sign-in start and cancel; a poll with an older value stops.
    login: AtomicU64,
    /// Serializes token changes against the generation and stale-401 checks.
    write: Mutex<()>,
}

impl GitHub {
    pub fn manage(app: &AppHandle) {
        app.manage(GitHub {
            agent: http::agent(),
            ends: Endpoints::default(),
            store: Box::new(auth::Keyring),
            token: Mutex::new(None),
            account: Mutex::new(None),
            login: AtomicU64::new(0),
            write: Mutex::new(()),
        });
    }

    fn token(&self) -> Result<Option<String>, String> {
        let mut cached = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = Some(self.store.load().map_err(String::from)?);
        }
        Ok(cached.clone().flatten())
    }

    fn set_token(&self, token: Option<String>) -> Result<(), String> {
        match &token {
            Some(t) => self.store.save(t).map_err(String::from)?,
            None => self.store.clear().map_err(String::from)?,
        }
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token);
        *self.account.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    fn api<'a>(&'a self, token: &'a str) -> Api<'a> {
        Api {
            agent: &self.agent,
            base: &self.ends.api,
            token,
        }
    }

    /// Runs an API call; an expired or revoked token signs out.
    fn call<T>(&self, app: &AppHandle, f: impl FnOnce(&Api) -> http::Result<T>) -> http::Result<T> {
        let token = self
            .token()?
            .ok_or_else(|| crate::tr!("GitHub에 로그인해 주세요", "Please sign in to GitHub"))?;
        let result = f(&self.api(&token));
        if matches!(result, Err(http::Error::Unauthorized)) {
            // Only the token that failed: a newer sign-in may have replaced it.
            let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
            if self.token().ok().flatten().as_deref() == Some(token.as_str()) {
                let _ = self.set_token(None);
                let _ = app.emit(AUTH_EVENT, AuthEvent::signed_out());
            }
        }
        result
    }
}

/// The signed-in GitHub token, for sync to sign in to Firebase with.
pub(crate) fn current_token(app: &AppHandle) -> Option<String> {
    app.try_state::<GitHub>()?.token().ok().flatten()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The app has a client ID (sign-in is possible).
    pub configured: bool,
    pub account: Option<Account>,
    /// Signed in, but GitHub could not be reached to confirm it.
    pub offline: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEvent {
    signed_in: bool,
    /// Why a sign-in ended without one.
    message: Option<String>,
}

impl AuthEvent {
    fn signed_out() -> Self {
        AuthEvent {
            signed_in: false,
            message: None,
        }
    }
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &GitHub) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app, app.state::<GitHub>().inner()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn github_status(app: AppHandle) -> Result<Status, String> {
    blocking(app, |app, gh| {
        let configured = client_id().is_some();
        // A credential store that cannot be read counts as signed out; signing
        // in then reports why the token cannot be kept.
        if gh.token().ok().flatten().is_none() {
            return Ok(Status {
                configured,
                account: None,
                offline: false,
            });
        }
        if let Some(account) = gh.account.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(Status {
                configured,
                account: Some(account),
                offline: false,
            });
        }
        match gh.call(app, api::account) {
            Ok(account) => {
                *gh.account.lock().unwrap_or_else(|e| e.into_inner()) = Some(account.clone());
                Ok(Status {
                    configured,
                    account: Some(account),
                    offline: false,
                })
            }
            Err(http::Error::Unauthorized) => Ok(Status {
                configured,
                account: None,
                offline: false,
            }),
            // Signed in, but GitHub is out of reach: not a reason to ask
            // for a new sign-in; the panel retries.
            Err(_) => Ok(Status {
                configured,
                account: None,
                offline: true,
            }),
        }
    })
    .await
}

#[tauri::command]
pub async fn github_login_start(app: AppHandle) -> Result<DeviceCode, String> {
    blocking(app, |app, gh| {
        let client = client_id().ok_or_else(|| {
            crate::tr!(
                "GitHub 앱 정보(Client ID)가 아직 설정되지 않았어요",
                "The GitHub app (client ID) isn't set up yet"
            )
        })?;
        let generation = gh.login.fetch_add(1, Ordering::SeqCst) + 1;
        let code = auth::start(&gh.agent, &gh.ends, &client)?;
        let app = app.clone();
        let pending = code.clone();
        std::thread::spawn(move || {
            let gh = app.state::<GitHub>();
            let outcome = auth::poll(
                &gh.agent,
                &gh.ends,
                &client,
                &pending,
                &gh.login,
                generation,
                Duration::from_secs(1),
            );
            let event = match outcome {
                Ok(Outcome::Cancelled) => return,
                Ok(Outcome::Token(token)) => {
                    // A cancel or newer sign-in since the poll returned wins.
                    let saved = {
                        let _guard = gh.write.lock().unwrap_or_else(|e| e.into_inner());
                        if gh.login.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        gh.set_token(Some(token))
                    };
                    match saved {
                        Ok(()) => AuthEvent {
                            signed_in: true,
                            message: None,
                        },
                        Err(e) => AuthEvent {
                            signed_in: false,
                            message: Some(e),
                        },
                    }
                }
                Ok(Outcome::Denied) => AuthEvent {
                    signed_in: false,
                    message: Some(crate::tr!(
                        "GitHub에서 로그인을 거절했어요",
                        "GitHub declined the sign-in"
                    )),
                },
                Ok(Outcome::Expired) => AuthEvent {
                    signed_in: false,
                    message: Some(crate::tr!(
                        "코드가 만료됐어요. 다시 로그인해 주세요",
                        "The code expired. Please sign in again"
                    )),
                },
                Err(e) => AuthEvent {
                    signed_in: false,
                    message: Some(e.into()),
                },
            };
            let _ = app.emit(AUTH_EVENT, event);
        });
        Ok(code)
    })
    .await
}

#[tauri::command]
pub fn github_login_cancel(app: AppHandle) {
    let gh = app.state::<GitHub>();
    let _guard = gh.write.lock().unwrap_or_else(|e| e.into_inner());
    gh.login.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
pub async fn github_logout(app: AppHandle) -> Result<(), String> {
    blocking(app, |app, gh| {
        let _guard = gh.write.lock().unwrap_or_else(|e| e.into_inner());
        gh.login.fetch_add(1, Ordering::SeqCst);
        gh.set_token(None)?;
        let _ = app.emit(AUTH_EVENT, AuthEvent::signed_out());
        Ok(())
    })
    .await
}

/// `owner/repo` and the checked-out branch, if the project is on github.com.
fn github_repo_of(project: &str) -> Result<Option<(String, Option<String>)>, String> {
    let info = crate::memo::repo::inspect(Path::new(project))?;
    Ok(info
        .key
        .strip_prefix("github.com/")
        .filter(|slug| slug.split('/').count() == 2)
        .map(|slug| (slug.to_string(), info.branch)))
}

/// `None` when the project's remote is not on github.com.
#[tauri::command]
pub async fn github_repo(app: AppHandle, project: String) -> Result<Option<RepoView>, String> {
    blocking(app, move |app, gh| {
        let Some((slug, branch)) = github_repo_of(&project)? else {
            return Ok(None);
        };
        gh.call(app, |api| api::repo_view(api, &slug, branch.as_deref()))
            .map(Some)
            .map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn github_create_pr(
    app: AppHandle,
    project: String,
    base: String,
    title: String,
    body: String,
    draft: bool,
) -> Result<Pull, String> {
    blocking(app, move |app, gh| {
        let (slug, branch) = github_repo_of(&project)?
            .ok_or_else(|| crate::tr!("GitHub 저장소가 아니에요", "Not a GitHub repository"))?;
        let head = branch.ok_or_else(|| {
            crate::tr!(
                "브랜치를 체크아웃한 상태에서 PR을 만들 수 있어요",
                "Check out a branch to open a PR"
            )
        })?;
        if head == base {
            return Err(crate::tr!(
                "기준 브랜치와 다른 브랜치에서 PR을 만들어 주세요",
                "Open the PR from a branch other than its base"
            ));
        }
        gh.call(app, |api| {
            api::create_pull(api, &slug, &head, &base, &title, &body, draft)
        })
        .map_err(String::from)
    })
    .await
}

/// `owner/repo` with GitHub's allowed characters only (it goes into URLs and
/// a clone target folder).
fn check_slug(slug: &str) -> Result<(), String> {
    let ok = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match slug.split_once('/') {
        Some((owner, repo)) if ok(owner) && ok(repo) => Ok(()),
        _ => Err(crate::tr!(
            "GitHub 저장소 이름이 올바르지 않아요: {slug}",
            "Not a valid GitHub repository name: {slug}"
        )),
    }
}

/// The signed-in account's repositories.
#[tauri::command]
pub async fn github_repos(app: AppHandle) -> Result<Vec<RepoSummary>, String> {
    blocking(app, |app, gh| {
        gh.call(app, api::list_repos).map_err(String::from)
    })
    .await
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalClone {
    /// `owner/repo`, lowercase.
    pub slug: String,
    pub path: String,
}

/// Which of `paths` are clones of github.com repositories.
#[tauri::command]
pub async fn github_local_clones(paths: Vec<String>) -> Vec<LocalClone> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .filter_map(|path| {
                let info = crate::memo::repo::inspect(Path::new(&path)).ok()?;
                let slug = info.key.strip_prefix("github.com/")?;
                (slug.split('/').count() == 2).then(|| LocalClone {
                    slug: slug.to_lowercase(),
                    path,
                })
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// A repository seen on GitHub without a clone; `branch` defaults to the
/// default branch.
#[tauri::command]
pub async fn github_remote(
    app: AppHandle,
    slug: String,
    branch: Option<String>,
) -> Result<RemoteView, String> {
    check_slug(&slug)?;
    blocking(app, move |app, gh| {
        gh.call(app, |api| api::remote_view(api, &slug, branch.as_deref()))
            .map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn github_remote_create_branch(
    app: AppHandle,
    slug: String,
    name: String,
    from: String,
) -> Result<(), String> {
    check_slug(&slug)?;
    blocking(app, move |app, gh| {
        gh.call(app, |api| api::create_branch(api, &slug, &name, &from))
            .map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn github_remote_delete_branch(
    app: AppHandle,
    slug: String,
    name: String,
) -> Result<(), String> {
    check_slug(&slug)?;
    blocking(app, move |app, gh| {
        gh.call(app, |api| api::delete_branch(api, &slug, &name))
            .map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn github_remote_create_pr(
    app: AppHandle,
    slug: String,
    head: String,
    base: String,
    title: String,
    body: String,
    draft: bool,
) -> Result<Pull, String> {
    check_slug(&slug)?;
    if head == base {
        return Err(crate::tr!(
            "기준 브랜치와 다른 브랜치에서 PR을 만들어 주세요",
            "Open the PR from a branch other than its base"
        ));
    }
    blocking(app, move |app, gh| {
        gh.call(app, |api| {
            api::create_pull(api, &slug, &head, &base, &title, &body, draft)
        })
        .map_err(String::from)
    })
    .await
}

/// Clones `slug` into a new folder under `parent` and returns its path. The
/// sign-in token is handed to git for this one command only (an
/// environment-only config, not written to the clone); later fetches and
/// pushes use the user's own git credentials.
#[tauri::command]
pub async fn github_clone(app: AppHandle, slug: String, parent: String) -> Result<String, String> {
    check_slug(&slug)?;
    blocking(app, move |_, gh| {
        let token = gh
            .token()?
            .ok_or_else(|| crate::tr!("GitHub에 로그인해 주세요", "Please sign in to GitHub"))?;
        clone_into(&token, &slug, Path::new(&parent))
    })
    .await
}

fn clone_into(token: &str, slug: &str, parent: &Path) -> Result<String, String> {
    let name = slug.split('/').nth(1).unwrap_or(slug);
    let target = parent.join(name);
    let path = target.to_string_lossy().into_owned();
    if target.exists() {
        // Already a clone of this repository: just open it.
        let same = crate::memo::repo::inspect(&target)
            .map(|info| info.key == format!("github.com/{}", slug.to_lowercase()))
            .unwrap_or(false);
        if same {
            return Ok(path);
        }
        return Err(crate::tr!(
            "'{path}' 폴더가 이미 있어요. 다른 위치를 골라 주세요",
            "'{path}' already exists. Choose another location"
        ));
    }
    let header = format!(
        "AUTHORIZATION: basic {}",
        base64(format!("x-access-token:{token}").as_bytes())
    );
    let output = crate::git::command(parent)
        .args(["clone", "--"])
        .arg(format!("https://github.com/{slug}.git"))
        .arg(&target)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env("GIT_CONFIG_VALUE_0", header)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::tr!(
                    "git을 찾을 수 없어요. Git을 설치한 뒤 다시 시도해 주세요",
                    "Couldn't find git. Install Git and try again"
                )
            } else {
                crate::tr!("git을 실행하지 못했어요: {e}", "Couldn't run git: {e}")
            }
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(crate::tr!(
            "clone하지 못했어요: {detail}",
            "Couldn't clone: {detail}"
        ));
    }
    Ok(path)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(
            base64(b"x-access-token:gho_1"),
            "eC1hY2Nlc3MtdG9rZW46Z2hvXzE="
        );
    }

    #[test]
    fn slugs_are_checked_before_use() {
        assert!(check_slug("me/repo.name_2").is_ok());
        for bad in [
            "me", "me/", "/r", "me/r/x", "me/../r", "me/.git", "me/r x", "-c/x;y",
        ] {
            assert!(check_slug(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn clone_refuses_a_folder_that_is_something_else() {
        let dir = std::env::temp_dir().join(format!("aim-clone-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("repo")).unwrap();
        let err = clone_into("t", "me/repo", &dir).unwrap_err();
        assert!(err.contains("repo"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
