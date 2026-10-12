//! Syncing the composer's documents and the library (skills and subagents)
//! across devices, next to the memos. Each one is a single Firestore
//! document in `users/{uid}/files/{id}`: its text (a skill's text files as
//! one JSON map), a content hash, when it was last edited, and a deleted
//! flag.
//!
//! The local files stay the source of truth. `sync-files.json` in the app
//! data folder remembers the hash each item had when it was last in step
//! with the server (the "base"), which tells three cases apart:
//! - only this device changed it → upload;
//! - only another device changed it → apply theirs;
//! - both changed it → the later edit wins, and the other version is kept as
//!   a copy ("<name> (다른 기기)" / "(이 기기)") so nothing is lost; the user
//!   is told.
//!
//! What is machine-specific (project paths, `composed.json`, `applied.json`,
//! the library folder setting) is never synced.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::firebase::{Client, Cursor, Error, Result, Session};
use crate::library::ops;

pub const COLLECTION: &str = "files";
/// Firestore documents hold at most 1 MiB; leave room for the other fields.
pub const MAX_DATA: usize = 900_000;
/// Skill files bigger than this, binary or hidden ones stay on this device.
const MAX_SKILL_FILE: u64 = 512 * 1024;
const MAX_DEPTH: usize = 8;
/// Documents can be large; pages stay well under the 10 MiB response cap.
const PAGE: usize = 8;
/// One upload request stays under Firestore's 10 MiB request limit.
const UPLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    Doc,
    Skill,
    Agent,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Doc => "doc",
            Kind::Skill => "skill",
            Kind::Agent => "agent",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        match s {
            "doc" => Some(Kind::Doc),
            "skill" => Some(Kind::Skill),
            "agent" => Some(Kind::Agent),
            _ => None,
        }
    }
}

/// One document or library item as it is now, on this device or the server.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// Document id, or the library item's folder/file name.
    pub key: String,
    /// What the user sees (a document's name).
    pub name: String,
    pub data: String,
    pub hash: String,
    /// Epoch ms of the last edit.
    pub updated_at: i64,
    pub deleted: bool,
    pub device_id: String,
}

/// Firestore document id: stable for a kind and key, and safe whatever
/// characters the key has.
pub fn file_id(kind: Kind, key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{hex}", kind.as_str())
}

pub fn hash(data: &str) -> String {
    format!("{:x}", Sha256::digest(data.as_bytes()))
}

/// What this device last agreed with the server on, per item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    pub kind: Kind,
    pub key: String,
    pub hash: String,
    pub deleted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub uid: Option<String>,
    pub cursor: Option<String>,
    /// The library folder the bases below belong to.
    pub library_dir: Option<String>,
    #[serde(default)]
    pub bases: BTreeMap<String, Base>,
}

pub fn load_state(path: &Path) -> State {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_state(path: &Path, state: &State) -> std::result::Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    ops::write_atomic(path, text)
}

/// Where the synced things live on this device.
#[derive(Clone, Debug)]
pub struct Roots {
    /// `<app data>/compositions`
    pub docs: PathBuf,
    /// The library folder (`skills/`, `agents/`).
    pub library: PathBuf,
}

// ---------------------------------------------------------------------------
// Reading what is here

fn mtime_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn valid_doc_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A skill's text files as `relative path → text`, and how many files were
/// left out (binary, too big, hidden or links).
fn skill_files(dir: &Path) -> (BTreeMap<String, String>, usize) {
    fn visit(
        root: &Path,
        dir: &Path,
        depth: usize,
        out: &mut BTreeMap<String, String>,
        skipped: &mut usize,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if name.starts_with('.') || meta.file_type().is_symlink() {
                *skipped += 1;
                continue;
            }
            if meta.is_dir() {
                if depth < MAX_DEPTH {
                    visit(root, &path, depth + 1, out, skipped);
                } else {
                    *skipped += 1;
                }
                continue;
            }
            if meta.len() > MAX_SKILL_FILE {
                *skipped += 1;
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                *skipped += 1;
                continue;
            };
            let Ok(text) = String::from_utf8(bytes) else {
                *skipped += 1;
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .map(|p| {
                    p.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .unwrap_or(name);
            out.insert(rel, text);
        }
    }
    let mut out = BTreeMap::new();
    let mut skipped = 0;
    visit(dir, dir, 0, &mut out, &mut skipped);
    (out, skipped)
}

fn newest_mtime(dir: &Path, files: &BTreeMap<String, String>) -> i64 {
    files
        .keys()
        .map(|rel| mtime_ms(&dir.join(rel)))
        .max()
        .unwrap_or_else(|| mtime_ms(dir))
}

/// Everything on this device that syncs. `partial` names skills with files
/// that stay here; `too_big` names items over the size limit (not synced).
#[derive(Debug, Default)]
pub struct Scan {
    pub items: BTreeMap<String, Item>,
    pub partial: Vec<String>,
    pub too_big: Vec<String>,
}

pub fn scan(roots: &Roots, device: &str) -> Scan {
    let mut out = Scan::default();
    let add = |out: &mut Scan, item: Item| {
        if item.data.len() > MAX_DATA {
            out.too_big.push(item.name.clone());
        } else {
            out.items.insert(file_id(item.kind, &item.key), item);
        }
    };
    if let Ok(entries) = fs::read_dir(&roots.docs) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "json"))
            else {
                continue;
            };
            if !valid_doc_id(id) {
                continue;
            }
            let Ok(data) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let updated_at = value["updatedAt"]
                .as_f64()
                .map(|v| v.round() as i64)
                .unwrap_or_else(|| mtime_ms(&path));
            add(
                &mut out,
                Item {
                    kind: Kind::Doc,
                    key: id.to_string(),
                    name: value["name"].as_str().unwrap_or(id).to_string(),
                    hash: hash(&data),
                    data,
                    updated_at,
                    deleted: false,
                    device_id: device.to_string(),
                },
            );
        }
    }
    if let Ok(entries) = fs::read_dir(roots.library.join("skills")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.starts_with('.')
                || crate::library::check_id(&id).is_err()
                || !dir.join("SKILL.md").is_file()
            {
                continue;
            }
            let (files, skipped) = skill_files(&dir);
            if skipped > 0 {
                out.partial.push(id.clone());
            }
            let data = serde_json::to_string(&files).unwrap_or_default();
            add(
                &mut out,
                Item {
                    kind: Kind::Skill,
                    updated_at: newest_mtime(&dir, &files),
                    name: id.clone(),
                    key: id,
                    hash: hash(&data),
                    data,
                    deleted: false,
                    device_id: device.to_string(),
                },
            );
        }
    }
    if let Ok(entries) = fs::read_dir(roots.library.join("agents")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "md"))
            else {
                continue;
            };
            if id.starts_with('.') || crate::library::check_id(id).is_err() {
                continue;
            }
            let Ok(data) = fs::read_to_string(&path) else {
                continue;
            };
            add(
                &mut out,
                Item {
                    kind: Kind::Agent,
                    key: id.to_string(),
                    name: id.to_string(),
                    hash: hash(&data),
                    data,
                    updated_at: mtime_ms(&path),
                    deleted: false,
                    device_id: device.to_string(),
                },
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Writing what arrives

fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\\')
        && !rel.contains(':')
        && rel
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.starts_with('.'))
}

fn check_key(kind: Kind, key: &str) -> std::result::Result<(), String> {
    let ok = match kind {
        Kind::Doc => valid_doc_id(key),
        Kind::Skill | Kind::Agent => crate::library::check_id(key).is_ok() && !key.starts_with('.'),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("bad key: {key}"))
    }
}

/// Puts `item` in place on this device, or removes it if it's deleted.
pub fn apply(roots: &Roots, item: &Item) -> std::result::Result<(), String> {
    check_key(item.kind, &item.key)?;
    match item.kind {
        Kind::Doc => {
            let path = roots.docs.join(format!("{}.json", item.key));
            if item.deleted {
                return ops::remove_path(&path);
            }
            let mut value: Value = serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            if !value.is_object() {
                return Err("not a document".into());
            }
            value["id"] = Value::from(item.key.clone());
            fs::create_dir_all(&roots.docs).map_err(|e| e.to_string())?;
            ops::write_atomic(
                &path,
                serde_json::to_string(&value).map_err(|e| e.to_string())?,
            )
        }
        Kind::Agent => {
            let path = roots
                .library
                .join("agents")
                .join(format!("{}.md", item.key));
            if item.deleted {
                return ops::remove_path(&path);
            }
            fs::create_dir_all(roots.library.join("agents")).map_err(|e| e.to_string())?;
            ops::write_atomic(&path, &item.data)
        }
        Kind::Skill => {
            let dir = roots.library.join("skills").join(&item.key);
            if item.deleted {
                return ops::remove_path(&dir);
            }
            let files: BTreeMap<String, String> =
                serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            if !files.contains_key("SKILL.md") || !files.keys().all(|r| safe_rel(r)) {
                return Err("bad skill".into());
            }
            for (rel, text) in &files {
                let path = dir.join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                ops::write_atomic(&path, text)?;
            }
            // Text files the other device no longer has; binaries and other
            // files that never sync stay.
            for rel in skill_files(&dir).0.keys() {
                if !files.contains_key(rel) {
                    ops::remove_path(&dir.join(rel))?;
                }
            }
            Ok(())
        }
    }
}

/// Saves `item`'s content as a new item next to the original, named with
/// `label` ("다른 기기" / "이 기기"). Returns the copy's display name.
pub fn keep_copy(
    roots: &Roots,
    item: &Item,
    label: &str,
    stamp: i64,
) -> std::result::Result<String, String> {
    let suffix = format!("{stamp:x}");
    match item.kind {
        Kind::Doc => {
            let mut key = format!("{}-c{suffix}", item.key);
            key.truncate(64);
            let name = format!("{} ({label})", item.name);
            let mut value: Value = serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            value["name"] = Value::from(name.clone());
            let copy = Item {
                key,
                data: serde_json::to_string(&value).map_err(|e| e.to_string())?,
                deleted: false,
                ..item.clone()
            };
            apply(roots, &copy)?;
            Ok(name)
        }
        Kind::Skill | Kind::Agent => {
            let key = format!("{}-conflict-{suffix}", item.key);
            let copy = Item {
                key: key.clone(),
                name: key.clone(),
                deleted: false,
                ..item.clone()
            };
            apply(roots, &copy)?;
            Ok(format!("{key} ({label})"))
        }
    }
}

// ---------------------------------------------------------------------------
// Firestore fields

pub fn to_fields(item: &Item) -> Value {
    json!({
        "kind": { "stringValue": item.kind.as_str() },
        "key": { "stringValue": item.key },
        "name": { "stringValue": item.name },
        "data": { "stringValue": if item.deleted { "" } else { item.data.as_str() } },
        "hash": { "stringValue": item.hash },
        "updatedAt": { "integerValue": item.updated_at.to_string() },
        "deleted": { "booleanValue": item.deleted },
        "deviceId": { "stringValue": item.device_id },
    })
}

pub fn from_fields(f: &Value) -> Option<Item> {
    let s = |k: &str| f[k]["stringValue"].as_str().map(str::to_string);
    let kind = Kind::parse(&s("kind")?)?;
    let data = s("data").unwrap_or_default();
    let deleted = f["deleted"]["booleanValue"].as_bool().unwrap_or(false);
    let key = s("key")?;
    check_key(kind, &key).ok()?;
    Some(Item {
        kind,
        name: s("name").unwrap_or_else(|| key.clone()),
        key,
        hash: if deleted { String::new() } else { hash(&data) },
        data,
        updated_at: f["updatedAt"]["integerValue"]
            .as_str()
            .and_then(|v| v.parse().ok())?,
        deleted,
        device_id: s("deviceId").unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// Deciding

/// What to do with one item that changed on the server.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Already the same here: only remember it as the base.
    Agree,
    /// Only the server changed: take it.
    Take,
    /// Only this device changed (or deleted) it: keep ours; it uploads.
    Keep,
    /// Both changed and theirs is later: save ours as a copy, take theirs.
    TakeAndCopyOurs,
    /// Both changed and ours is later: save theirs as a copy, keep ours.
    KeepAndCopyTheirs,
}

pub fn decide(local: Option<&Item>, base: Option<&Base>, remote: &Item) -> Action {
    let unchanged_here = |l: Option<&Item>| match (l, base) {
        (Some(l), Some(b)) => !b.deleted && l.hash == b.hash,
        (None, Some(b)) => b.deleted,
        (None, None) => true,
        (Some(_), None) => false,
    };
    let remote_same_as_base = base
        .is_some_and(|b| b.deleted == remote.deleted && (remote.deleted || b.hash == remote.hash));
    match local {
        Some(l) if !remote.deleted && l.hash == remote.hash => Action::Agree,
        None if remote.deleted => Action::Agree,
        _ if unchanged_here(local) => Action::Take,
        _ if remote_same_as_base => Action::Keep,
        // Deleted here while they edited it: bring theirs back.
        None => Action::Take,
        // Edited here while they deleted it: keep ours.
        Some(_) if remote.deleted => Action::Keep,
        Some(l) if remote.updated_at > l.updated_at => Action::TakeAndCopyOurs,
        Some(_) => Action::KeepAndCopyTheirs,
    }
}

/// A conflict the user should hear about.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub kind: Kind,
    pub name: String,
    /// The copy holding the version that lost, if one was made.
    pub copy: Option<String>,
    /// Deleted on one side, edited on the other: the edit was kept.
    pub revived: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub docs: bool,
    pub library: bool,
    pub conflicts: Vec<Conflict>,
    pub partial: Vec<String>,
    pub too_big: Vec<String>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn other(e: String) -> Error {
    Error::Other(e)
}

/// One round: take what other devices changed (resolving conflicts), then
/// upload what changed here.
pub fn exchange(
    client: &Client,
    session: &Session,
    roots: &Roots,
    state_path: &Path,
    device: &str,
) -> Result<Outcome> {
    let mut state = load_state(state_path);
    let mut outcome = Outcome::default();

    // Another account, or another library folder: nothing here is known to
    // the server yet, and nothing missing here was deleted on purpose.
    if state.uid.as_deref() != Some(session.uid.as_str()) {
        state = State {
            uid: Some(session.uid.clone()),
            ..State::default()
        };
    }
    let library_dir = roots.library.display().to_string();
    if state.library_dir.as_deref() != Some(library_dir.as_str()) {
        state.bases.retain(|_, b| b.kind == Kind::Doc);
        state.library_dir = Some(library_dir);
    }

    let mut local = scan(roots, device);
    // Ours won a conflict: upload it as later than theirs.
    let mut bump: BTreeMap<String, i64> = BTreeMap::new();

    // 1. Take what other devices changed.
    let mut cursor = state.cursor.as_deref().and_then(Cursor::decode);
    loop {
        let (docs, last) = client.changes(session, COLLECTION, cursor.as_ref(), PAGE)?;
        let count = docs.len();
        for (id, fields) in docs {
            let Some(remote) = from_fields(&fields) else {
                continue;
            };
            if id != file_id(remote.kind, &remote.key) {
                continue;
            }
            let here = local.items.get(&id).cloned();
            let base = state.bases.get(&id).cloned();
            let action = decide(here.as_ref(), base.as_ref(), &remote);
            let changed_here = |o: &mut Outcome| match remote.kind {
                Kind::Doc => o.docs = true,
                _ => o.library = true,
            };
            let stamp = now_ms();
            match action {
                Action::Agree => {}
                Action::Take => {
                    apply(roots, &remote).map_err(other)?;
                    changed_here(&mut outcome);
                    if let (None, Some(b)) = (&here, &base)
                        && !b.deleted
                        && !remote.deleted
                    {
                        outcome.conflicts.push(Conflict {
                            kind: remote.kind,
                            name: remote.name.clone(),
                            copy: None,
                            revived: true,
                        });
                    }
                }
                Action::Keep => {
                    if remote.deleted && here.is_some() {
                        outcome.conflicts.push(Conflict {
                            kind: remote.kind,
                            name: remote.name.clone(),
                            copy: None,
                            revived: true,
                        });
                        bump.insert(id.clone(), remote.updated_at + 1);
                    }
                }
                Action::TakeAndCopyOurs => {
                    let ours = here.clone().expect("local exists");
                    let copy =
                        keep_copy(roots, &ours, &crate::tr!("이 기기", "this device"), stamp)
                            .map_err(other)?;
                    apply(roots, &remote).map_err(other)?;
                    changed_here(&mut outcome);
                    outcome.conflicts.push(Conflict {
                        kind: remote.kind,
                        name: remote.name.clone(),
                        copy: Some(copy),
                        revived: false,
                    });
                }
                Action::KeepAndCopyTheirs => {
                    let copy = keep_copy(
                        roots,
                        &remote,
                        &crate::tr!("다른 기기", "other device"),
                        stamp,
                    )
                    .map_err(other)?;
                    changed_here(&mut outcome);
                    outcome.conflicts.push(Conflict {
                        kind: remote.kind,
                        name: remote.name.clone(),
                        copy: Some(copy),
                        revived: false,
                    });
                    bump.insert(id.clone(), remote.updated_at + 1);
                }
            }
            // The server's version is now the common ground.
            state.bases.insert(
                id.clone(),
                Base {
                    kind: remote.kind,
                    key: remote.key.clone(),
                    hash: remote.hash.clone(),
                    deleted: remote.deleted,
                },
            );
        }
        if let Some(last) = last {
            state.cursor = Some(last.encode());
            cursor = Some(last);
        }
        save_state(state_path, &state).map_err(other)?;
        if count < PAGE {
            break;
        }
    }

    // 2. Upload what changed here (re-read: taking and copying changed it).
    if outcome.docs || outcome.library || !outcome.conflicts.is_empty() {
        local = scan(roots, device);
    }
    outcome.partial = local.partial.clone();
    outcome.too_big = local.too_big.clone();
    let mut uploads: Vec<(String, Item)> = Vec::new();
    for (id, item) in &local.items {
        let base = state.bases.get(id);
        if base.is_some_and(|b| !b.deleted && b.hash == item.hash) {
            continue;
        }
        let mut item = item.clone();
        if let Some(at) = bump.get(id) {
            item.updated_at = item.updated_at.max(*at);
        }
        uploads.push((id.clone(), item));
    }
    for (id, base) in &state.bases {
        if !base.deleted && !local.items.contains_key(id) {
            uploads.push((
                id.clone(),
                Item {
                    kind: base.kind,
                    key: base.key.clone(),
                    name: base.key.clone(),
                    data: String::new(),
                    hash: String::new(),
                    updated_at: now_ms(),
                    deleted: true,
                    device_id: device.to_string(),
                },
            ));
        }
    }
    let mut start = 0;
    while start < uploads.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < uploads.len()
            && (end == start || bytes + uploads[end].1.data.len() < UPLOAD_BYTES)
        {
            bytes += uploads[end].1.data.len();
            end += 1;
        }
        let chunk = &uploads[start..end];
        let docs: Vec<(String, Value)> = chunk
            .iter()
            .map(|(id, item)| (id.clone(), to_fields(item)))
            .collect();
        let accepted = client.put(session, COLLECTION, &docs)?;
        for ((id, item), ok) in chunk.iter().zip(accepted) {
            // Refused: a newer copy is on the server; the next pull decides.
            if ok {
                state.bases.insert(
                    id.clone(),
                    Base {
                        kind: item.kind,
                        key: item.key.clone(),
                        hash: item.hash.clone(),
                        deleted: item.deleted,
                    },
                );
            }
        }
        save_state(state_path, &state).map_err(other)?;
        start = end;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind, key: &str, data: &str, at: i64) -> Item {
        Item {
            kind,
            key: key.into(),
            name: key.into(),
            data: data.into(),
            hash: hash(data),
            updated_at: at,
            deleted: false,
            device_id: "d".into(),
        }
    }

    fn base_of(i: &Item) -> Base {
        Base {
            kind: i.kind,
            key: i.key.clone(),
            hash: i.hash.clone(),
            deleted: i.deleted,
        }
    }

    #[test]
    fn decisions_cover_one_side_both_sides_and_deletes() {
        let old = item(Kind::Doc, "d1", "{\"v\":1}", 1);
        let mine = item(Kind::Doc, "d1", "{\"v\":2}", 5);
        let theirs = item(Kind::Doc, "d1", "{\"v\":3}", 9);
        let gone = Item {
            deleted: true,
            hash: String::new(),
            data: String::new(),
            ..theirs.clone()
        };
        let b = base_of(&old);
        // Only they changed it.
        assert_eq!(decide(Some(&old), Some(&b), &theirs), Action::Take);
        // Only we changed it (the server still has the base).
        assert_eq!(decide(Some(&mine), Some(&b), &old), Action::Keep);
        // Both changed: the later edit wins, the other is copied.
        assert_eq!(
            decide(Some(&mine), Some(&b), &theirs),
            Action::TakeAndCopyOurs
        );
        let late_mine = Item {
            updated_at: 99,
            ..mine.clone()
        };
        assert_eq!(
            decide(Some(&late_mine), Some(&b), &theirs),
            Action::KeepAndCopyTheirs
        );
        // First sync of two devices: same content agrees, different conflicts.
        assert_eq!(decide(Some(&theirs), None, &theirs), Action::Agree);
        assert_eq!(decide(Some(&mine), None, &theirs), Action::TakeAndCopyOurs);
        // New on the server.
        assert_eq!(decide(None, None, &theirs), Action::Take);
        // Deleted there, unchanged here: delete here.
        assert_eq!(decide(Some(&old), Some(&b), &gone), Action::Take);
        // Deleted there, edited here: keep ours.
        assert_eq!(decide(Some(&mine), Some(&b), &gone), Action::Keep);
        // Deleted here, unchanged there: our delete stands (uploads).
        assert_eq!(decide(None, Some(&b), &old), Action::Keep);
        // Deleted here, edited there: bring theirs back.
        assert_eq!(decide(None, Some(&b), &theirs), Action::Take);
        // Both deleted.
        assert_eq!(decide(None, Some(&b), &gone), Action::Agree);
    }

    #[test]
    fn scan_apply_and_copies_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = Roots {
            docs: tmp.path().join("compositions"),
            library: tmp.path().join("library"),
        };
        let doc = item(
            Kind::Doc,
            "dabc",
            r#"{"id":"dabc","name":"Plan","updatedAt":123.6,"nodes":[]}"#,
            0,
        );
        apply(&roots, &doc).unwrap();
        let skill_files: BTreeMap<String, String> = [
            (
                "SKILL.md".to_string(),
                "---\nname: s\n---\nbody".to_string(),
            ),
            ("ref/notes.md".to_string(), "more".to_string()),
        ]
        .into();
        let skill = item(
            Kind::Skill,
            "my-skill",
            &serde_json::to_string(&skill_files).unwrap(),
            0,
        );
        apply(&roots, &skill).unwrap();
        fs::write(
            roots.library.join("skills/my-skill/logo.png"),
            [0u8, 159, 146, 150],
        )
        .unwrap();
        let agent = item(Kind::Agent, "reviewer", "---\nname: reviewer\n---\nx", 0);
        apply(&roots, &agent).unwrap();

        let s = scan(&roots, "dev");
        assert_eq!(s.items.len(), 3);
        let d = &s.items[&file_id(Kind::Doc, "dabc")];
        assert_eq!((d.name.as_str(), d.updated_at), ("Plan", 124));
        let k = &s.items[&file_id(Kind::Skill, "my-skill")];
        assert_eq!(
            k.hash, skill.hash,
            "binary file stays out of the synced text"
        );
        assert_eq!(s.partial, ["my-skill"]);

        // An update drops a text file but keeps the binary.
        let fewer: BTreeMap<String, String> = [("SKILL.md".to_string(), "new".to_string())].into();
        apply(
            &roots,
            &item(
                Kind::Skill,
                "my-skill",
                &serde_json::to_string(&fewer).unwrap(),
                0,
            ),
        )
        .unwrap();
        assert!(!roots.library.join("skills/my-skill/ref/notes.md").exists());
        assert!(roots.library.join("skills/my-skill/logo.png").exists());

        let name = keep_copy(&roots, d, "다른 기기", 255).unwrap();
        assert_eq!(name, "Plan (다른 기기)");
        let copy: Value =
            serde_json::from_str(&fs::read_to_string(roots.docs.join("dabc-cff.json")).unwrap())
                .unwrap();
        assert_eq!(
            (copy["id"].as_str(), copy["name"].as_str()),
            (Some("dabc-cff"), Some("Plan (다른 기기)"))
        );

        let gone = Item {
            deleted: true,
            ..agent.clone()
        };
        apply(&roots, &gone).unwrap();
        assert!(!roots.library.join("agents/reviewer.md").exists());
    }

    #[test]
    fn unsafe_remote_items_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = Roots {
            docs: tmp.path().join("c"),
            library: tmp.path().join("l"),
        };
        let evil: BTreeMap<String, String> = [
            ("SKILL.md".to_string(), "x".to_string()),
            ("../../escape.md".to_string(), "x".to_string()),
        ]
        .into();
        assert!(
            apply(
                &roots,
                &item(Kind::Skill, "s", &serde_json::to_string(&evil).unwrap(), 0)
            )
            .is_err()
        );
        assert!(apply(&roots, &item(Kind::Agent, "../x", "x", 0)).is_err());
        assert!(apply(&roots, &item(Kind::Doc, "Bad Id", "{}", 0)).is_err());
        assert!(!tmp.path().join("escape.md").exists());
    }

    #[test]
    fn a_round_takes_theirs_keeps_ours_as_a_copy_and_uploads() {
        use super::super::firebase::{Config, Endpoints};
        let tmp = tempfile::tempdir().unwrap();
        let roots = Roots {
            docs: tmp.path().join("compositions"),
            library: tmp.path().join("library"),
        };
        let state_path = tmp.path().join("sync-files.json");
        let v1 = r#"{"id":"dabc","name":"Plan","updatedAt":1,"nodes":[1]}"#;
        let v2 = r#"{"id":"dabc","name":"Plan","updatedAt":5,"nodes":[2]}"#;
        let v3 = r#"{"id":"dabc","name":"Plan","updatedAt":9,"nodes":[3]}"#;
        fs::create_dir_all(&roots.docs).unwrap();
        fs::write(roots.docs.join("dabc.json"), v2).unwrap();
        let id = file_id(Kind::Doc, "dabc");
        let mut state = State {
            uid: Some("u1".into()),
            library_dir: Some(roots.library.display().to_string()),
            ..State::default()
        };
        state.bases.insert(
            id.clone(),
            Base {
                kind: Kind::Doc,
                key: "dabc".into(),
                hash: hash(v1),
                deleted: false,
            },
        );
        save_state(&state_path, &state).unwrap();

        let remote = |id: &str, item: &Item, at: &str| {
            json!({ "document": {
                "name": format!("projects/p/databases/(default)/documents/users/u1/files/{id}"),
                "fields": {
                    "syncedAt": { "timestampValue": at },
                    "kind": to_fields(item)["kind"], "key": to_fields(item)["key"],
                    "name": to_fields(item)["name"], "data": to_fields(item)["data"],
                    "hash": to_fields(item)["hash"], "updatedAt": to_fields(item)["updatedAt"],
                    "deleted": to_fields(item)["deleted"], "deviceId": to_fields(item)["deviceId"],
                }
            }})
        };
        let theirs = Item {
            name: "Plan".into(),
            ..item(Kind::Doc, "dabc", v3, 9)
        };
        let agent = item(Kind::Agent, "reviewer", "---\nname: reviewer\n---\nx", 3);
        let page = json!([
            remote(&id, &theirs, "2026-10-12T00:00:00Z"),
            remote(
                &file_id(Kind::Agent, "reviewer"),
                &agent,
                "2026-10-12T00:00:01Z"
            ),
        ])
        .to_string();
        let (base, seen) = crate::github::test_serve(vec![
            (200, r#"{"localId":"u1","idToken":"t","expiresIn":"3600"}"#),
            (200, Box::leak(page.into_boxed_str())),
            (200, r#"{"writeResults":[{}],"status":[{}]}"#),
        ]);
        let agent_http: ureq::Agent = ureq::Agent::config_builder()
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
        let client = Client {
            agent: &agent_http,
            ends: &ends,
            config: &config,
        };
        let session = client.sign_in("gho").unwrap();

        let outcome = exchange(&client, &session, &roots, &state_path, "me").unwrap();

        // Theirs (later) is in place; ours survives as a copy.
        assert!(
            fs::read_to_string(roots.docs.join("dabc.json"))
                .unwrap()
                .contains("[3]")
        );
        assert_eq!(fs::read_dir(&roots.docs).unwrap().count(), 2);
        assert_eq!(outcome.conflicts.len(), 1);
        assert!(
            outcome.conflicts[0]
                .copy
                .as_deref()
                .unwrap()
                .starts_with("Plan (")
        );
        assert!(outcome.docs && outcome.library);
        assert!(roots.library.join("agents/reviewer.md").is_file());
        // Only the copy is new here, so only it is uploaded.
        let seen = seen.lock().unwrap();
        assert!(
            seen[2].contains(":batchWrite") && seen[2].contains("Plan ("),
            "{}",
            seen[2]
        );
        let state = load_state(&state_path);
        assert_eq!(state.bases.len(), 3);
        assert!(state.cursor.is_some());
    }

    #[test]
    fn fields_round_trip_and_ids_are_stable() {
        let i = item(Kind::Agent, "한글 이름", "text", 42);
        let back = from_fields(&to_fields(&i)).unwrap();
        assert_eq!(back, i);
        assert_eq!(file_id(Kind::Agent, "a"), file_id(Kind::Agent, "a"));
        assert_ne!(file_id(Kind::Agent, "a"), file_id(Kind::Skill, "a"));
        assert!(
            file_id(Kind::Doc, "x/y")
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        );
    }
}
