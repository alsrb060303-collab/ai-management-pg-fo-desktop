//! Firebase over REST: sign-in with the GitHub token (Identity Toolkit) and
//! the memo items in Firestore (`users/{uid}/items/{id}`). REST from Rust,
//! not the JS SDK, which does not support desktop webviews.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::memo::SyncRow;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not reachable (offline, DNS, timeout).
    Offline(String),
    /// The rules refused: the account is not on the allow list.
    Forbidden,
    /// The GitHub token was rejected (expired, revoked).
    BadToken,
    Other(String),
}

impl From<Error> for String {
    fn from(error: Error) -> Self {
        match error {
            Error::Offline(e) => crate::tr!(
                "동기화 서버에 연결하지 못했어요 ({e})",
                "Couldn't reach the sync server ({e})"
            ),
            Error::Forbidden => crate::tr!(
                "이 계정은 아직 동기화를 쓸 수 없어요",
                "This account can't use sync yet"
            ),
            Error::BadToken => crate::tr!(
                "GitHub 로그인이 만료됐어요. 작업 탭에서 다시 로그인해 주세요",
                "Your GitHub sign-in expired. Sign in again from the Work tab"
            ),
            Error::Other(e) => e,
        }
    }
}

/// Where requests go; tests point both at a local server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub auth: String,
    pub firestore: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            auth: "https://identitytoolkit.googleapis.com".into(),
            firestore: "https://firestore.googleapis.com".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub project_id: String,
}

pub fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
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

fn send(
    response: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<(u16, Value)> {
    let mut response = response.map_err(|e| Error::Offline(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_json()
        .unwrap_or(Value::Null);
    Ok((status, body))
}

fn api_error(status: u16, body: &Value) -> Error {
    let message = body["error"]["message"]
        .as_str()
        .or_else(|| body[0]["error"]["message"].as_str())
        .unwrap_or("");
    match status {
        403 => Error::Forbidden,
        _ => Error::Other(crate::tr!(
            "동기화 오류 ({status}) {message}",
            "Sync error ({status}) {message}"
        )),
    }
}

/// A signed-in Firebase user.
#[derive(Clone, Debug)]
pub struct Session {
    pub uid: String,
    id_token: String,
    expires: Instant,
}

impl Session {
    pub fn fresh(&self) -> bool {
        Instant::now() + Duration::from_secs(120) < self.expires
    }
}

pub struct Client<'a> {
    pub agent: &'a Agent,
    pub ends: &'a Endpoints,
    pub config: &'a Config,
}

impl Client<'_> {
    /// Signs in with a GitHub OAuth token (the GitHub provider must be on in
    /// the Firebase project).
    pub fn sign_in(&self, github_token: &str) -> Result<Session> {
        let url = format!(
            "{}/v1/accounts:signInWithIdp?key={}",
            self.ends.auth, self.config.api_key
        );
        let (status, body) = send(self.agent.post(&url).send_json(json!({
            "postBody": format!("access_token={github_token}&providerId=github.com"),
            "requestUri": "http://localhost",
            "returnSecureToken": true,
            "returnIdpCredential": true,
        })))?;
        if status >= 400 {
            let message = body["error"]["message"].as_str().unwrap_or("");
            if message.contains("INVALID_IDP_RESPONSE") || message.contains("INVALID_CREDENTIAL") {
                return Err(Error::BadToken);
            }
            return Err(Error::Other(crate::tr!(
                "동기화 로그인에 실패했어요 ({message})",
                "Sync sign-in failed ({message})"
            )));
        }
        let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
        let seconds: u64 = text("expiresIn").parse().unwrap_or(3600);
        Ok(Session {
            uid: text("localId"),
            id_token: text("idToken"),
            expires: Instant::now() + Duration::from_secs(seconds),
        })
    }

    fn documents(&self) -> String {
        format!(
            "{}/v1/projects/{}/databases/(default)/documents",
            self.ends.firestore, self.config.project_id
        )
    }

    fn bearer(session: &Session) -> String {
        format!("Bearer {}", session.id_token)
    }

    /// Whether the owner has put this account on the allow list
    /// (`allowed/{uid}` exists).
    pub fn allowed(&self, session: &Session) -> Result<bool> {
        let url = format!("{}/allowed/{}", self.documents(), session.uid);
        let (status, body) = send(
            self.agent
                .get(&url)
                .header("Authorization", &Self::bearer(session))
                .call(),
        )?;
        match status {
            200 => Ok(true),
            403 | 404 => Ok(false),
            _ => Err(api_error(status, &body)),
        }
    }

    /// Uploads rows; returns the ids the server accepted. A row the server
    /// refuses (a newer one is there) comes back down on the next pull.
    pub fn push(&self, session: &Session, rows: &[SyncRow]) -> Result<Vec<String>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let base = format!(
            "projects/{}/databases/(default)/documents/users/{}/items",
            self.config.project_id, session.uid
        );
        let writes: Vec<Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "update": { "name": format!("{base}/{}", row.id), "fields": to_fields(row) },
                    "updateTransforms": [
                        { "fieldPath": "syncedAt", "setToServerValue": "REQUEST_TIME" }
                    ],
                })
            })
            .collect();
        let url = format!("{}:batchWrite", self.documents());
        let (status, body) = send(
            self.agent
                .post(&url)
                .header("Authorization", &Self::bearer(session))
                .send_json(json!({ "writes": writes })),
        )?;
        if status >= 400 {
            return Err(api_error(status, &body));
        }
        let statuses = body["status"].as_array().cloned().unwrap_or_default();
        Ok(rows
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                statuses
                    .get(*i)
                    .is_none_or(|s| s["code"].as_i64().unwrap_or(0) == 0)
            })
            .map(|(_, row)| row.id.clone())
            .collect())
    }

    /// Items the server stored after `after` (its `syncedAt` and document
    /// name, so equal timestamps page correctly), oldest first.
    pub fn pull(
        &self,
        session: &Session,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<(Vec<SyncRow>, Option<Cursor>)> {
        let mut query = json!({
            "from": [{ "collectionId": "items" }],
            "orderBy": [
                { "field": { "fieldPath": "syncedAt" }, "direction": "ASCENDING" },
                { "field": { "fieldPath": "__name__" }, "direction": "ASCENDING" }
            ],
            "limit": limit,
        });
        if let Some(c) = after {
            query["startAt"] = json!({
                "values": [{ "timestampValue": c.time }, { "referenceValue": c.name }],
                "before": false,
            });
        }
        let url = format!("{}/users/{}:runQuery", self.documents(), session.uid);
        let (status, body) = send(
            self.agent
                .post(&url)
                .header("Authorization", &Self::bearer(session))
                .send_json(json!({ "structuredQuery": query })),
        )?;
        if status >= 400 {
            return Err(api_error(status, &body));
        }
        let mut rows = Vec::new();
        let mut last = None;
        for entry in body.as_array().cloned().unwrap_or_default() {
            let doc = &entry["document"];
            let Some(name) = doc["name"].as_str() else {
                continue;
            };
            if let Some(time) = doc["fields"]["syncedAt"]["timestampValue"].as_str() {
                last = Some(Cursor {
                    time: time.to_string(),
                    name: name.to_string(),
                });
            }
            if let Some(row) = from_fields(name, &doc["fields"]) {
                rows.push(row);
            }
        }
        Ok((rows, last))
    }
}

impl Client<'_> {
    /// Writes documents `(id, fields)` to `users/{uid}/{collection}`, each
    /// stamped with the server's `syncedAt`. Returns, per document, whether
    /// the server accepted it (the rules refuse an older copy).
    pub fn put(
        &self,
        session: &Session,
        collection: &str,
        docs: &[(String, Value)],
    ) -> Result<Vec<bool>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let base = format!(
            "projects/{}/databases/(default)/documents/users/{}/{collection}",
            self.config.project_id, session.uid
        );
        let writes: Vec<Value> = docs
            .iter()
            .map(|(id, fields)| {
                json!({
                    "update": { "name": format!("{base}/{id}"), "fields": fields },
                    "updateTransforms": [
                        { "fieldPath": "syncedAt", "setToServerValue": "REQUEST_TIME" }
                    ],
                })
            })
            .collect();
        let url = format!("{}:batchWrite", self.documents());
        let (status, body) = send(
            self.agent
                .post(&url)
                .header("Authorization", &Self::bearer(session))
                .send_json(json!({ "writes": writes })),
        )?;
        if status >= 400 {
            return Err(api_error(status, &body));
        }
        let statuses = body["status"].as_array().cloned().unwrap_or_default();
        Ok((0..docs.len())
            .map(|i| {
                statuses
                    .get(i)
                    .is_none_or(|s| s["code"].as_i64().unwrap_or(0) == 0)
            })
            .collect())
    }

    /// Documents of `users/{uid}/{collection}` stored after `after`, oldest
    /// first, as `(document id, fields)`, and where this page ended.
    pub fn changes(
        &self,
        session: &Session,
        collection: &str,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<Page> {
        let mut query = json!({
            "from": [{ "collectionId": collection }],
            "orderBy": [
                { "field": { "fieldPath": "syncedAt" }, "direction": "ASCENDING" },
                { "field": { "fieldPath": "__name__" }, "direction": "ASCENDING" }
            ],
            "limit": limit,
        });
        if let Some(c) = after {
            query["startAt"] = json!({
                "values": [{ "timestampValue": c.time }, { "referenceValue": c.name }],
                "before": false,
            });
        }
        let url = format!("{}/users/{}:runQuery", self.documents(), session.uid);
        let (status, body) = send(
            self.agent
                .post(&url)
                .header("Authorization", &Self::bearer(session))
                .send_json(json!({ "structuredQuery": query })),
        )?;
        if status >= 400 {
            return Err(api_error(status, &body));
        }
        let mut docs = Vec::new();
        let mut last = None;
        for entry in body.as_array().cloned().unwrap_or_default() {
            let doc = &entry["document"];
            let Some(name) = doc["name"].as_str() else {
                continue;
            };
            if let Some(time) = doc["fields"]["syncedAt"]["timestampValue"].as_str() {
                last = Some(Cursor {
                    time: time.to_string(),
                    name: name.to_string(),
                });
            }
            if let Some(id) = name.rsplit('/').next() {
                docs.push((id.to_string(), doc["fields"].clone()));
            }
        }
        Ok((docs, last))
    }
}

/// Documents of one page, as `(document id, fields)`, and where it ended.
pub type Page = (Vec<(String, Value)>, Option<Cursor>);

/// Where the last pull stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub time: String,
    pub name: String,
}

impl Cursor {
    pub fn encode(&self) -> String {
        format!("{}\n{}", self.time, self.name)
    }

    pub fn decode(s: &str) -> Option<Cursor> {
        let (time, name) = s.split_once('\n')?;
        Some(Cursor {
            time: time.into(),
            name: name.into(),
        })
    }
}

fn to_fields(row: &SyncRow) -> Value {
    json!({
        "repoKey": { "stringValue": row.repo_key },
        "branch": { "stringValue": row.branch },
        "kind": { "stringValue": row.kind },
        "text": { "stringValue": row.text },
        "done": { "booleanValue": row.done },
        "position": { "doubleValue": row.position },
        "createdAt": { "integerValue": row.created_at.to_string() },
        "updatedAt": { "integerValue": row.updated_at.to_string() },
        "deleted": { "booleanValue": row.deleted },
        "deviceId": { "stringValue": row.device_id },
    })
}

fn from_fields(name: &str, f: &Value) -> Option<SyncRow> {
    let s = |k: &str| f[k]["stringValue"].as_str().map(str::to_string);
    let i = |k: &str| {
        f[k]["integerValue"]
            .as_str()
            .and_then(|v| v.parse::<i64>().ok())
    };
    let b = |k: &str| f[k]["booleanValue"].as_bool().unwrap_or(false);
    let position = f["position"]["doubleValue"]
        .as_f64()
        .or_else(|| i("position").map(|v| v as f64))
        .unwrap_or(0.0);
    let kind = s("kind")?;
    if kind != "note" && kind != "todo" {
        return None;
    }
    Some(SyncRow {
        id: name.rsplit('/').next()?.to_string(),
        repo_key: s("repoKey")?,
        branch: s("branch").unwrap_or_default(),
        kind,
        text: s("text").unwrap_or_default(),
        done: b("done"),
        position,
        created_at: i("createdAt").unwrap_or(0),
        updated_at: i("updatedAt")?,
        deleted: b("deleted"),
        device_id: s("deviceId").unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::test_serve as serve;

    fn row(id: &str, at: i64) -> SyncRow {
        SyncRow {
            id: id.into(),
            repo_key: "github.com/o/r".into(),
            branch: "main".into(),
            kind: "todo".into(),
            text: "x".into(),
            done: true,
            position: 1.5,
            created_at: 1,
            updated_at: at,
            deleted: false,
            device_id: "d1".into(),
        }
    }

    #[test]
    fn fields_round_trip() {
        let r = row("abc", 42);
        let back = from_fields(
            "projects/p/databases/(default)/documents/users/u/items/abc",
            &to_fields(&r),
        );
        assert_eq!(back, Some(r));
    }

    #[test]
    fn sign_in_push_and_pull_talk_firestore_rest() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"uid1","idToken":"tok","expiresIn":"3600"}"#,
            ),
            (200, r#"{"name":"x"}"#),
            (200, r#"{"writeResults":[{},{}],"status":[{},{"code":9}]}"#),
            (
                200,
                r#"[{"document":{"name":"projects/p/databases/(default)/documents/users/uid1/items/n1","fields":{"repoKey":{"stringValue":"r"},"kind":{"stringValue":"note"},"text":{"stringValue":"hi"},"updatedAt":{"integerValue":"7"},"deviceId":{"stringValue":"d2"},"syncedAt":{"timestampValue":"2026-10-11T00:00:00.5Z"}}},"readTime":"t"}]"#,
            ),
        ]);
        let agent: Agent = Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let ends = Endpoints {
            auth: base.clone(),
            firestore: base,
        };
        let config = Config {
            api_key: "k".into(),
            project_id: "p".into(),
        };
        let c = Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        };
        let session = c.sign_in("gho_x").unwrap();
        assert_eq!(session.uid, "uid1");
        assert!(c.allowed(&session).unwrap());
        let accepted = c.push(&session, &[row("a", 1), row("b", 2)]).unwrap();
        assert_eq!(accepted, ["a"]);
        let (rows, cursor) = c.pull(&session, None, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id.as_str(), rows[0].text.as_str()), ("n1", "hi"));
        let cursor = cursor.unwrap();
        assert_eq!(Cursor::decode(&cursor.encode()), Some(cursor));

        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("accounts:signInWithIdp?key=k"));
        assert!(seen[0].contains("access_token=gho_x&providerId=github.com"));
        assert!(seen[1].contains("GET /v1/projects/p/databases/(default)/documents/allowed/uid1"));
        assert!(seen[2].contains(":batchWrite") && seen[2].contains("REQUEST_TIME"));
        assert!(seen[3].contains("users/uid1:runQuery"));
    }
}
