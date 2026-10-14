//! The prompt log's source: Claude Code and Codex session files, read only.
//!
//! Neither tool documents these files, so everything is lenient: lines that
//! do not parse, and types or fields we do not know, are skipped, and a
//! format change only costs the sessions it breaks (docs/research.md §15).
//! Credential files are never opened; only `projects/*/*.jsonl` (Claude Code)
//! and `sessions/**/rollout-*` plus `archived_sessions/` (Codex).
//!
//! `scan` is incremental. `state` remembers, per file, how far it was read and
//! the context needed to continue (working folder, branch, the prompt that
//! tool uses attach to), so only appended bytes are read next time. A prompt
//! whose files, commits or push changed later is returned again with the same
//! id; the store upserts it. At most ~20 MB are read per call; the rest of
//! the history follows on the next calls.
//!
//! A file seen for the first time whose first working folder is outside the
//! repository is skipped without reading the rest of it (only what is
//! appended later is looked at). So worktrees added to `dirs` later do not
//! bring back sessions that were already skipped.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PromptEvent;
use crate::tools::ToolId;

/// Read at most this many bytes of session files per `scan` call.
const BUDGET: u64 = 20 * 1024 * 1024;
/// How much of a new file is looked at to find its working folder.
const SNIFF: u64 = 64 * 1024;
/// A compressed Codex session is read up to this many bytes decompressed.
const MAX_DECOMPRESSED: u64 = 128 * 1024 * 1024;
/// Distinct files kept per prompt.
const MAX_FILES: usize = 500;
/// Tool calls waiting for their result.
const MAX_PENDING: usize = 200;

pub struct Roots {
    /// `<Claude config dir>/projects`.
    pub claude_projects: PathBuf,
    /// `CODEX_HOME` or `~/.codex`.
    pub codex_home: PathBuf,
}

impl Roots {
    pub fn detect(home: &Path) -> Self {
        Roots {
            claude_projects: crate::tools::claude::Env::detect(home.to_path_buf())
                .config_dir
                .join("projects"),
            codex_home: crate::tools::codex::Env::detect(home.to_path_buf()).codex_home,
        }
    }
}

/// Prompts from sessions whose working folder is inside one of `dirs` (the
/// repository's worktrees), new since `state` (opaque; this updates it).
pub fn scan(
    roots: &Roots,
    dirs: &[PathBuf],
    repo: &str,
    state: &mut serde_json::Value,
) -> Vec<PromptEvent> {
    let mut st: State = serde_json::from_value(state.clone()).unwrap_or_default();
    let mut dirs = Dirs::new(dirs);
    let mut out = Vec::new();
    let mut budget = BUDGET;

    for path in claude_files(&roots.claude_projects) {
        if budget == 0 {
            break;
        }
        let mut cx = Ctx::new(repo, ToolId::Claude, &mut dirs, &mut out, &path);
        process_jsonl(&path, &mut st, &mut cx, &mut budget, claude_line);
    }
    for path in codex_files(&roots.codex_home) {
        if budget == 0 {
            break;
        }
        let mut cx = Ctx::new(repo, ToolId::Codex, &mut dirs, &mut out, &path);
        if path.extension().is_some_and(|e| e == "zst") {
            process_zst(&path, &mut st, &mut cx, &mut budget);
        } else {
            process_jsonl(&path, &mut st, &mut cx, &mut budget, codex_line);
        }
    }

    // Forget files that are gone.
    st.files.retain(|p, _| Path::new(p).exists());
    *state = serde_json::to_value(&st).unwrap_or(Value::Null);
    dedupe(out)
}

/// One event per id (the last one), in the order the ids first appeared.
fn dedupe(events: Vec<PromptEvent>) -> Vec<PromptEvent> {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<PromptEvent> = Vec::new();
    for e in events {
        match at.get(&e.id) {
            Some(&i) => out[i] = e,
            None => {
                at.insert(e.id.clone(), out.len());
                out.push(e);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- state

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    files: BTreeMap<String, FileState>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct FileState {
    len: u64,
    /// Modified time, ms since the epoch.
    mtime: i64,
    /// Bytes processed (up to the last complete line).
    offset: u64,
    /// Lines seen so far (Codex prompt ids use the line index).
    line: u64,
    /// Not a session to read (Codex sub-agent or `exec`).
    skip: bool,
    session: String,
    cwd: String,
    branch: Option<String>,
    /// The prompt that tool uses attach to.
    cur: Option<Cur>,
    /// Tool call id → what its result is checked for (1 commit, 2 push).
    pending: BTreeMap<String, u8>,
    /// Codex: activity lines since the current prompt (to drop duplicates).
    since: u32,
    /// Codex: start of the current prompt's text.
    last_text: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Cur {
    id: String,
    session: String,
    at: i64,
    branch: Option<String>,
    text: String,
    files: Vec<String>,
    commits: Vec<String>,
    pushed: bool,
    /// Changed since it was last returned.
    dirty: bool,
}

impl Cur {
    fn event(&self, repo: &str, tool: ToolId) -> PromptEvent {
        PromptEvent {
            id: self.id.clone(),
            repo: repo.to_string(),
            tool,
            session: self.session.clone(),
            at: self.at,
            branch: self.branch.clone(),
            text: self.text.clone(),
            files: self.files.clone(),
            commits: self.commits.clone(),
            pushed: self.pushed,
        }
    }

    fn add_file(&mut self, rel: String) {
        if self.files.len() < MAX_FILES && !self.files.contains(&rel) {
            self.files.push(rel);
            self.dirty = true;
        }
    }

    fn add_commit(&mut self, sha: String) {
        if !self.commits.contains(&sha) {
            self.commits.push(sha);
            self.dirty = true;
        }
    }

    fn set_pushed(&mut self) {
        if !self.pushed {
            self.pushed = true;
            self.dirty = true;
        }
    }
}

struct Ctx<'a> {
    repo: &'a str,
    tool: ToolId,
    dirs: &'a mut Dirs,
    out: &'a mut Vec<PromptEvent>,
    /// Fallback time for lines without a timestamp.
    mtime: i64,
    /// Session id from the file name, until the file says otherwise.
    stem_session: String,
}

impl<'a> Ctx<'a> {
    fn new(
        repo: &'a str,
        tool: ToolId,
        dirs: &'a mut Dirs,
        out: &'a mut Vec<PromptEvent>,
        path: &Path,
    ) -> Self {
        let mtime = fs::metadata(path).map(|m| mtime_ms(&m)).unwrap_or(0);
        let stem = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = stem
            .strip_suffix(".zst")
            .unwrap_or(&stem)
            .strip_suffix(".jsonl")
            .unwrap_or(&stem)
            .to_string();
        let stem_session = match tool {
            // rollout-2026-10-14T01-02-03-<36 char thread id>
            ToolId::Codex if stem.len() > 36 && stem.is_char_boundary(stem.len() - 36) => {
                stem[stem.len() - 36..].to_string()
            }
            _ => stem,
        };
        Ctx {
            repo,
            tool,
            dirs,
            out,
            mtime,
            stem_session,
        }
    }
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Returns the current prompt if it changed since it was last returned.
fn flush(f: &mut FileState, cx: &mut Ctx) {
    if let Some(c) = f.cur.as_mut()
        && c.dirty
    {
        cx.out.push(c.event(cx.repo, cx.tool));
        c.dirty = false;
    }
}

// ---------------------------------------------------------------- paths

/// Normalized: `/` separators, no `\\?\` prefix, no trailing slash.
fn norm(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.strip_prefix("//?/").unwrap_or(&p);
    p.trim_end_matches('/').to_string()
}

fn is_abs(p: &str) -> bool {
    p.starts_with('/') || p.as_bytes().get(1) == Some(&b':')
}

/// Resolves `.` and `..`.
fn clean(p: &str) -> String {
    let lead = p.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for s in p.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| !l.ends_with(':')) {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    if lead { format!("/{joined}") } else { joined }
}

/// `path` equals `dir` or is below it.
fn within(path: &str, dir: &str, case_insensitive: bool) -> bool {
    let rest = if case_insensitive {
        let (p, d) = (path.to_ascii_lowercase(), dir.to_ascii_lowercase());
        p.strip_prefix(d.as_str()).map(|r| r.to_string())
    } else {
        path.strip_prefix(dir).map(|r| r.to_string())
    };
    rest.is_some_and(|r| r.is_empty() || r.starts_with('/'))
}

/// `path` (normalized, absolute) relative to `dir`, `/`-separated.
fn rel(dir: &str, path: &str, case_insensitive: bool) -> Option<String> {
    if !is_abs(path) || !within(path, dir, case_insensitive) {
        return None;
    }
    let r = path[dir.len()..].trim_start_matches('/');
    (!r.is_empty()).then(|| r.to_string())
}

fn abs_path(cwd: &str, p: &str) -> String {
    let p = norm(p);
    if is_abs(&p) {
        clean(&p)
    } else {
        clean(&format!("{}/{p}", norm(cwd)))
    }
}

/// The repository's folders, to tell whether a working folder is inside.
struct Dirs {
    /// Raw and canonical forms, longest first.
    forms: Vec<String>,
    cache: HashMap<String, Option<String>>,
    ci: bool,
}

impl Dirs {
    fn new(dirs: &[PathBuf]) -> Self {
        let mut forms: Vec<String> = Vec::new();
        for d in dirs {
            let raw = norm(&d.to_string_lossy());
            if !raw.is_empty() && !forms.contains(&raw) {
                forms.push(raw);
            }
            if let Ok(c) = fs::canonicalize(d) {
                let c = norm(&c.to_string_lossy());
                if !c.is_empty() && !forms.contains(&c) {
                    forms.push(c);
                }
            }
        }
        forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
        Dirs {
            forms,
            cache: HashMap::new(),
            ci: cfg!(windows),
        }
    }

    /// The (longest) repository folder `cwd` is in.
    fn find(&mut self, cwd: &str) -> Option<String> {
        if cwd.is_empty() {
            return None;
        }
        if let Some(hit) = self.cache.get(cwd) {
            return hit.clone();
        }
        let n = clean(&norm(cwd));
        let mut hit = self.lookup(&n);
        if hit.is_none()
            && let Ok(c) = fs::canonicalize(cwd)
        {
            hit = self.lookup(&clean(&norm(&c.to_string_lossy())));
        }
        self.cache.insert(cwd.to_string(), hit.clone());
        hit
    }

    fn lookup(&self, n: &str) -> Option<String> {
        self.forms.iter().find(|d| within(n, d, self.ci)).cloned()
    }
}

// ---------------------------------------------------------------- reading

fn process_jsonl(
    path: &Path,
    st: &mut State,
    cx: &mut Ctx,
    budget: &mut u64,
    handler: fn(&Value, u64, &mut FileState, &mut Ctx),
) {
    let Ok(meta) = fs::metadata(path) else { return };
    let (len, mtime) = (meta.len(), mtime_ms(&meta));
    let key = path.to_string_lossy().into_owned();
    let existing = st.files.remove(&key);
    let mut f = existing.unwrap_or_default();
    if f.skip || (f.len == len && f.mtime == mtime && f.offset >= len) {
        st.files.insert(key, f);
        return;
    }
    if len < f.offset {
        // Rewritten or truncated: start over.
        f = FileState::default();
    }
    if *budget == 0 {
        if f.len != 0 {
            st.files.insert(key, f);
        }
        return;
    }
    if f.offset == 0 && sniff_outside(path, cx.dirs) {
        f.len = len;
        f.mtime = mtime;
        f.offset = len;
        st.files.insert(key, f);
        return;
    }
    if f.session.is_empty() {
        f.session = cx.stem_session.clone();
    }

    let want = (len - f.offset).min(*budget);
    let mut buf = Vec::new();
    if let Ok(mut file) = File::open(path)
        && file.seek(SeekFrom::Start(f.offset)).is_ok()
    {
        let _ = file.take(want).read_to_end(&mut buf);
    }
    *budget = budget.saturating_sub(buf.len() as u64);
    // Only complete lines: the writer may be in the middle of one.
    let used = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    for line in buf[..used].split_inclusive(|b| *b == b'\n') {
        let idx = f.line;
        f.line += 1;
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            handler(&v, idx, &mut f, cx);
            if f.skip {
                break;
            }
        }
    }
    f.offset = if f.skip { len } else { f.offset + used as u64 };
    f.len = len;
    f.mtime = mtime;
    flush(&mut f, cx);
    st.files.insert(key, f);
}

fn process_zst(path: &Path, st: &mut State, cx: &mut Ctx, budget: &mut u64) {
    let Ok(meta) = fs::metadata(path) else { return };
    let (len, mtime) = (meta.len(), mtime_ms(&meta));
    let key = path.to_string_lossy().into_owned();
    if let Some(f) = st.files.get(&key)
        && (f.skip || (f.len == len && f.mtime == mtime))
    {
        return;
    }
    if *budget == 0 {
        return;
    }
    let Ok(file) = File::open(path) else { return };
    let Ok(mut dec) = zstd::stream::read::Decoder::new(file) else {
        return;
    };
    let mut buf = Vec::new();
    let _ = (&mut dec).take(SNIFF).read_to_end(&mut buf);
    let mut f = FileState {
        len,
        mtime,
        offset: len,
        ..FileState::default()
    };
    if sniff_buf(&buf, cx.dirs) {
        st.files.insert(key, f);
        return;
    }
    let _ = (&mut dec)
        .take(MAX_DECOMPRESSED.saturating_sub(buf.len() as u64))
        .read_to_end(&mut buf);
    *budget = budget.saturating_sub(buf.len() as u64);
    f.session = cx.stem_session.clone();
    // A whole file again: the same prompts come out with the same ids.
    for line in buf.split_inclusive(|b| *b == b'\n') {
        let idx = f.line;
        f.line += 1;
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            codex_line(&v, idx, &mut f, cx);
            if f.skip {
                break;
            }
        }
    }
    flush(&mut f, cx);
    st.files.insert(key, f);
}

/// Whether the first working folder in the start of a file is outside the
/// repository.
fn sniff_outside(path: &Path, dirs: &mut Dirs) -> bool {
    let mut buf = Vec::new();
    if let Ok(file) = File::open(path) {
        let _ = file.take(SNIFF).read_to_end(&mut buf);
    }
    sniff_buf(&buf, dirs)
}

fn sniff_buf(buf: &[u8], dirs: &mut Dirs) -> bool {
    for line in buf.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let cwd = v
            .get("cwd")
            .or_else(|| v.pointer("/payload/cwd"))
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty());
        if let Some(cwd) = cwd {
            return dirs.find(cwd).is_none();
        }
    }
    false
}

fn list_dir(dir: &Path) -> Vec<fs::DirEntry> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    entries.sort_by_key(|e| e.file_name());
    entries
}

/// `projects/*/*.jsonl`: top-level files only (subfolders hold sub-agent logs).
fn claude_files(projects: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for project in list_dir(projects) {
        if !project.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for e in list_dir(&project.path()) {
            let path = e.path();
            if e.file_type().is_ok_and(|t| t.is_file())
                && path.extension().is_some_and(|x| x == "jsonl")
            {
                files.push(path);
            }
        }
    }
    files
}

/// `sessions/**/rollout-*.jsonl[.zst]` and `archived_sessions/**`.
fn codex_files(home: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, files: &mut Vec<PathBuf>) {
        for e in list_dir(dir) {
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if depth < 6 {
                    walk(&path, depth + 1, files);
                }
            } else if kind.is_file() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("rollout-")
                    && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
                {
                    files.push(path);
                }
            }
        }
    }
    let mut files = Vec::new();
    walk(&home.join("sessions"), 0, &mut files);
    walk(&home.join("archived_sessions"), 0, &mut files);
    files
}

// ---------------------------------------------------------------- shared line logic

fn str_of(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Milliseconds since the epoch from an ISO 8601 / RFC 3339 time
/// (`2026-10-14T01:02:03.456Z`, `+09:00` offsets accepted).
pub(super) fn parse_iso_ms(s: &str) -> Option<i64> {
    let b = s.trim().as_bytes();
    let num = |a: usize, n: usize| -> Option<i64> {
        let part = b.get(a..a + n)?;
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0i64, |v, d| v * 10 + i64::from(d - b'0')))
    };
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut frac = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let mut scale = 100;
        while let Some(c) = b.get(i).filter(|c| c.is_ascii_digit()) {
            frac += i64::from(c - b'0') * scale;
            scale /= 10;
            i += 1;
        }
    }
    let offset = match b.get(i) {
        Some(b'+' | b'-') => {
            let sign = if b[i] == b'-' { -1 } else { 1 };
            let oh = num(i + 1, 2)?;
            let om = if b.get(i + 3) == Some(&b':') {
                num(i + 4, 2)?
            } else {
                num(i + 3, 2).unwrap_or(0)
            };
            sign * (oh * 60 + om) * 60
        }
        _ => 0,
    };
    // Days from civil date (proleptic Gregorian).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(((days * 24 + h) * 60 + mi) * 60 * 1000 + sec * 1000 + frac - offset * 1000)
}

fn timestamp(v: &Value) -> Option<i64> {
    str_of(v.get("timestamp")).and_then(parse_iso_ms)
}

/// Messages a tool or the app injects into the prompt stream.
fn is_injected(text: &str) -> bool {
    let t = text.trim_start();
    [
        "<task-notification>",
        "<command-",
        "<local-command",
        "<system-reminder>",
        "Caveat:",
        "<environment_context>",
        "<user_instructions>",
    ]
    .iter()
    .any(|p| t.starts_with(p))
}

/// Starts a new current prompt (after returning the previous one).
fn start_prompt(f: &mut FileState, cx: &mut Ctx, id: String, at: i64, raw: &str) {
    if f.cur.as_ref().is_some_and(|c| c.id == id) {
        return;
    }
    flush(f, cx);
    f.cur = None;
    f.since = 0;
    f.last_text = raw.trim().chars().take(300).collect();
    if cx.dirs.find(&f.cwd).is_none() {
        return;
    }
    let text = super::redact::prompt_text(raw);
    if text.trim().is_empty() {
        return;
    }
    f.cur = Some(Cur {
        id,
        session: f.session.clone(),
        at,
        branch: f.branch.clone(),
        text,
        files: Vec::new(),
        commits: Vec::new(),
        pushed: false,
        dirty: true,
    });
}

/// Attaches a changed file (absolute, or relative to the working folder).
fn attach_file(f: &mut FileState, cx: &mut Ctx, path: &str) {
    let Some(dir) = cx.dirs.find(&f.cwd) else {
        return;
    };
    let abs = abs_path(&f.cwd, path);
    if let (Some(cur), Some(r)) = (f.cur.as_mut(), rel(&dir, &abs, cx.dirs.ci)) {
        cur.add_file(r);
    }
}

const COMMIT: u8 = 1;
const PUSH: u8 = 2;

/// What a shell command is checked for in its result.
fn command_flags(cmd: &str) -> u8 {
    let c = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut flags = 0;
    if c.contains("git commit") {
        flags |= COMMIT;
    }
    if c.contains("git push") {
        flags |= PUSH;
    }
    flags
}

fn remember(f: &mut FileState, call_id: &str, flags: u8) {
    if flags == 0 || call_id.is_empty() {
        return;
    }
    while f.pending.len() >= MAX_PENDING {
        let Some(k) = f.pending.keys().next().cloned() else {
            break;
        };
        f.pending.remove(&k);
    }
    f.pending.insert(call_id.to_string(), flags);
}

/// A shell command's result: its commits and push.
fn command_result(f: &mut FileState, call_id: &str, text: &str, is_error: bool) {
    let Some(flags) = f.pending.remove(call_id) else {
        return;
    };
    let Some(cur) = f.cur.as_mut() else { return };
    if flags & COMMIT != 0 {
        for sha in commit_shas(text) {
            cur.add_commit(sha);
        }
    }
    if flags & PUSH != 0 && !looks_failed(text, is_error) {
        cur.set_pushed();
    }
}

/// `[main abc1234] subject` and `[main (root-commit) abc1234] subject` lines
/// of `git commit`: the short shas.
fn commit_shas(text: &str) -> Vec<String> {
    let mut shas = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some(inner) = l
            .strip_prefix('[')
            .and_then(|r| r.find(']').map(|end| (&r[..end], &r[end + 1..])))
            .filter(|(_, after)| after.is_empty() || after.starts_with(' '))
            .map(|(inner, _)| inner)
        else {
            continue;
        };
        let words: Vec<&str> = inner.split_whitespace().collect();
        if let [_, .., last] = words.as_slice()
            && (7..=40).contains(&last.len())
            && last.bytes().all(|b| b.is_ascii_hexdigit())
            && !shas.iter().any(|s| s == last)
        {
            shas.push(last.to_string());
        }
    }
    shas
}

/// Whether a command's output looks like it failed.
fn looks_failed(text: &str, is_error: bool) -> bool {
    if is_error {
        return true;
    }
    let t = text.to_ascii_lowercase();
    if [
        "error:",
        "fatal:",
        "[rejected]",
        "[remote rejected]",
        "failed to push",
    ]
    .iter()
    .any(|m| t.contains(m))
    {
        return true;
    }
    ["exited with code ", "exit code: ", "exit code "]
        .iter()
        .any(|m| {
            t.find(m).is_some_and(|i| {
                let digits: String = t[i + m.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse::<u32>().is_ok_and(|n| n != 0)
            })
        })
}

/// File paths of an `apply_patch` patch.
fn patch_files(text: &str) -> Vec<String> {
    const MARKS: [&str; 4] = [
        "*** Add File: ",
        "*** Update File: ",
        "*** Delete File: ",
        "*** Move to: ",
    ];
    text.lines()
        .filter_map(|line| {
            let l = line.trim();
            MARKS
                .iter()
                .find_map(|m| l.strip_prefix(m))
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
        })
        .collect()
}

/// Text of a tool result: a string, or an array/object holding text parts.
fn result_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => {
            // Older Codex wraps the output as JSON: {"output": "...", "metadata": {...}}
            if s.starts_with('{')
                && let Ok(Value::Object(o)) = serde_json::from_str::<Value>(s)
                && let Some(out) = o.get("output").and_then(Value::as_str)
            {
                return out.to_string();
            }
            s.clone()
        }
        Some(Value::Array(a)) => a
            .iter()
            .map(|p| result_text(p.get("text").or(Some(p))))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::Object(o)) => result_text(o.get("output").or_else(|| o.get("content"))),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------- Claude Code

fn is_human_claude(v: &Value, text: &str) -> bool {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("isMeta")
        || flag("isSidechain")
        || flag("isCompactSummary")
        || flag("isVisibleInTranscriptOnly")
        || text.trim().is_empty()
    {
        return false;
    }
    match v.get("origin") {
        Some(Value::Null) | None => !is_injected(text),
        Some(origin) => origin.get("kind").and_then(Value::as_str) == Some("human"),
    }
}

fn claude_line(v: &Value, _idx: u64, f: &mut FileState, cx: &mut Ctx) {
    if let Some(c) = str_of(v.get("cwd")) {
        f.cwd = c.to_string();
    }
    if let Some(b) = str_of(v.get("gitBranch")) {
        f.branch = Some(b.to_string());
    }
    if let Some(s) = str_of(v.get("sessionId")) {
        f.session = s.to_string();
    }
    match v.get("type").and_then(Value::as_str) {
        Some("user") => match v.pointer("/message/content") {
            Some(Value::String(text)) => {
                if !is_human_claude(v, text) {
                    return;
                }
                let Some(pid) = str_of(v.get("promptId")).or_else(|| str_of(v.get("uuid"))) else {
                    return;
                };
                let id = format!("prompt:claude:{}:{pid}", f.session);
                let at = timestamp(v).unwrap_or(cx.mtime);
                start_prompt(f, cx, id, at, text);
            }
            Some(Value::Array(blocks)) => {
                for b in blocks {
                    if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let Some(id) = str_of(b.get("tool_use_id")) else {
                        continue;
                    };
                    let is_error = b.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                    command_result(f, id, &result_text(b.get("content")), is_error);
                }
            }
            _ => {}
        },
        Some("assistant") => {
            let Some(Value::Array(blocks)) = v.pointer("/message/content") else {
                return;
            };
            for b in blocks {
                if b.get("type").and_then(Value::as_str) != Some("tool_use") || f.cur.is_none() {
                    continue;
                }
                let input = b.get("input");
                match b.get("name").and_then(Value::as_str) {
                    Some("Edit" | "Write" | "MultiEdit" | "NotebookEdit") => {
                        let path = input.and_then(|i| {
                            str_of(i.get("file_path")).or(str_of(i.get("notebook_path")))
                        });
                        if let Some(path) = path {
                            attach_file(f, cx, path);
                        }
                    }
                    Some("Bash") => {
                        if let (Some(id), Some(cmd)) = (
                            str_of(b.get("id")),
                            input.and_then(|i| str_of(i.get("command"))),
                        ) {
                            remember(f, id, command_flags(cmd));
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- Codex

fn codex_line(v: &Value, idx: u64, f: &mut FileState, cx: &mut Ctx) {
    let Some(p) = v.get("payload") else { return };
    match v.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            if let Some(id) = str_of(p.get("id")) {
                f.session = id.to_string();
            }
            if let Some(c) = str_of(p.get("cwd")) {
                f.cwd = c.to_string();
            }
            if let Some(b) = str_of(p.pointer("/git/branch")) {
                f.branch = Some(b.to_string());
            }
            if let Some(source) = p.get("source") {
                let s = source.to_string().to_ascii_lowercase();
                if source.as_str() == Some("exec")
                    || s.contains("subagent")
                    || s.contains("sub_agent")
                {
                    f.skip = true;
                }
            }
        }
        Some("turn_context") => {
            if let Some(c) = str_of(p.get("cwd")) {
                f.cwd = c.to_string();
            }
        }
        Some("event_msg") => match p.get("type").and_then(Value::as_str) {
            Some("user_message") => {
                if let Some(text) = str_of(p.get("message")) {
                    codex_prompt(f, cx, idx, v, text);
                }
            }
            Some("item_completed") => {
                let Some(item) = p.get("item") else { return };
                let kind = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .replace('_', "")
                    .to_ascii_lowercase();
                if kind != "usermessage" {
                    return;
                }
                let text = match item.get("content") {
                    Some(Value::Array(parts)) => parts
                        .iter()
                        .filter_map(|c| str_of(c.get("text")))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => str_of(item.get("message"))
                        .or(str_of(item.get("text")))
                        .unwrap_or_default()
                        .to_string(),
                };
                if !text.trim().is_empty() {
                    codex_prompt(f, cx, idx, v, &text);
                }
            }
            _ => {}
        },
        Some("response_item") => codex_item(p, f, cx),
        _ => {}
    }
}

fn codex_prompt(f: &mut FileState, cx: &mut Ctx, idx: u64, v: &Value, text: &str) {
    if is_injected(text) || text.trim().is_empty() {
        return;
    }
    let head: String = text.trim().chars().take(300).collect();
    // The same message reported twice (user_message and item_completed).
    if f.since == 0 && f.last_text == head && f.cur.is_some() {
        return;
    }
    let id = format!("prompt:codex:{}:{idx}", f.session);
    let at = timestamp(v).unwrap_or(cx.mtime);
    start_prompt(f, cx, id, at, text);
}

fn codex_item(p: &Value, f: &mut FileState, cx: &mut Ctx) {
    let kind = p.get("type").and_then(Value::as_str).unwrap_or_default();
    let call_id = str_of(p.get("call_id"))
        .or(str_of(p.get("id")))
        .unwrap_or_default();
    match kind {
        "function_call_output" | "custom_tool_call_output" | "local_shell_call_output" => {
            let is_error = p.get("success").and_then(Value::as_bool) == Some(false);
            command_result(f, call_id, &result_text(p.get("output")), is_error);
            return;
        }
        "message" => {
            if p.get("role").and_then(Value::as_str) == Some("assistant") {
                f.since += 1;
            }
            return;
        }
        "custom_tool_call" | "function_call" | "local_shell_call" | "reasoning" => f.since += 1,
        _ => return,
    }
    if f.cur.is_none() {
        return;
    }

    let mut patches: Vec<String> = Vec::new();
    let mut cmd = String::new();
    match kind {
        "custom_tool_call" => {
            if let Some(input) = str_of(p.get("input")) {
                patches = patch_files(input);
            }
        }
        "function_call" => {
            let raw = p.get("arguments");
            let args = match raw {
                Some(Value::String(s)) => serde_json::from_str::<Value>(s).ok(),
                Some(o @ Value::Object(_)) => Some(o.clone()),
                _ => None,
            };
            match &args {
                Some(a) => {
                    for key in ["cmd", "command", "input", "patch"] {
                        match a.get(key) {
                            Some(Value::String(s)) => {
                                patches.extend(patch_files(s));
                                if key == "cmd" || key == "command" {
                                    cmd = s.clone();
                                }
                            }
                            Some(Value::Array(parts)) => {
                                let joined = parts
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                patches.extend(patch_files(&joined));
                                cmd = joined;
                            }
                            _ => {}
                        }
                    }
                }
                None => {
                    // Not JSON: look at the text as it is.
                    if let Some(s) = raw.and_then(Value::as_str) {
                        patches.extend(patch_files(&s.replace("\\n", "\n")));
                    }
                }
            }
        }
        "local_shell_call" => {
            cmd = match p.pointer("/action/command") {
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
        }
        _ => {}
    }
    for path in patches {
        attach_file(f, cx, &path);
    }
    remember(f, call_id, command_flags(&cmd));
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn roots(tmp: &Path) -> Roots {
        Roots {
            claude_projects: tmp.join("claude/projects"),
            codex_home: tmp.join("codex"),
        }
    }

    fn append(path: &Path, lines: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        // mtime resolution on some filesystems is coarse; the length changes anyway.
    }

    fn user(cwd: &str, ts: &str, pid: &str, text: &str, extra: Value) -> Value {
        let mut v = json!({
            "type": "user", "uuid": format!("u-{pid}"), "promptId": pid, "sessionId": "S1",
            "timestamp": ts, "cwd": cwd, "gitBranch": "feat",
            "message": {"role": "user", "content": text},
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    }

    fn tool_use(id: &str, name: &str, input: Value) -> Value {
        json!({"type": "assistant", "sessionId": "S1", "timestamp": "2026-10-14T01:00:05.000Z",
            "message": {"role": "assistant", "content": [
                {"type": "text", "text": "ok"},
                {"type": "tool_use", "id": id, "name": name, "input": input}]}})
    }

    fn tool_result(id: &str, text: &str, is_error: bool) -> Value {
        json!({"type": "user", "sessionId": "S1", "timestamp": "2026-10-14T01:00:06.000Z",
            "toolUseResult": {},
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": text, "is_error": is_error}]}})
    }

    #[test]
    fn iso_times_are_parsed() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso_ms("2026-10-14T01:02:03.456Z"),
            Some(1_791_939_723_456)
        );
        assert_eq!(
            parse_iso_ms("2026-10-14T10:02:03.456+09:00"),
            Some(1_791_939_723_456)
        );
        assert_eq!(
            parse_iso_ms("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
        assert_eq!(parse_iso_ms("nope"), None);
        assert_eq!(parse_iso_ms("2026-13-14T01:02:03Z"), None);
    }

    #[test]
    fn paths_are_matched_and_made_relative() {
        assert!(within("/r/a", "/r", false));
        assert!(!within("/rx/a", "/r", false));
        assert!(within("C:/Users/Me/Repo/src", "c:/users/me/repo", true));
        assert!(!within("C:/Users/Me/Repo/src", "c:/users/me/repo", false));
        assert_eq!(norm(r"\\?\C:\Users\me\"), "C:/Users/me");
        assert_eq!(
            rel("C:/Users/Me/Repo", "C:/users/me/repo/Src/A.rs", true).as_deref(),
            Some("Src/A.rs")
        );
        assert_eq!(rel("/r", "/r", false), None);
        assert_eq!(rel("/r", "/other/a", false), None);
        assert_eq!(abs_path("/r/sub", "../a/./b.rs"), "/r/a/b.rs");
        assert_eq!(abs_path("/r", "/x/y"), "/x/y");
    }

    #[test]
    fn commits_and_failures_are_read_from_output() {
        let out =
            "[main abc1234] fix it\n 1 file changed\n[feat/x (root-commit) deadbeef01] first\n";
        assert_eq!(commit_shas(out), ["abc1234", "deadbeef01"]);
        assert!(commit_shas("[INFO] building\n[ main ]").is_empty());
        assert!(looks_failed("error: failed to push some refs", false));
        assert!(looks_failed("To x\n ! [rejected] a -> a", false));
        assert!(looks_failed("Process exited with code 1\nOutput:", false));
        assert!(!looks_failed(
            "Process exited with code 0\nTo github.com:a/b",
            false
        ));
        assert!(!looks_failed("Everything up-to-date", false));
        assert!(looks_failed("whatever", true));
        assert_eq!(
            patch_files(
                "*** Begin Patch\n*** Add File: a/b.rs\n+x\n*** Update File: c.rs\n*** Move to: d.rs\n*** End Patch"
            ),
            ["a/b.rs", "c.rs", "d.rs"]
        );
    }

    #[test]
    fn claude_prompts_are_filtered_and_get_their_work() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let wt = repo.join(".claude/worktrees/x");
        fs::create_dir_all(&wt).unwrap();
        let cwd = repo.to_string_lossy().into_owned();
        let other = tmp.path().join("elsewhere").to_string_lossy().into_owned();
        let r = roots(tmp.path());
        let file = r.claude_projects.join("-proj").join("S1.jsonl");
        let edit_path = repo.join("src/a.rs").to_string_lossy().into_owned();
        let outside = tmp
            .path()
            .join("elsewhere/b.rs")
            .to_string_lossy()
            .into_owned();

        let human = json!({"origin": {"kind": "human"}});
        append(
            &file,
            &[
                json!({"type": "mode", "mode": "x"}),
                user(
                    &cwd,
                    "2026-10-14T01:00:00.000Z",
                    "p1",
                    "fix the bug",
                    human.clone(),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:01.000Z",
                    "p2",
                    "background done",
                    json!({"origin": {"kind": "task-notification"}}),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:02.000Z",
                    "p3",
                    "<task-notification>x</task-notification>",
                    json!({}),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:03.000Z",
                    "p4",
                    "meta",
                    json!({"isMeta": true}),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:04.000Z",
                    "p5",
                    "sub",
                    json!({"isSidechain": true}),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:04.500Z",
                    "p5b",
                    "This session is being continued",
                    json!({"isCompactSummary": true}),
                ),
                tool_use("t1", "Edit", json!({"file_path": edit_path})),
                tool_use("t2", "Write", json!({"file_path": outside})),
                tool_use(
                    "t3",
                    "Bash",
                    json!({"command": "git add -A && git commit -m x"}),
                ),
                tool_result("t3", "[feat abc1234] x\n 1 file changed", false),
                tool_use("t4", "Bash", json!({"command": "git push origin feat"})),
                tool_result("t4", "error: failed to push some refs", false),
                // Not human, in the middle of nothing: ignored.
                json!("garbage line"),
            ],
        );
        // A line that does not parse.
        fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"{not json\n")
            .unwrap();

        let mut state = Value::Null;
        let events = scan(&r, &[repo.clone(), wt], "github.com/o/r", &mut state);
        assert_eq!(events.len(), 1, "{events:?}");
        let e = &events[0];
        assert_eq!(e.id, "prompt:claude:S1:p1");
        assert_eq!(e.tool, ToolId::Claude);
        assert_eq!(e.text, "fix the bug");
        assert_eq!(e.branch.as_deref(), Some("feat"));
        assert_eq!(e.at, parse_iso_ms("2026-10-14T01:00:00Z").unwrap());
        assert_eq!(e.files, ["src/a.rs"]);
        assert_eq!(e.commits, ["abc1234"]);
        assert!(!e.pushed);

        // Nothing new: nothing returned.
        assert!(
            scan(
                &r,
                std::slice::from_ref(&repo),
                "github.com/o/r",
                &mut state
            )
            .is_empty()
        );

        // Appended after a "resume": work attaches to the same prompt, and a
        // successful push is noticed; a prompt outside the repo is not logged.
        append(
            &file,
            &[
                tool_use("t5", "Edit", json!({"file_path": "src/c.rs"})),
                tool_use("t6", "Bash", json!({"command": "git push"})),
                tool_result("t6", "To github.com:o/r.git\n   a..b  feat -> feat", false),
                user(
                    &other,
                    "2026-10-14T02:00:00.000Z",
                    "p6",
                    "elsewhere",
                    human.clone(),
                ),
                tool_use("t7", "Edit", json!({"file_path": edit_path})),
                user(&cwd, "2026-10-14T03:00:00.000Z", "p7", "second", human),
            ],
        );
        let events = scan(
            &r,
            std::slice::from_ref(&repo),
            "github.com/o/r",
            &mut state,
        );
        let ids: Vec<_> = events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["prompt:claude:S1:p1", "prompt:claude:S1:p7"]);
        assert_eq!(events[0].files, ["src/a.rs", "src/c.rs"]);
        assert!(events[0].pushed);
        assert!(events[1].files.is_empty());
    }

    #[test]
    fn claude_sessions_elsewhere_and_subfolders_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let r = roots(tmp.path());
        let human = json!({"origin": {"kind": "human"}});
        let other = tmp.path().join("other").to_string_lossy().into_owned();
        append(
            &r.claude_projects.join("-other/S2.jsonl"),
            &[user(
                &other,
                "2026-10-14T01:00:00.000Z",
                "p1",
                "hi",
                human.clone(),
            )],
        );
        append(
            &r.claude_projects.join("-proj/S1/subagents/agent-1.jsonl"),
            &[user(
                &repo.to_string_lossy(),
                "2026-10-14T01:00:00.000Z",
                "p1",
                "hi",
                human,
            )],
        );
        let mut state = Value::Null;
        assert!(scan(&r, &[repo], "k", &mut state).is_empty());
    }

    #[test]
    fn claude_file_that_shrinks_is_read_again() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let cwd = repo.to_string_lossy().into_owned();
        let r = roots(tmp.path());
        let file = r.claude_projects.join("-p/S1.jsonl");
        let human = json!({"origin": {"kind": "human"}});
        append(
            &file,
            &[
                user(
                    &cwd,
                    "2026-10-14T01:00:00.000Z",
                    "p1",
                    "a long first prompt for padding",
                    human.clone(),
                ),
                user(
                    &cwd,
                    "2026-10-14T01:00:01.000Z",
                    "p2",
                    "another long prompt for padding",
                    human.clone(),
                ),
            ],
        );
        let mut state = Value::Null;
        assert_eq!(
            scan(&r, std::slice::from_ref(&repo), "k", &mut state).len(),
            2
        );
        fs::remove_file(&file).unwrap();
        append(
            &file,
            &[user(&cwd, "2026-10-14T05:00:00.000Z", "p9", "x", human)],
        );
        let events = scan(&r, &[repo], "k", &mut state);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "prompt:claude:S1:p9");
    }

    fn codex_session(cwd: &str, source: Value) -> Vec<Value> {
        vec![
            json!({"timestamp": "2026-10-14T01:00:00.000Z", "type": "session_meta",
                "payload": {"id": "11111111-2222-3333-4444-555555555555", "cwd": cwd,
                    "source": source, "git": {"branch": "feat", "commit_hash": "abc"}}}),
            json!({"timestamp": "2026-10-14T01:00:01.000Z", "type": "turn_context", "payload": {"cwd": cwd}}),
            json!({"timestamp": "2026-10-14T01:00:02.000Z", "type": "response_item",
                "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>x</environment_context>"}]}}),
            json!({"timestamp": "2026-10-14T01:00:03.000Z", "type": "event_msg",
                "payload": {"type": "user_message", "message": "add a logger"}}),
            json!({"timestamp": "2026-10-14T01:00:03.100Z", "type": "event_msg",
                "payload": {"type": "item_completed", "thread_id": "t", "turn_id": "u",
                    "item": {"type": "UserMessage", "id": "i", "content": [{"type": "text", "text": "add a logger"}]}}}),
            json!({"timestamp": "2026-10-14T01:00:04.000Z", "type": "response_item",
                "payload": {"type": "custom_tool_call", "name": "apply_patch", "call_id": "c1",
                    "input": "*** Begin Patch\n*** Add File: src/log.rs\n+x\n*** Update File: src/main.rs\n*** End Patch"}}),
            json!({"timestamp": "2026-10-14T01:00:05.000Z", "type": "response_item",
                "payload": {"type": "function_call", "name": "exec_command", "call_id": "c2",
                    "arguments": "{\"cmd\":\"git commit -am log\"}"}}),
            json!({"timestamp": "2026-10-14T01:00:06.000Z", "type": "response_item",
                "payload": {"type": "function_call_output", "call_id": "c2",
                    "output": "Chunk ID: 1\nProcess exited with code 0\nOutput:\n[feat 9f8e7d6] log\n 2 files changed"}}),
            json!({"timestamp": "2026-10-14T01:00:07.000Z", "type": "response_item",
                "payload": {"type": "local_shell_call", "call_id": "c3", "status": "completed",
                    "action": {"type": "exec", "command": ["bash", "-lc", "git push origin feat"]}}}),
            json!({"timestamp": "2026-10-14T01:00:08.000Z", "type": "response_item",
                "payload": {"type": "function_call_output", "call_id": "c3", "output": "{\"output\":\"To github.com:o/r.git\\n a..b feat -> feat\",\"metadata\":{\"exit_code\":0}}"}}),
        ]
    }

    #[test]
    fn codex_prompts_get_files_commits_and_push() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let cwd = repo.to_string_lossy().into_owned();
        let r = roots(tmp.path());
        let name = "rollout-2026-10-14T10-00-00-11111111-2222-3333-4444-555555555555.jsonl";
        let file = r.codex_home.join("sessions/2026/10/14").join(name);
        append(&file, &codex_session(&cwd, json!("cli")));

        let mut state = Value::Null;
        let events = scan(&r, std::slice::from_ref(&repo), "k", &mut state);
        assert_eq!(events.len(), 1, "{events:?}");
        let e = &events[0];
        assert_eq!(e.tool, ToolId::Codex);
        assert_eq!(e.id, "prompt:codex:11111111-2222-3333-4444-555555555555:3");
        assert_eq!(e.text, "add a logger");
        assert_eq!(e.branch.as_deref(), Some("feat"));
        assert_eq!(e.files, ["src/log.rs", "src/main.rs"]);
        assert_eq!(e.commits, ["9f8e7d6"]);
        assert!(e.pushed);

        // Appended later: a second prompt; a repeated text after work is new.
        append(
            &file,
            &[
                json!({"timestamp": "2026-10-14T02:00:00.000Z", "type": "event_msg",
                "payload": {"type": "user_message", "message": "add a logger"}}),
            ],
        );
        let events = scan(&r, &[repo], "k", &mut state);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].id,
            "prompt:codex:11111111-2222-3333-4444-555555555555:10"
        );
        assert!(events[0].files.is_empty());
    }

    #[test]
    fn codex_subagent_exec_and_other_folders_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let cwd = repo.to_string_lossy().into_owned();
        let r = roots(tmp.path());
        let dir = r.codex_home.join("sessions/2026/10/14");
        append(
            &dir.join("rollout-a-11111111-2222-3333-4444-555555555551.jsonl"),
            &codex_session(&cwd, json!({"subagent": {"thread_spawn": {}}})),
        );
        append(
            &dir.join("rollout-b-11111111-2222-3333-4444-555555555552.jsonl"),
            &codex_session(&cwd, json!("exec")),
        );
        append(
            &dir.join("rollout-c-11111111-2222-3333-4444-555555555553.jsonl"),
            &codex_session(&tmp.path().join("other").to_string_lossy(), json!("cli")),
        );
        let mut state = Value::Null;
        assert!(scan(&r, &[repo], "k", &mut state).is_empty());
    }

    #[test]
    fn codex_compressed_and_archived_sessions_are_read() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let cwd = repo.to_string_lossy().into_owned();
        let r = roots(tmp.path());
        let text: String = codex_session(&cwd, json!("cli"))
            .iter()
            .map(|l| format!("{l}\n"))
            .collect();
        let packed = zstd::stream::encode_all(text.as_bytes(), 3).unwrap();
        let dir = r.codex_home.join("archived_sessions");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("rollout-z-11111111-2222-3333-4444-555555555555.jsonl.zst"),
            packed,
        )
        .unwrap();
        // A zst that is not zstd is ignored.
        fs::write(dir.join("rollout-bad.jsonl.zst"), b"not zstd").unwrap();

        let mut state = Value::Null;
        let events = scan(&r, std::slice::from_ref(&repo), "k", &mut state);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].files, ["src/log.rs", "src/main.rs"]);
        assert_eq!(events[0].commits, ["9f8e7d6"]);
        assert!(events[0].pushed);
        // Unchanged: not read again.
        assert!(scan(&r, &[repo], "k", &mut state).is_empty());
    }
}
