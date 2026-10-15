//! The work-tree logs (`crate::activity`) in Firestore: one small document per
//! event in `users/{uid}/activity/{id}`, merged like the memo items (the newer
//! `updatedAt` wins, the device id breaks ties, an incremental pull by
//! `syncedAt`). Prompt events only travel while "sync prompts" is on: when it
//! is off they are neither uploaded nor applied.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::activity::store::{Event, Store, SyncRow};
use crate::activity::{GitEvent, GitKind, PromptEvent};

use super::firebase::{Client, Cursor, Error, Result, Session};

pub const COLLECTION: &str = "activity";
/// Where the last pull stopped (in the activity database's `meta`).
pub const CURSOR_KEY: &str = "sync_cursor";
const UID_KEY: &str = "sync_uid";
const PUSH: usize = 100;
const PAGE: usize = 100;
/// The security rules allow `text` up to 600 characters and lists up to 200.
const MAX_TEXT: usize = 500;
const MAX_LIST: usize = 200;
/// Per-field string limits of the security rules (`eventId` is left alone:
/// the document id is derived from it).
const MAX_REPO: usize = 1000;
const MAX_DEVICE: usize = 100;
const MAX_BRANCH: usize = 500;
const MAX_SHA: usize = 100;
const MAX_STATE: usize = 200;
const MAX_URL: usize = 1000;
const MAX_AUTHOR: usize = 300;
const MAX_SESSION: usize = 200;
const MAX_KIND: usize = 30;
/// Rules cap list sizes, not their items; keep those sane too.
const MAX_ITEM: usize = 500;

/// The document id for an event: its id contains `:` and branch names, which
/// documents can't hold, so it is hashed. The event id travels in `eventId`.
pub fn doc_id(event_id: &str) -> String {
    let digest = Sha256::digest(event_id.as_bytes());
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("e-{hex}")
}

fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn string(s: &str) -> Value {
    json!({ "stringValue": s })
}

fn opt(s: &Option<String>, max: usize) -> Value {
    string(&cut(s.as_deref().unwrap_or(""), max))
}

fn int(n: i64) -> Value {
    json!({ "integerValue": n.to_string() })
}

fn list(items: &[String]) -> Value {
    let values: Vec<Value> = items
        .iter()
        .take(MAX_LIST)
        .map(|s| string(&cut(s, MAX_ITEM)))
        .collect();
    if values.is_empty() {
        json!({ "arrayValue": {} })
    } else {
        json!({ "arrayValue": { "values": values } })
    }
}

fn kind_name(kind: GitKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Every field is always present (empty string / 0 / empty list for "none"),
/// which keeps the security rules simple.
pub fn to_fields(row: &SyncRow) -> Value {
    // A deleted row travels without its content, whatever the caller holds.
    let blanked;
    let row = if row.deleted {
        blanked = SyncRow {
            event: crate::activity::store::tombstone(&row.event),
            ..row.clone()
        };
        &blanked
    } else {
        row
    };
    let (kind, id, repo, at) = match &row.event {
        Event::Git(e) => ("git", &e.id, &e.repo, e.at),
        Event::Prompt(e) => ("prompt", &e.id, &e.repo, e.at),
    };
    let mut f = json!({
        "kind": string(kind),
        "eventId": string(id),
        "repo": string(&cut(repo, MAX_REPO)),
        "at": int(at),
        "updatedAt": int(row.updated_at),
        "deleted": { "booleanValue": row.deleted },
        "deviceId": string(&cut(&row.device_id, MAX_DEVICE)),
    });
    let extra = match &row.event {
        Event::Git(e) => json!({
            "evKind": string(&cut(&kind_name(e.kind), MAX_KIND)),
            "branch": opt(&e.branch, MAX_BRANCH),
            "sha": opt(&e.sha, MAX_SHA),
            "parents": list(&e.parents),
            "pr": int(e.pr.unwrap_or(0) as i64),
            "text": string(&cut(&e.title, MAX_TEXT)),
            "state": opt(&e.state, MAX_STATE),
            "url": opt(&e.url, MAX_URL),
            "author": opt(&e.author, MAX_AUTHOR),
            "tool": string(""),
            "session": string(""),
            "files": list(&[]),
            "commits": list(&[]),
            "pushed": { "booleanValue": false },
        }),
        Event::Prompt(e) => json!({
            "evKind": string(""),
            "branch": opt(&e.branch, MAX_BRANCH),
            "sha": string(""),
            "parents": list(&[]),
            "pr": int(0),
            "text": string(&cut(&e.text, MAX_TEXT)),
            "state": string(""),
            "url": string(""),
            "author": string(""),
            "tool": string(&cut(&tool_name(e), MAX_KIND)),
            "session": string(&cut(&e.session, MAX_SESSION)),
            "files": list(&e.files),
            "commits": list(&e.commits),
            "pushed": { "booleanValue": e.pushed },
        }),
    };
    if let (Some(f), Some(extra)) = (f.as_object_mut(), extra.as_object()) {
        f.extend(extra.clone());
    }
    f
}

fn tool_name(e: &PromptEvent) -> String {
    serde_json::to_value(e.tool)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A pulled document, or `None` when it isn't one this app wrote (unknown
/// kind, missing fields, or a document id that doesn't match its event).
pub fn from_fields(doc: &str, f: &Value) -> Option<SyncRow> {
    let s = |k: &str| f[k]["stringValue"].as_str().map(str::to_string);
    let nonempty = |k: &str| s(k).filter(|v| !v.is_empty());
    let i = |k: &str| {
        f[k]["integerValue"]
            .as_str()
            .and_then(|v| v.parse::<i64>().ok())
    };
    let strings = |k: &str| -> Vec<String> {
        f[k]["arrayValue"]["values"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v["stringValue"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let b = |k: &str| f[k]["booleanValue"].as_bool().unwrap_or(false);

    let id = s("eventId")?;
    if doc != doc_id(&id) {
        return None;
    }
    let repo = s("repo")?;
    let at = i("at")?;
    let event = match s("kind")?.as_str() {
        "git" => Event::Git(GitEvent {
            id,
            repo,
            kind: serde_json::from_value(Value::String(s("evKind")?)).ok()?,
            at,
            branch: nonempty("branch"),
            sha: nonempty("sha"),
            parents: strings("parents"),
            pr: i("pr").filter(|n| *n > 0).map(|n| n as u64),
            title: s("text").unwrap_or_default(),
            state: nonempty("state"),
            url: nonempty("url"),
            author: nonempty("author"),
        }),
        "prompt" => Event::Prompt(PromptEvent {
            id,
            repo,
            tool: serde_json::from_value(Value::String(s("tool")?)).ok()?,
            session: s("session")?,
            at,
            branch: nonempty("branch"),
            text: s("text").unwrap_or_default(),
            files: strings("files"),
            commits: strings("commits"),
            pushed: b("pushed"),
        }),
        _ => return None,
    };
    Some(SyncRow {
        updated_at: i("updatedAt")?,
        deleted: b("deleted"),
        device_id: s("deviceId").unwrap_or_default(),
        event,
    })
}

/// One round: upload what changed here, then apply what other devices
/// uploaded. Returns the repositories whose logs changed.
pub fn exchange(store: &Store, client: &Client, session: &Session) -> Result<Vec<String>> {
    // Another account than last time: its copy has none of this yet.
    if store.meta(UID_KEY).map_err(Error::Other)?.as_deref() != Some(session.uid.as_str()) {
        store.mark_all_unsynced().map_err(Error::Other)?;
        store.set_meta(CURSOR_KEY, None).map_err(Error::Other)?;
        store
            .set_meta(UID_KEY, Some(&session.uid))
            .map_err(Error::Other)?;
    }
    let prompts = store.settings().map_err(Error::Other)?.sync_prompts;

    loop {
        let rows = store.pending(PUSH, prompts).map_err(Error::Other)?;
        if rows.is_empty() {
            break;
        }
        let docs: Vec<(String, Value)> = rows
            .iter()
            .map(|r| (doc_id(r.event.id()), to_fields(r)))
            .collect();
        let accepted = client.put(session, COLLECTION, &docs)?;
        let done: Vec<(String, i64)> = rows
            .iter()
            .zip(&accepted)
            .filter(|(_, ok)| **ok)
            .map(|(r, _)| (r.event.id().to_string(), r.updated_at))
            .collect();
        store.mark_synced(&done).map_err(Error::Other)?;
        // Refused rows lost to newer ones on the server; the pull brings those.
        if done.len() < rows.len() || rows.len() < PUSH {
            break;
        }
    }

    let mut changed: Vec<String> = Vec::new();
    let mut cursor = store
        .meta(CURSOR_KEY)
        .map_err(Error::Other)?
        .and_then(|c| Cursor::decode(&c));
    loop {
        let (docs, last) = client.changes(session, COLLECTION, cursor.as_ref(), PAGE)?;
        let count = docs.len();
        let rows: Vec<SyncRow> = docs
            .iter()
            .filter_map(|(id, fields)| from_fields(id, fields))
            .collect();
        for repo in store.apply_remote(&rows, prompts).map_err(Error::Other)? {
            if !changed.contains(&repo) {
                changed.push(repo);
            }
        }
        if let Some(last) = last {
            store
                .set_meta(CURSOR_KEY, Some(&last.encode()))
                .map_err(Error::Other)?;
            cursor = Some(last);
        }
        if count < PAGE {
            break;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::store::Settings;
    use crate::github::test_serve as serve;
    use crate::tools::ToolId;

    use super::super::firebase::{Config, Endpoints};

    fn git_row() -> SyncRow {
        SyncRow {
            updated_at: 42,
            deleted: false,
            device_id: "d1".into(),
            event: Event::Git(GitEvent {
                id: "git:pr_merged:7".into(),
                repo: "github.com/o/r".into(),
                kind: GitKind::PrMerged,
                at: 1000,
                branch: Some("feat/x".into()),
                sha: Some("abc".into()),
                parents: vec!["p1".into()],
                pr: Some(7),
                title: "Add x".into(),
                state: Some("main".into()),
                url: Some("https://github.com/o/r/pull/7".into()),
                author: None,
            }),
        }
    }

    fn prompt_row() -> SyncRow {
        SyncRow {
            updated_at: 43,
            deleted: true,
            device_id: "d2".into(),
            event: Event::Prompt(PromptEvent {
                id: "prompt:codex:s1:3".into(),
                repo: "github.com/o/r".into(),
                tool: ToolId::Codex,
                session: "s1".into(),
                at: 2000,
                branch: None,
                text: "로그인 고쳐 줘".into(),
                files: vec!["src/a.rs".into(), "src/b.rs".into()],
                commits: vec!["abc".into()],
                pushed: true,
            }),
        }
    }

    #[test]
    fn fields_round_trip_for_both_kinds() {
        for row in [git_row(), prompt_row()] {
            let fields = to_fields(&row);
            let doc = doc_id(row.event.id());
            // A deleted row comes back without its content.
            let want = if row.deleted {
                SyncRow {
                    event: crate::activity::store::tombstone(&row.event),
                    ..row.clone()
                }
            } else {
                row
            };
            assert_eq!(from_fields(&doc, &fields), Some(want));
        }
    }

    #[test]
    fn deleted_rows_upload_without_content() {
        for mut row in [git_row(), prompt_row()] {
            row.deleted = true;
            let f = to_fields(&row);
            assert_eq!(f["deleted"]["booleanValue"], true);
            for k in ["text", "url", "author", "branch"] {
                assert_eq!(f[k]["stringValue"], "", "{k}");
            }
            for k in ["parents", "files", "commits"] {
                assert!(f[k]["arrayValue"]["values"].is_null(), "{k}");
            }
            assert_eq!(f["eventId"]["stringValue"], row.event.id());
        }
    }

    #[test]
    fn every_string_field_is_clamped_to_the_rule_limits() {
        let long = "x".repeat(5000);
        let mut row = git_row();
        row.device_id = long.clone();
        if let Event::Git(e) = &mut row.event {
            e.repo = long.clone();
            e.branch = Some(long.clone());
            e.sha = Some(long.clone());
            e.state = Some(long.clone());
            e.url = Some(long.clone());
            e.author = Some(long.clone());
            e.parents = vec![long.clone(); 300];
            e.title = long.clone();
        }
        let f = to_fields(&row);
        let len = |k: &str| f[k]["stringValue"].as_str().unwrap().chars().count();
        assert!(len("repo") <= 1000 && len("deviceId") <= 100);
        assert!(len("branch") <= 500 && len("sha") <= 100 && len("state") <= 200);
        assert!(len("url") <= 1000 && len("author") <= 300 && len("text") <= 600);
        assert!(len("evKind") <= 30 && len("tool") <= 30 && len("session") <= 200);
        assert_eq!(
            f["parents"]["arrayValue"]["values"]
                .as_array()
                .unwrap()
                .len(),
            200
        );
    }

    #[test]
    fn documents_are_small_and_match_the_rules_allow_list() {
        let mut row = prompt_row();
        row.deleted = false;
        if let Event::Prompt(e) = &mut row.event {
            e.text = "가".repeat(900);
            e.files = (0..500).map(|i| format!("f{i}")).collect();
        }
        let fields = to_fields(&row);
        assert_eq!(
            fields["text"]["stringValue"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            500
        );
        assert_eq!(
            fields["files"]["arrayValue"]["values"]
                .as_array()
                .unwrap()
                .len(),
            200
        );
        let mut keys: Vec<_> = fields.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        let rules = include_str!("../../../firebase/firestore.rules");
        for key in keys {
            assert!(rules.contains(&format!("'{key}'")), "{key} is in the rules");
        }
    }

    #[test]
    fn foreign_or_mismatched_documents_are_skipped() {
        let row = git_row();
        let fields = to_fields(&row);
        assert_eq!(
            from_fields("e-other", &fields),
            None,
            "id doesn't match the event"
        );
        let mut odd = fields.clone();
        odd["kind"] = string("alien");
        assert_eq!(from_fields(&doc_id(row.event.id()), &odd), None);
    }

    fn client_parts(base: String) -> (ureq::Agent, Endpoints, Config) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        (
            agent,
            Endpoints {
                auth: base.clone(),
                token: base.clone(),
                firestore: base,
            },
            Config {
                api_key: "k".into(),
                project_id: "p".into(),
            },
        )
    }

    #[test]
    fn exchange_uploads_local_events_and_applies_remote_ones() {
        let store = Store::in_memory().unwrap();
        store
            .upsert(&[
                git_row().event,
                Event::Prompt(match prompt_row().event {
                    Event::Prompt(p) => p,
                    Event::Git(_) => unreachable!(),
                }),
            ])
            .unwrap();

        let theirs = to_fields(&SyncRow {
            updated_at: 9,
            deleted: false,
            device_id: "laptop".into(),
            event: Event::Git(GitEvent {
                id: "git:commit:zzz:gh".into(),
                repo: "github.com/o/r".into(),
                kind: GitKind::Commit,
                at: 5,
                branch: None,
                sha: Some("zzz".into()),
                parents: vec![],
                pr: None,
                title: "from laptop".into(),
                state: None,
                url: None,
                author: None,
            }),
        });
        let doc = doc_id("git:commit:zzz:gh");
        let remote = json!([{ "document": {
            "name": format!("projects/p/databases/(default)/documents/users/u1/activity/{doc}"),
            "fields": theirs.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone()))
                .chain([("syncedAt".to_string(), json!({ "timestampValue": "2026-10-14T00:00:00Z" }))])
                .collect::<serde_json::Map<_, _>>(),
        }}])
        .to_string();
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"u1","idToken":"t","refreshToken":"r","expiresIn":"3600"}"#,
            ),
            (200, r#"{"writeResults":[{},{}],"status":[{},{}]}"#),
            (200, Box::leak(remote.into_boxed_str())),
        ]);
        let (agent, ends, config) = client_parts(base);
        let client = Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        };
        let session = client.sign_in_github("gho", None).unwrap().session;
        let changed = exchange(&store, &client, &session).unwrap();

        assert_eq!(changed, ["github.com/o/r"]);
        assert!(store.pending(10, true).unwrap().is_empty());
        let log = store.git_page("github.com/o/r", 10, None, None).unwrap();
        assert!(log.iter().any(|e| e.title == "from laptop"));
        assert_eq!(store.meta(UID_KEY).unwrap().as_deref(), Some("u1"));
        assert!(store.meta(CURSOR_KEY).unwrap().is_some());
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains("/users/u1/activity/") || seen[1].contains(":batchWrite"));
        assert!(seen[1].contains(&doc_id("prompt:codex:s1:3")));
        assert!(
            seen[2].contains("collectionId") && seen[2].contains("activity"),
            "{}",
            seen[2]
        );
    }

    #[test]
    fn prompts_stay_home_while_prompt_sync_is_off() {
        let store = Store::in_memory().unwrap();
        store.upsert(&[git_row().event]).unwrap();
        store
            .upsert(&[Event::Prompt(match prompt_row().event {
                Event::Prompt(p) => p,
                Event::Git(_) => unreachable!(),
            })])
            .unwrap();
        store
            .set_settings(Settings {
                retention_days: Some(90),
                sync_prompts: false,
            })
            .unwrap();

        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"u1","idToken":"t","refreshToken":"r","expiresIn":"3600"}"#,
            ),
            (200, r#"{"writeResults":[{}],"status":[{}]}"#),
            (200, "[]"),
        ]);
        let (agent, ends, config) = client_parts(base);
        let client = Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        };
        let session = client.sign_in_github("gho", None).unwrap().session;
        exchange(&store, &client, &session).unwrap();

        let seen = seen.lock().unwrap();
        assert!(seen[1].contains(&doc_id("git:pr_merged:7")));
        assert!(!seen[1].contains(&doc_id("prompt:codex:s1:3")));
        // Still waiting for the day sync is turned on.
        assert_eq!(store.pending(10, true).unwrap().len(), 1);
    }
}
