//! The signed-in app account: its refresh token in the OS credential store,
//! the sign-in commands (email, Google, GitHub) and linking more methods to
//! the same account so its synced data stays together.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use super::auth::{Profile, Tokens};
use super::firebase::{self, Client, Endpoints, Error, Session};
use super::{Sync, UID_KEY, config, kick};
use crate::memo::Memo;

pub const ACCOUNT_EVENT: &str = "account";
/// Set once the account was chosen here (signed in or out), so an earlier
/// version's GitHub-only sync is no longer carried over.
const MIGRATED_KEY: &str = "account_migrated";
/// Google comes back to `http://localhost:{GOOGLE_PORT}{GOOGLE_PATH}`; the
/// address must be on the OAuth client's list (docs/sync-setup.md).
const GOOGLE_PORT: u16 = 47823;
const GOOGLE_PATH: &str = "/auth";
const GOOGLE_WAIT: Duration = Duration::from_secs(300);

/// What is kept between runs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub uid: String,
    refresh_token: String,
    #[serde(default)]
    profile: Profile,
}

/// The account as the frontend sees it.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// This build has a Firebase project.
    pub configured: bool,
    pub uid: Option<String>,
    pub profile: Option<Profile>,
}

/// Where the refresh token is kept; tests use memory.
pub trait Secrets: Send + std::marker::Sync {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, value: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

pub struct Keyring;

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("ai-management", "app-account").map_err(|e| {
        crate::tr!(
            "자격 증명 저장소를 쓸 수 없어요 ({e})",
            "The credential store isn't available ({e})"
        )
    })
}

impl Secrets for Keyring {
    fn load(&self) -> Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(crate::tr!(
                "저장된 로그인을 읽지 못했어요 ({e})",
                "Couldn't read the saved sign-in ({e})"
            )),
        }
    }

    fn save(&self, value: &str) -> Result<(), String> {
        entry()?.set_password(value).map_err(|e| {
            crate::tr!(
                "로그인을 저장하지 못했어요 ({e})",
                "Couldn't save the sign-in ({e})"
            )
        })
    }

    fn clear(&self) -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(crate::tr!(
                "로그인을 지우지 못했어요 ({e})",
                "Couldn't remove the sign-in ({e})"
            )),
        }
    }
}

impl Sync {
    fn saved(&self) -> Result<Option<Saved>, String> {
        let mut cached = self.account.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            let loaded = self
                .secrets
                .load()?
                .and_then(|json| serde_json::from_str::<Saved>(&json).ok());
            *cached = Some(loaded);
        }
        Ok(cached.clone().flatten())
    }

    fn info(&self) -> Info {
        let saved = self.saved().ok().flatten();
        Info {
            configured: config().is_some(),
            uid: saved.as_ref().map(|s| s.uid.clone()),
            profile: saved.map(|s| s.profile),
        }
    }
}

/// Keeps (or forgets) the account and tells sync and the windows.
fn store(app: &AppHandle, saved: Option<Saved>) -> Result<Info, String> {
    let sync = app.state::<Sync>();
    match &saved {
        Some(s) => sync
            .secrets
            .save(&serde_json::to_string(s).map_err(|e| e.to_string())?)?,
        None => sync.secrets.clear()?,
    }
    let uid_changed = {
        let mut cached = sync.account.lock().unwrap_or_else(|e| e.into_inner());
        let before = cached.clone().flatten().map(|s| s.uid);
        *cached = Some(saved.clone());
        before != saved.as_ref().map(|s| s.uid.clone())
    };
    if uid_changed {
        *sync.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    let info = sync.info();
    let _ = app.emit(ACCOUNT_EVENT, &info);
    kick(app, Duration::ZERO);
    Ok(info)
}

fn mark_chosen(app: &AppHandle) {
    if let Ok(store) = app.state::<Memo>().store() {
        let _ = store.set_meta(MIGRATED_KEY, Some("1"));
    }
}

/// Keeps a fresh sign-in (or a link, which renews the tokens).
fn keep(app: &AppHandle, client: &Client, tokens: Tokens) -> firebase::Result<Info> {
    let profile = client.profile(&tokens.session)?;
    mark_chosen(app);
    let info = store(
        app,
        Some(Saved {
            uid: tokens.session.uid.clone(),
            refresh_token: tokens.refresh_token,
            profile,
        }),
    )
    .map_err(Error::Other)?;
    *app.state::<Sync>()
        .session
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(tokens.session);
    Ok(info)
}

/// A current ID token for the saved account: the cached one, or renewed.
/// `None` when nobody is signed in. A rejected refresh token signs out.
pub(super) fn session(app: &AppHandle, client: &Client) -> firebase::Result<Option<Session>> {
    let sync = app.state::<Sync>();
    let cached = sync
        .session
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let saved = sync.saved().map_err(Error::Other)?;
    if let Some(s) = cached.filter(|s| s.fresh() && saved.as_ref().is_some_and(|v| v.uid == s.uid))
    {
        return Ok(Some(s));
    }
    let tokens = match saved {
        Some(saved) => match client.refresh(&saved.refresh_token) {
            Ok(t) => {
                if t.refresh_token != saved.refresh_token || t.session.uid != saved.uid {
                    let _ = store(
                        app,
                        Some(Saved {
                            uid: t.session.uid.clone(),
                            refresh_token: t.refresh_token.clone(),
                            profile: saved.profile,
                        }),
                    );
                }
                t
            }
            Err(Error::BadToken) => {
                let _ = store(app, None);
                return Err(Error::BadToken);
            }
            Err(e) => return Err(e),
        },
        None => match migrate(app, client)? {
            Some(t) => t,
            None => return Ok(None),
        },
    };
    *sync.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(tokens.session.clone());
    Ok(Some(tokens.session))
}

/// Earlier versions synced as the GitHub account of the work tab. A device
/// that did keeps its account: the same GitHub sign-in becomes the app's.
fn migrate(app: &AppHandle, client: &Client) -> firebase::Result<Option<Tokens>> {
    let memo = app.state::<Memo>();
    let store_ = memo.store().map_err(Error::Other)?;
    if store_.meta(MIGRATED_KEY).map_err(Error::Other)?.is_some()
        || store_.meta(UID_KEY).map_err(Error::Other)?.is_none()
    {
        return Ok(None);
    }
    let Some(token) = crate::github::current_token(app) else {
        return Ok(None);
    };
    let tokens = client.sign_in_github(&token, None)?;
    keep(app, client, tokens.clone())?;
    Ok(Some(tokens))
}

/// Runs `f` with a Firebase client off the main thread.
async fn with_client<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &Client) -> firebase::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = config().ok_or_else(|| {
            crate::tr!(
                "이 빌드에는 동기화 서버가 설정되지 않았어요",
                "This build has no sync server set up"
            )
        })?;
        let sync = app.state::<Sync>();
        let ends = Endpoints::default();
        let client = Client {
            agent: &sync.agent,
            ends: &ends,
            config: &config,
        };
        f(&app, &client).map_err(String::from)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The signed-in session, for linking.
fn current(app: &AppHandle, client: &Client) -> firebase::Result<Session> {
    session(app, client)?
        .ok_or_else(|| Error::Other(crate::tr!("먼저 로그인해 주세요", "Please sign in first")))
}

#[tauri::command]
pub async fn account_status(app: AppHandle, refresh: bool) -> Result<Info, String> {
    let info = app.state::<Sync>().info();
    if !refresh || info.uid.is_none() || !info.configured {
        return Ok(info);
    }
    // The email may have been verified (or methods changed) elsewhere.
    with_client(app, |app, client| {
        let s = current(app, client)?;
        let profile = client.profile(&s)?;
        let sync = app.state::<Sync>();
        let saved = sync.saved().map_err(Error::Other)?;
        match saved {
            Some(saved) if saved.profile != profile => {
                store(app, Some(Saved { profile, ..saved })).map_err(Error::Other)
            }
            _ => Ok(sync.info()),
        }
    })
    .await
}

fn clean_email(email: &str) -> String {
    email.trim().to_string()
}

#[tauri::command]
pub async fn account_sign_up(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<Info, String> {
    with_client(app, move |app, client| {
        let tokens = client.sign_up(&clean_email(&email), &password)?;
        // Not required to sync, but it is how a forgotten password comes back.
        let _ = client.send_verification(&tokens.session);
        keep(app, client, tokens)
    })
    .await
}

#[tauri::command]
pub async fn account_sign_in(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<Info, String> {
    with_client(app, move |app, client| {
        let tokens = client.sign_in_password(&clean_email(&email), &password)?;
        keep(app, client, tokens)
    })
    .await
}

#[tauri::command]
pub async fn account_reset_password(app: AppHandle, email: String) -> Result<(), String> {
    with_client(app, move |_, client| {
        client.send_password_reset(&clean_email(&email))
    })
    .await
}

#[tauri::command]
pub async fn account_send_verification(app: AppHandle) -> Result<(), String> {
    with_client(app, |app, client| {
        let s = current(app, client)?;
        client.send_verification(&s)
    })
    .await
}

/// Adds email and password sign-in to the signed-in account.
#[tauri::command]
pub async fn account_add_password(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<Info, String> {
    with_client(app, move |app, client| {
        let s = current(app, client)?;
        let tokens = client.link_password(&s, &clean_email(&email), &password)?;
        keep(app, client, tokens)
    })
    .await
}

/// Signs in with (or links) the GitHub account the work tab is signed in to.
#[tauri::command]
pub async fn account_github(app: AppHandle, link: bool) -> Result<Info, String> {
    with_client(app, move |app, client| {
        let token = crate::github::current_token(app).ok_or_else(|| {
            Error::Other(crate::tr!(
                "GitHub에 먼저 로그인해 주세요",
                "Sign in to GitHub first"
            ))
        })?;
        let linking = if link {
            Some(current(app, client)?)
        } else {
            None
        };
        let tokens = client.sign_in_github(&token, linking.as_ref())?;
        keep(app, client, tokens)
    })
    .await
}

/// Signs in with (or links) Google in the user's browser; Firebase's own
/// OAuth client does the exchange, so the app holds no client secret.
#[tauri::command]
pub async fn account_google(app: AppHandle, link: bool) -> Result<Info, String> {
    let generation = app.state::<Sync>().google.fetch_add(1, Ordering::SeqCst) + 1;
    with_client(app, move |app, client| {
        let linking = if link {
            Some(current(app, client)?)
        } else {
            None
        };
        let listener = bind().map_err(Error::Other)?;
        let redirect = format!("http://localhost:{GOOGLE_PORT}{GOOGLE_PATH}");
        let uri = client.auth_uri("google.com", &redirect)?;
        app.opener()
            .open_url(&uri.url, None::<&str>)
            .map_err(|e| Error::Other(e.to_string()))?;
        let query = wait_for_redirect(&listener, || {
            app.state::<Sync>().google.load(Ordering::SeqCst) != generation
        })
        .map_err(Error::Other)?;
        let tokens = client.sign_in_idp(
            "",
            &format!("{redirect}?{query}"),
            Some(&uri.session_id),
            linking.as_ref(),
        )?;
        keep(app, client, tokens)
    })
    .await
}

#[tauri::command]
pub fn account_google_cancel(app: AppHandle) {
    app.state::<Sync>().google.fetch_add(1, Ordering::SeqCst);
}

/// Removes a sign-in method (not the last one).
#[tauri::command]
pub async fn account_unlink(app: AppHandle, provider: String) -> Result<Info, String> {
    with_client(app, move |app, client| {
        let s = current(app, client)?;
        let profile = client.profile(&s)?;
        if profile.providers.len() <= 1 {
            return Err(Error::Other(crate::tr!(
                "마지막 로그인 방법은 뺄 수 없어요",
                "You can't remove the last sign-in method"
            )));
        }
        client.unlink(&s, &provider)?;
        let profile = client.profile(&s)?;
        let saved = app.state::<Sync>().saved().map_err(Error::Other)?;
        match saved {
            Some(saved) => store(app, Some(Saved { profile, ..saved })).map_err(Error::Other),
            None => Ok(app.state::<Sync>().info()),
        }
    })
    .await
}

#[tauri::command]
pub async fn account_sign_out(app: AppHandle) -> Result<Info, String> {
    tauri::async_runtime::spawn_blocking(move || {
        mark_chosen(&app);
        store(&app, None)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The port Google comes back to; a sign-in that is still waiting on it
/// lets go within a moment once cancelled.
fn bind() -> Result<TcpListener, String> {
    let mut last = None;
    for _ in 0..10 {
        match TcpListener::bind(("127.0.0.1", GOOGLE_PORT)) {
            Ok(l) => {
                l.set_nonblocking(true).map_err(|e| e.to_string())?;
                return Ok(l);
            }
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    }
    Err(crate::tr!(
        "로그인 응답을 받을 포트 {GOOGLE_PORT}을(를) 열지 못했어요 ({})",
        "Couldn't open port {GOOGLE_PORT} for the sign-in reply ({})",
        last.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Waits for the browser to come back to the redirect address; returns its
/// query string (`code=…&state=…`).
fn wait_for_redirect(
    listener: &TcpListener,
    cancelled: impl Fn() -> bool,
) -> Result<String, String> {
    let deadline = Instant::now() + GOOGLE_WAIT;
    loop {
        if cancelled() {
            return Err(crate::tr!("로그인을 취소했어요", "Sign-in cancelled"));
        }
        if Instant::now() > deadline {
            return Err(crate::tr!(
                "로그인 시간이 지났어요. 다시 해 주세요",
                "The sign-in timed out. Please try again"
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = answer(stream) {
                    return result;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Reads one request; `None` when it isn't the redirect (a favicon, a
/// browser's spare connection).
fn answer(mut stream: TcpStream) -> Option<Result<String, String>> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut line = String::new();
    BufReader::new(stream.try_clone().ok()?)
        .read_line(&mut line)
        .ok()?;
    // "GET /auth?code=…&state=… HTTP/1.1"
    let target = line.split_whitespace().nth(1)?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != GOOGLE_PATH {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return None;
    }
    let failed = query.split('&').any(|p| p.starts_with("error="));
    let message = if failed {
        crate::tr!(
            "로그인하지 않았어요. 이 창을 닫고 앱에서 다시 해 주세요.",
            "Not signed in. Close this tab and try again in the app."
        )
    } else {
        crate::tr!(
            "로그인했어요. 이 창을 닫고 앱으로 돌아가세요.",
            "Signed in. Close this tab and return to the app."
        )
    };
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>AI Management</title><body style=\"font-family:system-ui;padding:3rem;text-align:center\"><p>{message}</p></body>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    Some(if failed {
        Err(crate::tr!(
            "Google 로그인을 취소했어요",
            "The Google sign-in was cancelled"
        ))
    } else {
        Ok(query.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_skips_other_requests_and_returns_the_query() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let browser = std::thread::spawn(move || {
            let get = |path: &str| {
                let mut s = TcpStream::connect(addr).unwrap();
                write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
                let mut out = String::new();
                let _ = std::io::Read::read_to_string(&mut s, &mut out);
                out
            };
            let favicon = get("/favicon.ico");
            let page = get("/auth?code=abc&state=xyz");
            (favicon, page)
        });
        let query = wait_for_redirect(&listener, || false).unwrap();
        assert_eq!(query, "code=abc&state=xyz");
        let (favicon, page) = browser.join().unwrap();
        assert!(favicon.starts_with("HTTP/1.1 404"));
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("로그인했어요"));
    }

    #[test]
    fn a_denied_sign_in_and_a_cancel_end_the_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            write!(s, "GET /auth?error=access_denied HTTP/1.1\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = std::io::Read::read_to_string(&mut s, &mut out);
        });
        assert!(wait_for_redirect(&listener, || false).is_err());
        assert!(wait_for_redirect(&listener, || true).is_err());
    }
}
