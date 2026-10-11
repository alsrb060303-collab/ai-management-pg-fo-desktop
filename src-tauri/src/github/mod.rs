//! GitHub in the work panel: sign-in (OAuth device flow), the current
//! branch's pull request and checks, open pull requests and issues, and
//! opening a pull request. Reviews, merges and comments stay on github.com.

mod api;
mod auth;
mod http;

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ureq::Agent;

use api::{Account, Pull, RepoView};
use auth::{DeviceCode, Outcome, TokenStore};
use http::{Api, Endpoints};

/// The OAuth app's client ID (public; the device flow needs no secret).
/// Empty until the app is registered; `AIM_GITHUB_CLIENT_ID` overrides it.
const CLIENT_ID: &str = "";

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
        });
    }

    fn token(&self) -> Result<Option<String>, String> {
        let mut cached = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = Some(self.store.load()?);
        }
        Ok(cached.clone().flatten())
    }

    fn set_token(&self, token: Option<String>) -> Result<(), String> {
        match &token {
            Some(t) => self.store.save(t)?,
            None => self.store.clear()?,
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
    fn call<T>(
        &self,
        app: &AppHandle,
        f: impl FnOnce(&Api) -> Result<T, String>,
    ) -> Result<T, String> {
        let token = self.token()?.ok_or("GitHub에 로그인해 주세요")?;
        let result = f(&self.api(&token));
        if let Err(e) = &result
            && e.contains("로그인이 만료")
        {
            let _ = self.set_token(None);
            let _ = app.emit(AUTH_EVENT, AuthEvent::signed_out());
        }
        result
    }
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
            Err(e) if e.contains("로그인이 만료") => Ok(Status {
                configured,
                account: None,
                offline: false,
            }),
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
        let client = client_id().ok_or("GitHub 앱 정보(Client ID)가 아직 설정되지 않았어요")?;
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
                Ok(Outcome::Token(token)) => match gh.set_token(Some(token)) {
                    Ok(()) => AuthEvent {
                        signed_in: true,
                        message: None,
                    },
                    Err(e) => AuthEvent {
                        signed_in: false,
                        message: Some(e),
                    },
                },
                Ok(Outcome::Denied) => AuthEvent {
                    signed_in: false,
                    message: Some("GitHub에서 로그인을 거절했어요".into()),
                },
                Ok(Outcome::Expired) => AuthEvent {
                    signed_in: false,
                    message: Some("코드가 만료됐어요. 다시 로그인해 주세요".into()),
                },
                Err(e) => AuthEvent {
                    signed_in: false,
                    message: Some(e),
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
    app.state::<GitHub>().login.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
pub async fn github_logout(app: AppHandle) -> Result<(), String> {
    blocking(app, |app, gh| {
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
        let (slug, branch) = github_repo_of(&project)?.ok_or("GitHub 저장소가 아니에요")?;
        let head = branch.ok_or("브랜치를 체크아웃한 상태에서 PR을 만들 수 있어요")?;
        if head == base {
            return Err("기준 브랜치와 다른 브랜치에서 PR을 만들어 주세요".into());
        }
        gh.call(app, |api| {
            api::create_pull(api, &slug, &head, &base, &title, &body, draft)
        })
    })
    .await
}
