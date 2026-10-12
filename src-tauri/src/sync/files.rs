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

use std::collections::{BTreeMap, BTreeSet};
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
/// And under its limit on writes per request.
const UPLOAD_COUNT: usize = 200;

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
    /// For a deletion's notice on other devices.
    #[serde(default)]
    pub name: String,
    /// The server copy's `updatedAt`: an upload must be later to be accepted.
    #[serde(default)]
    pub updated_at: i64,
}

impl Base {
    fn of(item: &Item) -> Base {
        Base {
            kind: item.kind,
            key: item.key.clone(),
            hash: item.hash.clone(),
            deleted: item.deleted,
            name: item.name.clone(),
            updated_at: item.updated_at,
        }
    }
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

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

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

/// A path segment every OS accepts (the library's own name rules, which
/// cover Windows: reserved names, trailing dots and spaces, `<>:"|?*`).
fn safe_segment(s: &str) -> bool {
    crate::library::check_id(s).is_ok() && !s.starts_with('.')
}

/// A relative path inside a skill that is safe to write on any device.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty() && !rel.contains('\\') && rel.split('/').all(safe_segment)
}

/// A skill's text files as `relative path → text`, and how many files were
/// left out (binary, too big, hidden, links, or names other systems refuse).
fn skill_files(dir: &Path) -> (BTreeMap<String, String>, usize) {
    fn visit(
        root: &Path,
        dir: &Path,
        depth: usize,
        out: &mut BTreeMap<String, String>,
        skipped: &mut usize,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            *skipped += 1;
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                *skipped += 1;
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !safe_segment(&name) || meta.file_type().is_symlink() {
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
            let Ok(text) = fs::read(&path).map(String::from_utf8) else {
                *skipped += 1;
                continue;
            };
            let Ok(text) = text else {
                *skipped += 1;
                continue;
            };
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(rel, text);
        }
    }
    let mut out = BTreeMap::new();
    let mut skipped = 0;
    visit(dir, dir, 0, &mut out, &mut skipped);
    (out, skipped)
}

/// Where an item lives on this device.
fn path_of(roots: &Roots, kind: Kind, key: &str) -> PathBuf {
    match kind {
        Kind::Doc => roots.docs.join(format!("{key}.json")),
        Kind::Skill => roots.library.join("skills").join(key),
        Kind::Agent => roots.library.join("agents").join(format!("{key}.md")),
    }
}

/// Why an item on disk can't be read now; `None` when it isn't there.
enum Read {
    Ok(Item),
    /// There, but not syncable right now (unreadable, not text, invalid,
    /// too big): never treated as deleted, and never overwritten.
    Held(String),
    Missing,
}

fn read_item(roots: &Roots, kind: Kind, key: &str, device: &str) -> Read {
    let path = path_of(roots, kind, key);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Missing,
        Err(_) => return Read::Held(key.to_string()),
    };
    if meta.file_type().is_symlink() {
        return Read::Held(key.to_string());
    }
    let item = |name: String, data: String, updated_at: i64| Item {
        kind,
        key: key.to_string(),
        name,
        hash: hash(&data),
        data,
        // A clock far ahead mustn't make an edit win every conflict.
        updated_at: updated_at.min(now_ms()),
        deleted: false,
        device_id: device.to_string(),
    };
    let found = match kind {
        Kind::Doc => {
            let Ok(data) = fs::read_to_string(&path) else {
                return Read::Held(key.to_string());
            };
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                return Read::Held(key.to_string());
            };
            let updated_at = value["updatedAt"]
                .as_f64()
                .map(|v| v.round() as i64)
                .unwrap_or_else(|| mtime_ms(&path));
            let name = value["name"].as_str().unwrap_or(key).to_string();
            item(name, data, updated_at)
        }
        Kind::Agent => {
            let Ok(data) = fs::read_to_string(&path) else {
                return Read::Held(key.to_string());
            };
            item(key.to_string(), data, mtime_ms(&path))
        }
        Kind::Skill => {
            let (files, _) = skill_files(&path);
            if !files.contains_key("SKILL.md") {
                return Read::Held(key.to_string());
            }
            let updated_at = files
                .keys()
                .map(|rel| mtime_ms(&path.join(rel)))
                .max()
                .unwrap_or_else(|| mtime_ms(&path));
            let data = serde_json::to_string(&files).unwrap_or_default();
            item(key.to_string(), data, updated_at)
        }
    };
    if found.data.len() > MAX_DATA {
        Read::Held(found.name)
    } else {
        Read::Ok(found)
    }
}

/// Everything on this device that syncs.
#[derive(Debug, Default)]
pub struct Scan {
    pub items: BTreeMap<String, Item>,
    /// On disk but not syncable now, by file id (with a display name).
    pub held: BTreeMap<String, String>,
    /// Skills with files that stay here (binary, large, hidden).
    pub partial: Vec<String>,
    /// Kinds whose folder couldn't be listed: nothing of that kind counts as
    /// deleted this round.
    pub unlisted: Vec<Kind>,
}

/// Lists a folder; `None` when it can't be read (a missing folder is empty).
fn list_dir(dir: &Path) -> Option<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => Some(entries.flatten().collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
        Err(_) => None,
    }
}

pub fn scan(roots: &Roots, device: &str) -> Scan {
    let mut out = Scan::default();
    let sources = [
        (Kind::Doc, roots.docs.clone(), ".json"),
        (Kind::Skill, roots.library.join("skills"), ""),
        (Kind::Agent, roots.library.join("agents"), ".md"),
    ];
    for (kind, dir, ext) in sources {
        let Some(entries) = list_dir(&dir) else {
            out.unlisted.push(kind);
            continue;
        };
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = if ext.is_empty() {
                name.as_str()
            } else {
                match name.strip_suffix(ext) {
                    Some(k) => k,
                    None => continue,
                }
            };
            let valid = match kind {
                Kind::Doc => valid_doc_id(key),
                _ => safe_segment(key),
            };
            if !valid {
                continue;
            }
            let id = file_id(kind, key);
            match read_item(roots, kind, key, device) {
                Read::Ok(item) => {
                    if kind == Kind::Skill && skill_files(&entry.path()).1 > 0 {
                        out.partial.push(key.to_string());
                    }
                    out.items.insert(id, item);
                }
                Read::Held(name) => {
                    out.held.insert(id, name);
                }
                Read::Missing => {}
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Writing what arrives

fn check_key(kind: Kind, key: &str) -> std::result::Result<(), String> {
    let ok = match kind {
        Kind::Doc => valid_doc_id(key),
        Kind::Skill | Kind::Agent => safe_segment(key),
    };
    if ok {
        Ok(())
    } else {
        Err(crate::tr!(
            "이름을 쓸 수 없어요: {key}",
            "Unusable name: {key}"
        ))
    }
}

/// Refuses to write through a link that is already on the way to `path`
/// (from `root` down), which could point anywhere.
fn no_links(root: &Path, path: &Path) -> std::result::Result<(), String> {
    let Ok(rel) = path.strip_prefix(root) else {
        return Err("outside the folder".into());
    };
    let mut at = root.to_path_buf();
    for part in rel.components() {
        at.push(part);
        if fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(crate::tr!(
                "링크(바로가기)가 있어서 건너뛰었어요: {}",
                "Skipped because of a link: {}",
                at.display()
            ));
        }
    }
    Ok(())
}

/// Puts `item` in place on this device, or removes it if it's deleted.
/// Returns a note when a deleted skill's folder was kept because it still
/// holds files that never sync.
pub fn apply(roots: &Roots, item: &Item) -> std::result::Result<Option<String>, String> {
    check_key(item.kind, &item.key)?;
    let path = path_of(roots, item.kind, &item.key);
    let root = match item.kind {
        Kind::Doc => roots.docs.clone(),
        _ => roots.library.clone(),
    };
    no_links(&root, &path)?;
    match item.kind {
        Kind::Doc => {
            if item.deleted {
                return ops::remove_path(&path).map(|_| None);
            }
            let mut value: Value = serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            if !value.is_object() {
                return Err("not a document".into());
            }
            value["id"] = Value::from(item.key.clone());
            fs::create_dir_all(&roots.docs).map_err(|e| e.to_string())?;
            let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
            ops::write_atomic(&path, text).map(|_| None)
        }
        Kind::Agent => {
            if item.deleted {
                return ops::remove_path(&path).map(|_| None);
            }
            fs::create_dir_all(roots.library.join("agents")).map_err(|e| e.to_string())?;
            ops::write_atomic(&path, &item.data).map(|_| None)
        }
        Kind::Skill => {
            let here = skill_files(&path).0;
            if item.deleted {
                // Only what syncs goes; anything else keeps the folder.
                for rel in here.keys() {
                    ops::remove_path(&path.join(rel))?;
                }
                remove_empty_dirs(&path);
                return Ok(path.exists().then(|| item.key.clone()));
            }
            let files: BTreeMap<String, String> =
                serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            let mut lower = BTreeSet::new();
            let clash = files.keys().any(|r| !lower.insert(r.to_lowercase()));
            if !files.contains_key("SKILL.md") || clash || !files.keys().all(|r| safe_rel(r)) {
                return Err(crate::tr!(
                    "skill 파일 이름이 올바르지 않아요: {}",
                    "A skill file name isn't valid: {}",
                    item.key
                ));
            }
            for (rel, text) in &files {
                let file = path.join(rel);
                no_links(&root, &file)?;
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                ops::write_atomic(&file, text)?;
            }
            // Text files the other device no longer has; files that never
            // sync (binary, large, hidden) stay.
            for rel in here.keys() {
                if !files.contains_key(rel) {
                    ops::remove_path(&path.join(rel))?;
                }
            }
            remove_empty_dirs(&path);
            Ok(None)
        }
    }
}

/// Removes empty folders below and including `dir`.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
                remove_empty_dirs(&path);
            }
        }
    }
    let _ = fs::remove_dir(dir);
}

/// Saves `item`'s content as a new item next to the original, named with
/// `label` ("다른 기기" / "이 기기"). Returns the copy's display name.
pub fn keep_copy(
    roots: &Roots,
    item: &Item,
    label: &str,
    stamp: i64,
) -> std::result::Result<String, String> {
    // A fresh key that isn't taken (never derived by cutting the original).
    let mut n = stamp;
    let key = loop {
        let key = match item.kind {
            Kind::Doc => format!("d{n:x}"),
            _ => format!("{}-conflict-{n:x}", item.key),
        };
        if key != item.key && fs::symlink_metadata(path_of(roots, item.kind, &key)).is_err() {
            break key;
        }
        n += 1;
    };
    let name = format!("{} ({label})", item.name);
    let data = match item.kind {
        Kind::Doc => {
            let mut value: Value = serde_json::from_str(&item.data).map_err(|e| e.to_string())?;
            value["name"] = Value::from(name.clone());
            serde_json::to_string(&value).map_err(|e| e.to_string())?
        }
        _ => item.data.clone(),
    };
    let copy = Item {
        name: name.clone(),
        hash: hash(&data),
        data,
        deleted: false,
        ..item.clone()
    };
    apply(roots, &Item { key, ..copy })?;
    Ok(name)
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
    let unchanged_here = match (local, base) {
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
        _ if unchanged_here => Action::Take,
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
    /// Documents this round changed on disk (by id).
    pub doc_ids: Vec<String>,
    pub library: bool,
    pub conflicts: Vec<Conflict>,
    pub partial: Vec<String>,
    /// Items that stay out of sync for now (too big, unreadable, invalid).
    pub held: Vec<String>,
    /// Items from other devices not applied here: `(name, why)`.
    pub failed: Vec<(String, String)>,
    /// Deleted skills whose folders were kept for files that never sync.
    pub kept: Vec<String>,
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
    // the server yet, and nothing missing here was deleted on purpose. Read
    // the server from the start again so every item meets what is here.
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
        state.cursor = None;
    }

    let local = scan(roots, device);
    // Ours won a conflict: upload it as later than theirs.
    let mut bump: BTreeMap<String, i64> = BTreeMap::new();
    // Something was skipped or refused: read everything again next round.
    let mut again = false;

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
            match take(
                roots,
                &local,
                &state,
                &id,
                &remote,
                device,
                &mut outcome,
                &mut bump,
            ) {
                Ok(()) => {
                    state.bases.insert(id, Base::of(&remote));
                }
                Err(why) => {
                    outcome.failed.push((remote.name.clone(), why));
                    again = true;
                }
            }
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

    // 2. Upload what changed here (read again: taking and copying changed it).
    let local = scan(roots, device);
    outcome.partial = local.partial.clone();
    outcome.held = local.held.values().cloned().collect();
    let mut uploads: Vec<(String, Item)> = Vec::new();
    for (id, item) in &local.items {
        let base = state.bases.get(id);
        if base.is_some_and(|b| !b.deleted && b.hash == item.hash) {
            continue;
        }
        let mut item = item.clone();
        // Later than the server's copy, or the rules refuse it.
        let floor = base
            .map_or(0, |b| b.updated_at + 1)
            .max(bump.get(id).copied().unwrap_or(0));
        item.updated_at = item.updated_at.max(floor);
        uploads.push((id.clone(), item));
    }
    for (id, base) in &state.bases {
        // Deleted here: only when its folder could be listed and the item
        // is really gone (not just unreadable for now).
        if base.deleted
            || local.items.contains_key(id)
            || local.held.contains_key(id)
            || local.unlisted.contains(&base.kind)
            || fs::symlink_metadata(path_of(roots, base.kind, &base.key)).is_ok()
        {
            continue;
        }
        uploads.push((
            id.clone(),
            Item {
                kind: base.kind,
                key: base.key.clone(),
                name: if base.name.is_empty() {
                    base.key.clone()
                } else {
                    base.name.clone()
                },
                data: String::new(),
                hash: String::new(),
                updated_at: now_ms().max(base.updated_at + 1),
                deleted: true,
                device_id: device.to_string(),
            },
        ));
    }
    let mut start = 0;
    while start < uploads.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < uploads.len()
            && end - start < UPLOAD_COUNT
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
            if ok {
                state.bases.insert(id.clone(), Base::of(item));
            } else {
                // A newer copy is on the server: meet it on the next round.
                again = true;
            }
        }
        save_state(state_path, &state).map_err(other)?;
        start = end;
    }

    if again {
        state.cursor = None;
        save_state(state_path, &state).map_err(other)?;
    }
    Ok(outcome)
}

/// Applies one item from the server according to [`decide`].
#[allow(clippy::too_many_arguments)]
fn take(
    roots: &Roots,
    local: &Scan,
    state: &State,
    id: &str,
    remote: &Item,
    device: &str,
    outcome: &mut Outcome,
    bump: &mut BTreeMap<String, i64>,
) -> std::result::Result<(), String> {
    if let Some(name) = local.held.get(id) {
        return Err(crate::tr!(
            "이 기기의 '{name}'을(를) 지금 읽을 수 없어서 건너뛰었어요",
            "Skipped: '{name}' on this device can't be read right now"
        ));
    }
    let here = local.items.get(id);
    // A name that differs only in case is the same file on Windows and macOS.
    if here.is_none()
        && !remote.deleted
        && local.items.values().any(|i| {
            i.kind == remote.kind
                && i.key != remote.key
                && i.key.to_lowercase() == remote.key.to_lowercase()
        })
    {
        return Err(crate::tr!(
            "대소문자만 다른 같은 이름이 이 기기에 있어요",
            "This device has the same name in different case"
        ));
    }
    // The user may have saved since the scan: then it's a conflict, next round.
    let now = match read_item(roots, remote.kind, &remote.key, device) {
        Read::Ok(i) => Some(i.hash),
        Read::Missing => None,
        Read::Held(_) => Some(String::new()),
    };
    if now != here.map(|i| i.hash.clone()) {
        return Err(crate::tr!(
            "방금 바뀌어서 다음 동기화 때 맞춰요",
            "Just changed; it's matched on the next sync"
        ));
    }
    let base = state.bases.get(id);
    let stamp = now_ms();
    let changed = |o: &mut Outcome| match remote.kind {
        Kind::Doc => o.doc_ids.push(remote.key.clone()),
        _ => o.library = true,
    };
    let revived = |o: &mut Outcome, name: &str| {
        o.conflicts.push(Conflict {
            kind: remote.kind,
            name: name.to_string(),
            copy: None,
            revived: true,
        })
    };
    match decide(here, base, remote) {
        Action::Agree => {}
        Action::Take => {
            if let Some(kept) = apply(roots, remote)? {
                outcome.kept.push(kept);
            }
            changed(outcome);
            if here.is_none() && base.is_some_and(|b| !b.deleted) && !remote.deleted {
                revived(outcome, &remote.name);
            }
        }
        Action::Keep => {
            if remote.deleted
                && let Some(h) = here
            {
                revived(outcome, &h.name);
                bump.insert(id.to_string(), remote.updated_at + 1);
            }
        }
        Action::TakeAndCopyOurs => {
            let ours = here.ok_or("missing")?;
            let copy = keep_copy(roots, ours, &crate::tr!("이 기기", "this device"), stamp)?;
            apply(roots, remote)?;
            changed(outcome);
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
                remote,
                &crate::tr!("다른 기기", "other device"),
                stamp,
            )?;
            changed(outcome);
            outcome.conflicts.push(Conflict {
                kind: remote.kind,
                name: remote.name.clone(),
                copy: Some(copy),
                revived: false,
            });
            bump.insert(id.to_string(), remote.updated_at + 1);
        }
    }
    Ok(())
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
        Base::of(i)
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
            serde_json::from_str(&fs::read_to_string(roots.docs.join("dff.json")).unwrap())
                .unwrap();
        assert_eq!(
            (copy["id"].as_str(), copy["name"].as_str()),
            (Some("dff"), Some("Plan (다른 기기)"))
        );
        // The next copy at the same moment takes a fresh id.
        keep_copy(&roots, d, "다른 기기", 255).unwrap();
        assert!(roots.docs.join("d100.json").is_file());

        let gone = Item {
            deleted: true,
            ..agent.clone()
        };
        apply(&roots, &gone).unwrap();
        assert!(!roots.library.join("agents/reviewer.md").exists());
    }

    #[test]
    fn unreadable_items_are_held_not_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = Roots {
            docs: tmp.path().join("compositions"),
            library: tmp.path().join("library"),
        };
        fs::create_dir_all(&roots.docs).unwrap();
        fs::write(roots.docs.join("dbroken.json"), "{ not json").unwrap();
        fs::write(
            roots.docs.join("dbig.json"),
            format!("{{\"x\":\"{}\"}}", "a".repeat(MAX_DATA)),
        )
        .unwrap();
        let skill = roots.library.join("skills/no-skill-md");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("notes.md"), "x").unwrap();
        let s = scan(&roots, "me");
        assert!(s.items.is_empty());
        assert_eq!(s.held.len(), 3);
        assert!(s.unlisted.is_empty());

        // A deleted skill keeps the files that never sync.
        let files: BTreeMap<String, String> = [("SKILL.md".to_string(), "x".to_string())].into();
        let live = item(
            Kind::Skill,
            "keep",
            &serde_json::to_string(&files).unwrap(),
            0,
        );
        apply(&roots, &live).unwrap();
        fs::write(roots.library.join("skills/keep/logo.png"), [0u8, 159]).unwrap();
        let kept = apply(
            &roots,
            &Item {
                deleted: true,
                ..live.clone()
            },
        )
        .unwrap();
        assert_eq!(kept.as_deref(), Some("keep"));
        assert!(!roots.library.join("skills/keep/SKILL.md").exists());
        assert!(roots.library.join("skills/keep/logo.png").exists());
        // Without such files the folder goes.
        apply(
            &roots,
            &item(
                Kind::Skill,
                "gone",
                &serde_json::to_string(&files).unwrap(),
                0,
            ),
        )
        .unwrap();
        let gone = item(Kind::Skill, "gone", "", 0);
        assert_eq!(
            apply(
                &roots,
                &Item {
                    deleted: true,
                    ..gone
                }
            )
            .unwrap(),
            None
        );
        assert!(!roots.library.join("skills/gone").exists());
    }

    #[cfg(unix)]
    #[test]
    fn links_in_the_way_are_not_written_through() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let roots = Roots {
            docs: tmp.path().join("c"),
            library: tmp.path().join("l"),
        };
        fs::create_dir_all(roots.library.join("skills")).unwrap();
        std::os::unix::fs::symlink(&outside, roots.library.join("skills/s")).unwrap();
        let files: BTreeMap<String, String> = [("SKILL.md".to_string(), "x".to_string())].into();
        assert!(
            apply(
                &roots,
                &item(Kind::Skill, "s", &serde_json::to_string(&files).unwrap(), 0)
            )
            .is_err()
        );
        assert!(!outside.join("SKILL.md").exists());
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
        assert!(
            apply(&roots, &item(Kind::Agent, "con", "x", 0)).is_err(),
            "Windows reserved name"
        );
        let case: BTreeMap<String, String> = [
            ("SKILL.md".to_string(), "x".to_string()),
            ("skill.md".to_string(), "y".to_string()),
        ]
        .into();
        assert!(
            apply(
                &roots,
                &item(Kind::Skill, "c", &serde_json::to_string(&case).unwrap(), 0)
            )
            .is_err()
        );
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
                name: "Plan".into(),
                updated_at: 1,
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
        assert!(!outcome.doc_ids.is_empty() && outcome.library);
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
