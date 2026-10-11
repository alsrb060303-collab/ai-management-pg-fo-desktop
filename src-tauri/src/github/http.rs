//! A small JSON-over-HTTPS client for github.com and its API.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

pub type Result<T> = std::result::Result<T, String>;

/// Where requests go; tests point these at a local server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    /// `https://github.com` (device flow).
    pub web: String,
    /// `https://api.github.com`.
    pub api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            web: "https://github.com".into(),
            api: "https://api.github.com".into(),
        }
    }
}

pub fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        // The OS certificate store, so corporate proxies with their own CA work.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .http_status_as_error(false)
        .user_agent(concat!("ai-management/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// An API failure the panel words for the user.
fn status_error(status: u16, body: &Value) -> String {
    let message = body["message"].as_str().unwrap_or("");
    match status {
        401 => "GitHub 로그인이 만료됐어요. 다시 로그인해 주세요".into(),
        403 | 429 if message.contains("rate limit") => {
            "GitHub 요청 한도에 걸렸어요. 잠시 뒤 다시 시도해 주세요".into()
        }
        403 => format!("GitHub에서 거절했어요: {message}"),
        404 => "GitHub에서 찾을 수 없어요 (비공개 저장소라면 접근 권한을 확인해 주세요)".into(),
        422 => {
            let detail = body["errors"][0]["message"].as_str().unwrap_or(message);
            format!("GitHub에서 받아들이지 않았어요: {detail}")
        }
        _ => format!("GitHub 오류 ({status}) {message}"),
    }
}

/// Status code and JSON body.
fn read(
    response: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<(u16, Value)> {
    let mut response = response.map_err(|e| format!("GitHub에 연결하지 못했어요 ({e})"))?;
    let status = response.status().as_u16();
    let body: Value = response
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_json()
        .unwrap_or(Value::Null);
    Ok((status, body))
}

/// A POST to github.com (device flow endpoints): JSON both ways, which
/// GitHub requires of requests that carry a body.
pub fn post_form(agent: &Agent, url: &str, form: &[(&str, &str)]) -> Result<Value> {
    let payload: serde_json::Map<String, Value> = form
        .iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect();
    let (status, body) = read(
        agent
            .post(url)
            .header("Accept", "application/json")
            .send_json(Value::Object(payload)),
    )?;
    if status >= 400 {
        return Err(status_error(status, &body));
    }
    Ok(body)
}

pub struct Api<'a> {
    pub agent: &'a Agent,
    pub base: &'a str,
    pub token: &'a str,
}

impl Api<'_> {
    /// `None` for 404 (no such PR, branch not pushed, ...).
    pub fn get_opt<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{path}", self.base);
        let (status, body) = read(
            self.agent
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("Authorization", &format!("Bearer {}", self.token))
                .call(),
        )?;
        match status {
            404 => Ok(None),
            s if s >= 400 => Err(status_error(s, &body)),
            _ => serde_json::from_value(body)
                .map(Some)
                .map_err(|e| format!("GitHub 응답을 읽지 못했어요 ({e})")),
        }
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.get_opt(path)?
            .ok_or_else(|| status_error(404, &Value::Null))
    }

    pub fn post<T: DeserializeOwned>(&self, path: &str, json: &Value) -> Result<T> {
        let url = format!("{}{path}", self.base);
        let (status, body) = read(
            self.agent
                .post(&url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("Authorization", &format!("Bearer {}", self.token))
                .send_json(json),
        )?;
        if status >= 400 {
            return Err(status_error(status, &body));
        }
        serde_json::from_value(body).map_err(|e| format!("GitHub 응답을 읽지 못했어요 ({e})"))
    }
}
