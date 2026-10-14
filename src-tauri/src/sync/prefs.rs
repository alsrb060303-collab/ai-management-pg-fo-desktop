//! The portable preferences (`crate::prefs`, only the keys on its allow-list)
//! in Firestore: one small document per key in `users/{uid}/prefs/{key}`,
//! merged like the memo items (the newer `updatedAt` wins, the device id
//! breaks ties, an incremental pull by `syncedAt`).

use serde_json::{Value, json};

use crate::prefs::store::{Change, Store, SyncRow};
use crate::prefs::{SYNCED_VALUE_CHARS, is_synced};

use super::firebase::{Client, Cursor, Error, Result, Session};

pub const COLLECTION: &str = "prefs";
/// Where the last pull stopped (in the prefs database's `meta`).
const CURSOR_KEY: &str = "sync_cursor";
const UID_KEY: &str = "sync_uid";
const PUSH: usize = 100;
const PAGE: usize = 100;
/// The security rules cap the device id.
const MAX_DEVICE: usize = 100;

/// The document id is the key itself: allow-listed keys are plain words.
pub fn to_fields(row: &SyncRow) -> Value {
    // A deleted key travels without its value.
    let value = if row.deleted { "" } else { row.value.as_str() };
    json!({
        "key": { "stringValue": row.key },
        "value": { "stringValue": value },
        "updatedAt": { "integerValue": row.updated_at.to_string() },
        "deviceId": { "stringValue": row.device_id.chars().take(MAX_DEVICE).collect::<String>() },
        "deleted": { "booleanValue": row.deleted },
    })
}

/// A pulled document, or `None` when it isn't one this app syncs (a key off
/// the allow-list, a document id that doesn't match its key, missing fields,
/// a value the rules wouldn't have let in).
pub fn from_fields(doc: &str, f: &Value) -> Option<SyncRow> {
    let s = |k: &str| f[k]["stringValue"].as_str().map(str::to_string);
    let key = s("key")?;
    if key != doc || !is_synced(&key) {
        return None;
    }
    let value = s("value")?;
    if value.chars().count() > SYNCED_VALUE_CHARS {
        return None;
    }
    Some(SyncRow {
        key,
        value,
        updated_at: f["updatedAt"]["integerValue"].as_str()?.parse().ok()?,
        deleted: f["deleted"]["booleanValue"].as_bool().unwrap_or(false),
        device_id: s("deviceId").unwrap_or_default(),
    })
}

/// One round: upload what changed here, then apply what other devices
/// uploaded. Returns the keys whose value changed here.
pub fn exchange(store: &Store, client: &Client, session: &Session) -> Result<Vec<Change>> {
    // Another account than last time: its copy has none of this yet.
    if store.meta(UID_KEY).map_err(Error::Other)?.as_deref() != Some(session.uid.as_str()) {
        store.mark_all_unsynced().map_err(Error::Other)?;
        store.set_meta(CURSOR_KEY, None).map_err(Error::Other)?;
        store
            .set_meta(UID_KEY, Some(&session.uid))
            .map_err(Error::Other)?;
    }

    loop {
        let rows = store.pending(PUSH).map_err(Error::Other)?;
        if rows.is_empty() {
            break;
        }
        let docs: Vec<(String, Value)> =
            rows.iter().map(|r| (r.key.clone(), to_fields(r))).collect();
        let accepted = client.put(session, COLLECTION, &docs)?;
        let done: Vec<(String, i64)> = rows
            .iter()
            .zip(&accepted)
            .filter(|(_, ok)| **ok)
            .map(|(r, _)| (r.key.clone(), r.updated_at))
            .collect();
        store.mark_synced(&done).map_err(Error::Other)?;
        // Refused rows lost to newer ones on the server; the pull brings those.
        if done.len() < rows.len() || rows.len() < PUSH {
            break;
        }
    }

    let mut changed: Vec<Change> = Vec::new();
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
        changed.extend(store.apply_remote(&rows).map_err(Error::Other)?);
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
    use crate::github::test_serve as serve;

    use super::super::firebase::{Config, Endpoints};

    fn row(key: &str, value: &str, deleted: bool) -> SyncRow {
        SyncRow {
            key: key.into(),
            value: value.into(),
            updated_at: 42,
            deleted,
            device_id: "d1".into(),
        }
    }

    #[test]
    fn fields_round_trip() {
        let r = row("theme", "dark", false);
        assert_eq!(from_fields("theme", &to_fields(&r)), Some(r));
        // A deleted key comes back without its value.
        let gone = row("theme", "dark", true);
        let back = from_fields("theme", &to_fields(&gone)).unwrap();
        assert!(back.deleted && back.value.is_empty());
    }

    #[test]
    fn foreign_documents_are_skipped() {
        let f = to_fields(&row("theme", "dark", false));
        assert_eq!(from_fields("other", &f), None, "id doesn't match the key");
        let local_only = to_fields(&row("recentProjects", "[]", false));
        assert_eq!(from_fields("recentProjects", &local_only), None);
        let long = to_fields(&row("theme", &"x".repeat(SYNCED_VALUE_CHARS + 1), false));
        assert_eq!(from_fields("theme", &long), None);
    }

    #[test]
    fn documents_match_the_rules_allow_list() {
        let fields = to_fields(&row("theme", "dark", false));
        let rules = include_str!("../../../firebase/firestore.rules");
        for key in fields.as_object().unwrap().keys() {
            assert!(rules.contains(&format!("'{key}'")), "{key} is in the rules");
        }
        assert!(rules.contains("'syncedAt'"));
    }

    #[test]
    fn exchange_uploads_synced_keys_only_and_applies_remote_ones() {
        let store = Store::in_memory().unwrap();
        store.set("theme", Some("dark")).unwrap();
        store.set("recentProjects", Some("[]")).unwrap();

        let remote = json!([{ "document": {
            "name": "projects/p/databases/(default)/documents/users/u1/prefs/language",
            "fields": {
                "key": { "stringValue": "language" },
                "value": { "stringValue": "en" },
                "updatedAt": { "integerValue": "9" },
                "deviceId": { "stringValue": "laptop" },
                "deleted": { "booleanValue": false },
                "syncedAt": { "timestampValue": "2026-10-14T00:00:00Z" },
            },
        }}])
        .to_string();
        let (base, seen) = serve(vec![
            (
                200,
                r#"{"localId":"u1","idToken":"t","refreshToken":"r","expiresIn":"3600"}"#,
            ),
            (200, r#"{"writeResults":[{}],"status":[{}]}"#),
            (200, Box::leak(remote.into_boxed_str())),
        ]);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let ends = Endpoints {
            auth: base.clone(),
            token: base.clone(),
            firestore: base,
        };
        let config = Config {
            api_key: "k".into(),
            project_id: "p".into(),
        };
        let client = Client {
            agent: &agent,
            ends: &ends,
            config: &config,
        };
        let session = client.sign_in_github("gho", None).unwrap().session;
        let changed = exchange(&store, &client, &session).unwrap();

        assert_eq!(changed, [("language".to_string(), Some("en".to_string()))]);
        assert_eq!(store.get("language").unwrap().as_deref(), Some("en"));
        assert!(store.pending(10).unwrap().is_empty(), "uploaded and marked");
        assert_eq!(store.meta(UID_KEY).unwrap().as_deref(), Some("u1"));
        assert!(store.meta(CURSOR_KEY).unwrap().is_some());
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains("prefs/theme"), "{}", seen[1]);
        assert!(!seen[1].contains("recentProjects"));
        assert!(seen[2].contains("prefs"), "{}", seen[2]);
    }
}
