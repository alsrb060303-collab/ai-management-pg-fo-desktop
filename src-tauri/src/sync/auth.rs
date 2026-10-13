//! The app's own account over the Identity Toolkit REST API: email and
//! password, Google, and GitHub all sign in to one Firebase user, so a
//! user's synced data follows the account rather than one sign-in method.
//! The refresh token keeps the user signed in between runs.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::firebase::{Client, Error, Result, Session, send};

/// A signed-in user: the short-lived ID token and the refresh token that
/// renews it.
#[derive(Clone, Debug)]
pub struct Tokens {
    pub session: Session,
    pub refresh_token: String,
}

/// What the account looks like (from `accounts:lookup`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    /// Sign-in methods on the account: `password`, `google.com`, `github.com`.
    pub providers: Vec<String>,
}

/// Where a Google sign-in starts (`createAuthUri`).
#[derive(Clone, Debug)]
pub struct AuthUri {
    pub url: String,
    pub session_id: String,
}

/// Firebase's error codes in words the user can act on.
fn auth_error(status: u16, body: &Value) -> Error {
    let raw = body["error"]["message"].as_str().unwrap_or("");
    // "WEAK_PASSWORD : Password should be at least 6 characters"
    let code = raw.split([' ', ':']).next().unwrap_or("");
    let text = match code {
        "EMAIL_EXISTS" => crate::tr!(
            "이미 가입된 이메일이에요. 그 방법으로 로그인한 뒤 계정 설정에서 연결해 주세요",
            "That email is already registered. Sign in with it, then link this method in your account"
        ),
        "EMAIL_NOT_FOUND" | "INVALID_PASSWORD" | "INVALID_LOGIN_CREDENTIALS" => crate::tr!(
            "이메일이나 비밀번호가 맞지 않아요",
            "The email or password is wrong"
        ),
        "INVALID_EMAIL" | "MISSING_EMAIL" => {
            crate::tr!("이메일 주소를 확인해 주세요", "Check the email address")
        }
        "WEAK_PASSWORD" | "MISSING_PASSWORD" => crate::tr!(
            "비밀번호는 6자 이상이어야 해요",
            "The password needs at least 6 characters"
        ),
        "USER_DISABLED" => crate::tr!("사용이 중지된 계정이에요", "This account is disabled"),
        "TOO_MANY_ATTEMPTS_TRY_LATER" => crate::tr!(
            "시도가 너무 많아요. 잠시 뒤에 다시 해 주세요",
            "Too many attempts. Try again in a while"
        ),
        "OPERATION_NOT_ALLOWED" => crate::tr!(
            "이 로그인 방법이 서버에서 아직 켜져 있지 않아요",
            "This sign-in method isn't turned on for the server yet"
        ),
        "FEDERATED_USER_ID_ALREADY_LINKED" | "CREDENTIAL_ALREADY_IN_USE" => crate::tr!(
            "이 계정은 이미 다른 앱 계정에 연결돼 있어요. 로그아웃한 뒤 그 방법으로 로그인해 주세요",
            "That account is already linked to another app account. Sign out, then sign in with it"
        ),
        "PROVIDER_ALREADY_LINKED" => crate::tr!(
            "이미 연결된 로그인 방법이에요",
            "That sign-in method is already linked"
        ),
        "CREDENTIAL_TOO_OLD_LOGIN_AGAIN"
        | "TOKEN_EXPIRED"
        | "INVALID_ID_TOKEN"
        | "INVALID_REFRESH_TOKEN"
        | "USER_NOT_FOUND"
        | "INVALID_GRANT_TYPE" => {
            return Error::BadToken;
        }
        "INVALID_IDP_RESPONSE" | "INVALID_CREDENTIAL" => crate::tr!(
            "로그인 정보를 확인하지 못했어요. 다시 로그인해 주세요",
            "The sign-in couldn't be verified. Please try again"
        ),
        _ => crate::tr!(
            "로그인 오류 ({status}) {raw}",
            "Sign-in error ({status}) {raw}"
        ),
    };
    Error::Other(text)
}

fn tokens(body: &Value, id_key: &str, refresh_key: &str, uid_key: &str) -> Result<Tokens> {
    let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
    let (uid, id_token, refresh_token) = (text(uid_key), text(id_key), text(refresh_key));
    if uid.is_empty() || id_token.is_empty() || refresh_token.is_empty() {
        return Err(Error::Other(crate::tr!(
            "로그인 응답이 올바르지 않아요",
            "The sign-in response is incomplete"
        )));
    }
    let seconds: u64 = body[if id_key == "id_token" {
        "expires_in"
    } else {
        "expiresIn"
    }]
    .as_str()
    .and_then(|s| s.parse().ok())
    .unwrap_or(3600);
    Ok(Tokens {
        session: Session {
            uid,
            id_token,
            expires: Instant::now() + Duration::from_secs(seconds),
        },
        refresh_token,
    })
}

impl Client<'_> {
    fn identity(&self, method: &str, body: Value) -> Result<Value> {
        let url = format!(
            "{}/v1/accounts:{method}?key={}",
            self.ends.auth, self.config.api_key
        );
        let (status, body) = send(self.agent.post(&url).send_json(body))?;
        if status >= 400 {
            return Err(auth_error(status, &body));
        }
        Ok(body)
    }

    pub fn sign_up(&self, email: &str, password: &str) -> Result<Tokens> {
        let body = self.identity(
            "signUp",
            json!({ "email": email, "password": password, "returnSecureToken": true }),
        )?;
        tokens(&body, "idToken", "refreshToken", "localId")
    }

    pub fn sign_in_password(&self, email: &str, password: &str) -> Result<Tokens> {
        let body = self.identity(
            "signInWithPassword",
            json!({ "email": email, "password": password, "returnSecureToken": true }),
        )?;
        tokens(&body, "idToken", "refreshToken", "localId")
    }

    /// Signs in with (or, given `link`, links) an identity provider's
    /// credential: `post_body` like `access_token=…&providerId=github.com`,
    /// or the redirect a `createAuthUri` sign-in came back with.
    pub fn sign_in_idp(
        &self,
        post_body: &str,
        request_uri: &str,
        session_id: Option<&str>,
        link: Option<&Session>,
    ) -> Result<Tokens> {
        let mut req = json!({
            "requestUri": request_uri,
            "returnSecureToken": true,
            "returnIdpCredential": true,
        });
        // A redirect carries the credential in `request_uri` instead.
        if !post_body.is_empty() {
            req["postBody"] = json!(post_body);
        }
        if let Some(id) = session_id {
            req["sessionId"] = json!(id);
        }
        if let Some(s) = link {
            req["idToken"] = json!(s.id_token);
        }
        let body = self.identity("signInWithIdp", req)?;
        // Same email as an account that signs in another way: Firebase wants
        // that account to sign in first and link this one.
        if body["needConfirmation"].as_bool() == Some(true) {
            return Err(Error::Other(crate::tr!(
                "같은 이메일로 가입된 계정이 있어요. 원래 방법으로 로그인한 뒤 계정 설정에서 연결해 주세요",
                "An account with this email already exists. Sign in the way you did before, then link this method in your account"
            )));
        }
        if let Some(code) = body["errorMessage"].as_str() {
            return Err(auth_error(400, &json!({ "error": { "message": code } })));
        }
        tokens(&body, "idToken", "refreshToken", "localId")
    }

    pub fn sign_in_github(&self, github_token: &str, link: Option<&Session>) -> Result<Tokens> {
        self.sign_in_idp(
            &format!("access_token={github_token}&providerId=github.com"),
            "http://localhost",
            None,
            link,
        )
    }

    /// Where to send the browser for a provider's sign-in; the provider
    /// comes back to `continue_uri`.
    pub fn auth_uri(&self, provider: &str, continue_uri: &str) -> Result<AuthUri> {
        let body = self.identity(
            "createAuthUri",
            json!({
                "providerId": provider,
                "continueUri": continue_uri,
                "customParameter": { "prompt": "select_account" },
            }),
        )?;
        let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
        let (url, session_id) = (text("authUri"), text("sessionId"));
        if url.is_empty() || session_id.is_empty() {
            return Err(Error::Other(crate::tr!(
                "로그인 주소를 받지 못했어요",
                "Didn't get a sign-in address"
            )));
        }
        Ok(AuthUri { url, session_id })
    }

    /// Renews the ID token. The refresh token may change; keep the new one.
    pub fn refresh(&self, refresh_token: &str) -> Result<Tokens> {
        let url = format!("{}/v1/token?key={}", self.ends.token, self.config.api_key);
        let (status, body) = send(self.agent.post(&url).send_form([
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ]))?;
        if status >= 400 {
            // securetoken puts the code in `error.message` too.
            return Err(auth_error(status, &body));
        }
        tokens(&body, "id_token", "refresh_token", "user_id")
    }

    pub fn profile(&self, session: &Session) -> Result<Profile> {
        let body = self.identity("lookup", json!({ "idToken": session.id_token }))?;
        let user = &body["users"][0];
        let mut providers: Vec<String> = user["providerUserInfo"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|p| p["providerId"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if user["passwordHash"].is_string() && !providers.iter().any(|p| p == "password") {
            providers.push("password".into());
        }
        providers.sort();
        providers.dedup();
        let name = user["displayName"]
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                user["providerUserInfo"].as_array().and_then(|list| {
                    list.iter()
                        .find_map(|p| p["screenName"].as_str().or(p["displayName"].as_str()))
                        .map(str::to_string)
                })
            });
        Ok(Profile {
            email: user["email"].as_str().map(str::to_string),
            email_verified: user["emailVerified"].as_bool().unwrap_or(false),
            name,
            providers,
        })
    }

    /// Adds email and password sign-in to the signed-in account.
    pub fn link_password(&self, session: &Session, email: &str, password: &str) -> Result<Tokens> {
        let body = self.identity(
            "update",
            json!({
                "idToken": session.id_token,
                "email": email,
                "password": password,
                "returnSecureToken": true,
            }),
        )?;
        tokens(&body, "idToken", "refreshToken", "localId")
    }

    /// Removes a sign-in method from the account.
    pub fn unlink(&self, session: &Session, provider: &str) -> Result<()> {
        self.identity(
            "update",
            json!({ "idToken": session.id_token, "deleteProvider": [provider] }),
        )?;
        Ok(())
    }

    pub fn send_password_reset(&self, email: &str) -> Result<()> {
        self.identity(
            "sendOobCode",
            json!({ "requestType": "PASSWORD_RESET", "email": email }),
        )?;
        Ok(())
    }

    pub fn send_verification(&self, session: &Session) -> Result<()> {
        self.identity(
            "sendOobCode",
            json!({ "requestType": "VERIFY_EMAIL", "idToken": session.id_token }),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::test_serve as serve;
    use crate::sync::firebase::{Config, Endpoints};

    fn with_client<T>(base: &str, f: impl FnOnce(&Client) -> T) -> T {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let ends = Endpoints {
            auth: base.into(),
            token: base.into(),
            firestore: base.into(),
        };
        let config = Config {
            api_key: "k".into(),
            project_id: "p".into(),
        };
        f(&Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        })
    }

    #[test]
    fn password_sign_in_refresh_and_profile() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"u1","idToken":"id1","refreshToken":"r1","expiresIn":"3600"}"#,
            ),
            (
                200,
                r#"{"user_id":"u1","id_token":"id2","refresh_token":"r2","expires_in":"3600"}"#,
            ),
            (
                200,
                r#"{"users":[{"email":"a@b.c","emailVerified":true,"passwordHash":"x","providerUserInfo":[{"providerId":"github.com","screenName":"octo"},{"providerId":"password"}]}]}"#,
            ),
        ]);
        with_client(&base, |c| {
            let t = c.sign_in_password("a@b.c", "secret").unwrap();
            assert_eq!(
                (t.session.uid.as_str(), t.refresh_token.as_str()),
                ("u1", "r1")
            );
            let t = c.refresh(&t.refresh_token).unwrap();
            assert_eq!(t.refresh_token, "r2");
            assert!(t.session.fresh());
            let p = c.profile(&t.session).unwrap();
            assert_eq!(
                p,
                Profile {
                    email: Some("a@b.c".into()),
                    email_verified: true,
                    name: Some("octo".into()),
                    providers: vec!["github.com".into(), "password".into()],
                }
            );
        });
        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("POST /v1/accounts:signInWithPassword?key=k"));
        assert!(seen[1].contains("grant_type=refresh_token&refresh_token=r1"));
        assert!(seen[2].contains(r#""idToken": "id2""#));
    }

    #[test]
    fn linking_sends_the_current_id_token() {
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"u1","idToken":"id1","refreshToken":"r1","expiresIn":"3600"}"#,
            ),
            (
                200,
                r#"{"localId":"u1","idToken":"id2","refreshToken":"r2","expiresIn":"3600"}"#,
            ),
        ]);
        with_client(&base, |c| {
            let t = c.sign_in_github("gho_x", None).unwrap();
            let linked = c.link_password(&t.session, "a@b.c", "secret").unwrap();
            assert_eq!(linked.session.uid, "u1", "same user, so the data stays");
        });
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("access_token=gho_x&providerId=github.com"));
        assert!(!seen[0].contains("idToken"));
        assert!(seen[1].contains(r#""idToken": "id1""#));
    }

    #[test]
    fn errors_read_as_actions() {
        let (base, _) = serve(vec![
            (400, r#"{"error":{"message":"EMAIL_EXISTS"}}"#),
            (
                400,
                r#"{"error":{"message":"WEAK_PASSWORD : Password should be at least 6 characters"}}"#,
            ),
            (400, r#"{"error":{"message":"INVALID_REFRESH_TOKEN"}}"#),
            (
                200,
                r#"{"needConfirmation":true,"email":"a@b.c","providerId":"google.com"}"#,
            ),
        ]);
        with_client(&base, |c| {
            let Err(Error::Other(e)) = c.sign_up("a@b.c", "secret") else {
                panic!("expected an error");
            };
            assert!(
                e.contains("이미 가입된") || e.contains("already registered"),
                "{e}"
            );
            let Err(Error::Other(e)) = c.sign_up("a@b.c", "x") else {
                panic!("expected an error");
            };
            assert!(e.contains('6'), "{e}");
            assert_eq!(c.refresh("r").unwrap_err(), Error::BadToken);
            assert!(
                c.sign_in_idp("x", "http://localhost", Some("s"), None)
                    .is_err()
            );
        });
    }
}
