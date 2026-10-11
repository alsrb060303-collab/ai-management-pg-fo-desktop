//! GitHub sign-in with the OAuth device flow: the app shows a short code, the
//! user enters it on github.com in their own browser, and the app polls until
//! GitHub hands over a token. No client secret is involved, so the app can be
//! open source; the token is kept in the OS credential store.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use ureq::Agent;

use super::http::{self, Endpoints, Result};

/// Scope for private repositories' pull requests, issues and checks.
pub const SCOPE: &str = "repo";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(skip)]
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Seconds until the code expires.
    pub expires_in: u64,
    #[serde(skip)]
    pub interval: u64,
}

pub fn start(agent: &Agent, ends: &Endpoints, client_id: &str) -> Result<DeviceCode> {
    let body = http::post_form(
        agent,
        &format!("{}/login/device/code", ends.web),
        &[("client_id", client_id), ("scope", SCOPE)],
    )?;
    if let Some(error) = body["error"].as_str() {
        return Err(match error {
            "device_flow_disabled" => crate::tr!(
                "GitHub 앱 설정에서 Device Flow가 꺼져 있어요",
                "Device Flow is turned off in the GitHub app's settings"
            ),
            _ => crate::tr!(
                "GitHub 로그인을 시작하지 못했어요 ({error})",
                "Couldn't start the GitHub sign-in ({error})"
            ),
        }
        .into());
    }
    let text = |key: &str| body[key].as_str().map(str::to_string);
    let missing = || {
        crate::tr!(
            "GitHub 응답에 코드가 없어요",
            "GitHub's response has no code"
        )
    };
    Ok(DeviceCode {
        device_code: text("device_code").ok_or_else(missing)?,
        user_code: text("user_code").ok_or_else(missing)?,
        verification_uri: text("verification_uri")
            .unwrap_or_else(|| "https://github.com/login/device".into()),
        expires_in: body["expires_in"].as_u64().unwrap_or(900),
        interval: body["interval"].as_u64().unwrap_or(5),
    })
}

/// How a sign-in ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Token(String),
    Denied,
    Expired,
    Cancelled,
}

/// Polls until the user approves, declines, the code expires, or `cancel`
/// moves past `generation` (a newer sign-in or a cancel).
pub fn poll(
    agent: &Agent,
    ends: &Endpoints,
    client_id: &str,
    code: &DeviceCode,
    cancel: &AtomicU64,
    generation: u64,
    tick: Duration,
) -> Result<Outcome> {
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = code.interval.max(1);
    let url = format!("{}/login/oauth/access_token", ends.web);
    loop {
        // Sleep in small steps so a cancel takes effect quickly.
        let wake = Instant::now() + tick * interval as u32;
        while Instant::now() < wake {
            if cancel.load(Ordering::SeqCst) != generation {
                return Ok(Outcome::Cancelled);
            }
            std::thread::sleep(tick.min(Duration::from_millis(200)));
        }
        if cancel.load(Ordering::SeqCst) != generation {
            return Ok(Outcome::Cancelled);
        }
        if Instant::now() > deadline {
            return Ok(Outcome::Expired);
        }
        let body = match http::post_form(
            agent,
            &url,
            &[
                ("client_id", client_id),
                ("device_code", &code.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        ) {
            Ok(body) => body,
            // A dropped connection: the code is still good, try again.
            Err(http::Error::Offline(_)) => continue,
            Err(e) => return Err(e),
        };
        // Cancelled (or superseded) while the request was in flight.
        if cancel.load(Ordering::SeqCst) != generation {
            return Ok(Outcome::Cancelled);
        }
        if let Some(token) = body["access_token"].as_str() {
            return Ok(Outcome::Token(token.to_string()));
        }
        match body["error"].as_str() {
            Some("authorization_pending") => {}
            // GitHub asks to back off; its response carries the new interval.
            Some("slow_down") => {
                interval = body["interval"].as_u64().unwrap_or(interval + 5);
            }
            Some("expired_token") => return Ok(Outcome::Expired),
            Some("access_denied") => return Ok(Outcome::Denied),
            Some(other) => {
                return Err(crate::tr!(
                    "GitHub 로그인에 실패했어요 ({other})",
                    "GitHub sign-in failed ({other})"
                )
                .into());
            }
            None => {
                return Err(crate::tr!(
                    "GitHub 응답을 이해하지 못했어요",
                    "Couldn't understand GitHub's response"
                )
                .into());
            }
        }
    }
}

/// Keeps the token in the OS credential store (Windows Credential Manager,
/// macOS Keychain, Secret Service on Linux).
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<String>>;
    fn save(&self, token: &str) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub struct Keyring;

const SERVICE: &str = "ai-management";
const ACCOUNT: &str = "github-token";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| {
        crate::tr!(
            "자격 증명 저장소를 쓸 수 없어요 ({e})",
            "The credential store isn't available ({e})"
        )
        .into()
    })
}

impl TokenStore for Keyring {
    fn load(&self) -> Result<Option<String>> {
        match entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(crate::tr!(
                "저장된 GitHub 로그인을 읽지 못했어요 ({e})",
                "Couldn't read the saved GitHub sign-in ({e})"
            )
            .into()),
        }
    }

    fn save(&self, token: &str) -> Result<()> {
        entry()?.set_password(token).map_err(|e| {
            crate::tr!(
                "GitHub 로그인을 저장하지 못했어요 ({e})",
                "Couldn't save the GitHub sign-in ({e})"
            )
            .into()
        })
    }

    fn clear(&self) -> Result<()> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(crate::tr!(
                "GitHub 로그인을 지우지 못했어요 ({e})",
                "Couldn't remove the GitHub sign-in ({e})"
            )
            .into()),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    /// Answers each request with the next canned JSON body; returns the base
    /// URL and the request lines it saw.
    pub fn serve(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
                let mut payload = vec![0; length];
                reader.read_exact(&mut payload).unwrap();
                log.lock().unwrap().push(format!(
                    "{} {}",
                    line.trim(),
                    String::from_utf8_lossy(&payload)
                ));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (base, seen)
    }

    fn plain_agent() -> Agent {
        Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into()
    }

    #[test]
    fn device_flow_waits_backs_off_and_returns_the_token() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"device_code":"dev","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","expires_in":900,"interval":1}"#,
            ),
            (200, r#"{"error":"authorization_pending"}"#),
            (200, r#"{"error":"slow_down","interval":1}"#),
            (
                200,
                r#"{"access_token":"gho_x","token_type":"bearer","scope":"repo"}"#,
            ),
        ]);
        let ends = Endpoints {
            web: base.clone(),
            api: base,
        };
        let agent = plain_agent();
        let code = start(&agent, &ends, "cid").unwrap();
        assert_eq!(code.user_code, "ABCD-1234");
        let cancel = AtomicU64::new(1);
        let outcome = poll(
            &agent,
            &ends,
            "cid",
            &code,
            &cancel,
            1,
            Duration::from_millis(5),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Token("gho_x".into()));
        let seen = seen.lock().unwrap();
        let body = |i: usize| -> serde_json::Value {
            let at = seen[i].find('{').unwrap();
            serde_json::from_str(&seen[i][at..]).unwrap()
        };
        assert!(seen[0].contains("/login/device/code"));
        assert_eq!(body(0)["scope"], "repo");
        assert_eq!(
            body(1)["grant_type"],
            "urn:ietf:params:oauth:grant-type:device_code"
        );
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn device_flow_reports_denial_and_cancel() {
        let (base, _) = serve(vec![(200, r#"{"error":"access_denied"}"#)]);
        let ends = Endpoints {
            web: base.clone(),
            api: base,
        };
        let code = DeviceCode {
            device_code: "d".into(),
            user_code: "u".into(),
            verification_uri: String::new(),
            expires_in: 60,
            interval: 1,
        };
        let agent = plain_agent();
        let cancel = AtomicU64::new(1);
        let tick = Duration::from_millis(5);
        assert_eq!(
            poll(&agent, &ends, "c", &code, &cancel, 1, tick).unwrap(),
            Outcome::Denied
        );
        cancel.store(2, Ordering::SeqCst);
        assert_eq!(
            poll(&agent, &ends, "c", &code, &cancel, 1, tick).unwrap(),
            Outcome::Cancelled
        );
    }
}
