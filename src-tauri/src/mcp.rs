//! MCP servers Claude Code uses in a project, and changing them.
//!
//! - Project servers live in `<project>/.mcp.json`, which the app reads and
//!   writes itself (read-merge-write: fields it doesn't know are kept).
//!   Whether this project uses them is `enabledMcpjsonServers` /
//!   `disabledMcpjsonServers` in `.claude/settings.local.json`.
//! - Local (this project, just me) and user (every project) servers live in
//!   `~/.claude.json`, which also holds sign-in data, so the app never opens
//!   it: it runs the user's own `claude mcp list/get/add/remove` instead.
//! - Connection status comes from `claude mcp list`, which health-checks
//!   each server it would use.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use crate::claude_mod::claude_in;
use crate::library::ops;

type Result<T> = std::result::Result<T, String>;

/// `claude mcp list` connects to every server; a slow one shouldn't hang us.
const LIST_TIMEOUT: Duration = Duration::from_secs(90);
const CHANGE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// `.mcp.json`, shared with the team through git.
    Project,
    /// This project, just me (`~/.claude.json`).
    Local,
    /// Every project (`~/.claude.json`).
    User,
    /// Plugins, claude.ai connectors and anything else: shown, not edited.
    Other,
}

impl Scope {
    fn cli(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Local => "local",
            Scope::User => "user",
            Scope::Other => "",
        }
    }
}

/// The parts of a server entry the app edits. Anything else in a project
/// entry is kept as it was.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// `stdio`, `http`, `sse` or `ws`.
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Connected,
    Failed,
    /// A `.mcp.json` server nobody has approved for this project yet.
    Pending,
    NeedsAuth,
    /// Turned off for this project.
    Off,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub scope: Scope,
    /// `None` for servers only `claude mcp list` knows (plugins, connectors).
    pub config: Option<Config>,
    /// What the list shows as its command or URL.
    pub target: String,
    pub health: Health,
    /// `claude mcp list`'s own words, e.g. the connection error.
    pub status: String,
    /// Project servers: approved (`Some(true)`), turned off (`Some(false)`),
    /// or not decided yet (`None`).
    pub enabled: Option<bool>,
    /// Read back from `claude mcp get`, which joins arguments with spaces:
    /// an argument that contains a space can't be told apart.
    pub args_approx: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub servers: Vec<Server>,
    /// Why personal servers and connection status are missing, if they are.
    pub cli_error: Option<String>,
    /// `.mcp.json` exists but can't be read.
    pub project_error: Option<String>,
}

// ---------------------------------------------------------------------------
// .mcp.json

fn mcp_json(project: &Path) -> PathBuf {
    project.join(".mcp.json")
}

/// The file's JSON object, `{}` when missing, or why it can't be used.
fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let text = match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => {
            return Err(crate::tr!(
                "{}을(를) 읽지 못했어요: {e}",
                "Couldn't read {}: {e}",
                path.display()
            ));
        }
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(crate::tr!(
            "{}이(가) JSON 객체가 아니에요",
            "{} isn't a JSON object",
            path.display()
        )),
        Err(e) => Err(crate::tr!(
            "{}의 JSON 오류: {e}",
            "JSON error in {}: {e}",
            path.display()
        )),
    }
}

fn write_object(path: &Path, map: &Map<String, Value>) -> Result<()> {
    let text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())? + "\n";
    ops::write_atomic(path, text)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let text = v.as_str().map_or_else(|| v.to_string(), str::to_string);
                    (k.clone(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A `.mcp.json` entry as the app edits it.
pub fn config_of(entry: &Value) -> Config {
    let command = entry["command"].as_str().unwrap_or_default().to_string();
    let url = entry["url"].as_str().unwrap_or_default().to_string();
    let transport = entry["type"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            if command.is_empty() && !url.is_empty() {
                "http"
            } else {
                "stdio"
            }
            .into()
        });
    Config {
        transport,
        command,
        args: strings(entry.get("args")),
        env: string_map(entry.get("env")),
        url,
        headers: string_map(entry.get("headers")),
    }
}

/// `entry` with the edited fields from `config`; other fields stay, and the
/// fields of the other transport kind are dropped.
pub fn merge_entry(entry: Option<&Value>, config: &Config) -> Value {
    let mut map = entry
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in ["type", "command", "args", "env", "url", "headers"] {
        map.remove(key);
    }
    let mut out = Map::new();
    out.insert("type".into(), Value::from(config.transport.clone()));
    if config.transport == "stdio" {
        out.insert("command".into(), Value::from(config.command.clone()));
        if !config.args.is_empty() {
            out.insert("args".into(), Value::from(config.args.clone()));
        }
        if !config.env.is_empty() {
            out.insert(
                "env".into(),
                serde_json::to_value(&config.env).unwrap_or_default(),
            );
        }
    } else {
        out.insert("url".into(), Value::from(config.url.clone()));
        if !config.headers.is_empty() {
            out.insert(
                "headers".into(),
                serde_json::to_value(&config.headers).unwrap_or_default(),
            );
        }
    }
    out.extend(map);
    Value::Object(out)
}

/// Names Claude Code accepts (`claude mcp add` says so).
pub fn check_name(name: &str) -> Result<()> {
    if !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(())
    } else {
        Err(crate::tr!(
            "이름은 영문·숫자·-·_만 쓸 수 있어요: {name}",
            "Names can only use letters, digits, - and _: {name}"
        ))
    }
}

pub fn check_config(config: &Config) -> Result<()> {
    match config.transport.as_str() {
        "stdio" if config.command.trim().is_empty() => Err(crate::tr!(
            "실행할 명령을 입력해 주세요",
            "Enter the command to run"
        )),
        "stdio" => Ok(()),
        "http" | "sse" | "ws" => {
            let url = config.url.trim();
            let ok = match config.transport.as_str() {
                "ws" => url.starts_with("ws://") || url.starts_with("wss://"),
                _ => url.starts_with("http://") || url.starts_with("https://"),
            };
            if ok {
                Ok(())
            } else {
                Err(crate::tr!(
                    "주소가 올바르지 않아요: {url}",
                    "The URL doesn't look right: {url}"
                ))
            }
        }
        other => Err(crate::tr!(
            "모르는 연결 방식이에요: {other}",
            "Unknown transport: {other}"
        )),
    }
}

/// Approval of `.mcp.json` servers, from every settings file that can hold it.
#[derive(Default)]
struct Approval {
    all: bool,
    enabled: Vec<String>,
    disabled: Vec<String>,
    /// Turned off somewhere other than settings.local.json.
    disabled_elsewhere: Vec<String>,
}

fn settings_files(project: &Path, config_dir: &Path) -> Vec<(PathBuf, bool)> {
    vec![
        (config_dir.join("settings.json"), false),
        (project.join(".claude").join("settings.json"), false),
        (local_settings(project), true),
    ]
}

fn local_settings(project: &Path) -> PathBuf {
    project.join(".claude").join("settings.local.json")
}

fn approval(project: &Path, config_dir: &Path) -> Approval {
    let mut a = Approval::default();
    for (path, local) in settings_files(project, config_dir) {
        let Ok(map) = read_object(&path) else {
            continue;
        };
        a.all |= map.get("enableAllProjectMcpServers") == Some(&Value::Bool(true));
        a.enabled.extend(strings(map.get("enabledMcpjsonServers")));
        let off = strings(map.get("disabledMcpjsonServers"));
        if !local {
            a.disabled_elsewhere.extend(off.iter().cloned());
        }
        a.disabled.extend(off);
    }
    a
}

impl Approval {
    fn state(&self, name: &str) -> Option<bool> {
        if self.disabled.iter().any(|n| n == name) {
            Some(false)
        } else if self.all || self.enabled.iter().any(|n| n == name) {
            Some(true)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// `claude mcp` output

/// One line of `claude mcp list`: `name: target - status`.
#[derive(Debug, PartialEq)]
pub struct ListLine {
    pub name: String,
    pub target: String,
    pub status: String,
}

pub fn parse_list(text: &str) -> Vec<ListLine> {
    let mut out = Vec::new();
    for line in text.lines() {
        // Diagnostics follow the list; their lines start with a marker or
        // a known heading, never with `name: `.
        if line.starts_with("MCP config diagnostics") {
            break;
        }
        let Some((name, rest)) = line.split_once(": ") else {
            continue;
        };
        let Some((target, status)) = rest.rsplit_once(" - ") else {
            continue;
        };
        if name.is_empty() || name.starts_with(' ') || name.starts_with("Location") {
            continue;
        }
        out.push(ListLine {
            name: name.to_string(),
            target: target.trim().to_string(),
            status: status.trim().to_string(),
        });
    }
    out
}

pub fn health_of(status: &str) -> Health {
    let s = status.to_lowercase();
    if s.contains("pending") {
        Health::Pending
    } else if s.contains("auth") {
        Health::NeedsAuth
    } else if s.contains("rejected") || s.contains("disabled") {
        Health::Off
    } else if s.contains("failed") || s.starts_with('×') || s.starts_with('✗') {
        Health::Failed
    } else if s.contains("connected") || s.starts_with('✓') || s.starts_with('✔') {
        Health::Connected
    } else {
        Health::Unknown
    }
}

/// `claude mcp get <name>`: where it's configured and its settings.
#[derive(Debug, Default, PartialEq)]
pub struct Details {
    pub scope: Option<Scope>,
    pub config: Config,
    pub status: String,
}

pub fn parse_get(text: &str) -> Details {
    let mut d = Details::default();
    let mut section = "";
    for line in text.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if indent >= 4 {
            match section {
                "env" => {
                    if let Some((k, v)) = trimmed.split_once('=') {
                        d.config.env.insert(k.to_string(), v.to_string());
                    }
                }
                "headers" => {
                    if let Some((k, v)) = trimmed.split_once(": ") {
                        d.config.headers.insert(k.to_string(), v.to_string());
                    }
                }
                _ => {}
            }
            continue;
        }
        section = "";
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Scope" => {
                let v = value.to_lowercase();
                d.scope = Some(if v.starts_with("local") {
                    Scope::Local
                } else if v.starts_with("user") {
                    Scope::User
                } else if v.starts_with("project") {
                    Scope::Project
                } else {
                    Scope::Other
                });
            }
            "Status" => d.status = value.to_string(),
            "Type" => d.config.transport = value.to_lowercase(),
            "Command" => d.config.command = value.to_string(),
            "Args" => {
                d.config.args = value.split_whitespace().map(str::to_string).collect();
            }
            "URL" => d.config.url = value.to_string(),
            "Environment" => section = "env",
            "Headers" => section = "headers",
            _ => {}
        }
    }
    d
}

/// `get` leaves out the type and URL of some servers (WebSocket); the list
/// line has them as `url (WS)`.
pub fn fill_from_target(mut config: Config, target: &str) -> Config {
    if config.transport.is_empty()
        && let Some((url, kind)) = target.rsplit_once(" (")
        && let Some(kind) = kind.strip_suffix(')')
    {
        config.transport = kind.to_lowercase();
        if config.url.is_empty() {
            config.url = url.to_string();
        }
    }
    config
}

/// Arguments for `claude mcp add` (or `add-json` for WebSocket servers).
pub fn add_args(name: &str, scope: Scope, config: &Config) -> Vec<String> {
    let mut a: Vec<String> = vec!["mcp".into()];
    if config.transport == "ws" {
        let json = merge_entry(None, config).to_string();
        a.extend([
            "add-json".into(),
            name.into(),
            json,
            "-s".into(),
            scope.cli().into(),
        ]);
        return a;
    }
    a.extend(["add".into(), name.into(), "-s".into(), scope.cli().into()]);
    if config.transport == "stdio" {
        for (k, v) in &config.env {
            a.extend(["-e".into(), format!("{k}={v}")]);
        }
        a.push("--".into());
        a.push(config.command.clone());
        a.extend(config.args.iter().cloned());
    } else {
        a.extend(["-t".into(), config.transport.clone()]);
        for (k, v) in &config.headers {
            a.extend(["-H".into(), format!("{k}: {v}")]);
        }
        a.push("--".into());
        a.push(config.url.clone());
    }
    a
}

fn run(project: &Path, args: &[String], timeout: Duration) -> Result<String> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, stdout, stderr) = claude_in(project, &refs, timeout)?;
    if ok {
        Ok(stdout)
    } else {
        let text = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        Err(text.trim().to_string())
    }
}

// ---------------------------------------------------------------------------
// Listing and changes

fn list(project: &Path, config_dir: &Path) -> Listing {
    let mut listing = Listing::default();
    let approval = approval(project, config_dir);

    let file = match read_object(&mcp_json(project)) {
        Ok(map) => map,
        Err(e) => {
            listing.project_error = Some(e);
            Map::new()
        }
    };
    let empty = Map::new();
    let project_servers = file
        .get("mcpServers")
        .and_then(Value::as_object)
        .unwrap_or(&empty);

    let lines = match run(project, &["mcp".into(), "list".into()], LIST_TIMEOUT) {
        Ok(text) => parse_list(&text),
        Err(e) => {
            listing.cli_error = Some(e);
            Vec::new()
        }
    };

    for (name, entry) in project_servers {
        let config = config_of(entry);
        let line = lines.iter().find(|l| &l.name == name);
        let enabled = approval.state(name);
        let (health, status) = match (enabled, line) {
            (Some(false), _) => (Health::Off, String::new()),
            (_, Some(l)) => (health_of(&l.status), l.status.clone()),
            (None, None) => (Health::Pending, String::new()),
            _ => (Health::Unknown, String::new()),
        };
        let target = if config.transport == "stdio" {
            std::iter::once(config.command.clone())
                .chain(config.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            config.url.clone()
        };
        listing.servers.push(Server {
            name: name.clone(),
            scope: Scope::Project,
            config: Some(config),
            target,
            health,
            status,
            enabled,
            args_approx: false,
        });
    }

    // Everything else is personal (or from a plugin): ask `claude mcp get`
    // where it lives, in parallel, since each one health-checks again.
    let others: Vec<&ListLine> = lines
        .iter()
        .filter(|l| !project_servers.contains_key(&l.name))
        .collect();
    let details: Vec<Option<Details>> = std::thread::scope(|s| {
        let handles: Vec<_> = others
            .iter()
            .map(|l| {
                let name = l.name.clone();
                s.spawn(move || {
                    // Plugin and claude.ai servers have names `add` can't make.
                    check_name(&name).ok()?;
                    run(project, &["mcp".into(), "get".into(), name], LIST_TIMEOUT)
                        .ok()
                        .map(|t| parse_get(&t))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().ok().flatten())
            .collect()
    });
    for (line, detail) in others.into_iter().zip(details) {
        let scope = detail
            .as_ref()
            .and_then(|d| d.scope)
            .filter(|s| *s != Scope::Project)
            .unwrap_or(Scope::Other);
        let config = detail
            .filter(|_| scope != Scope::Other)
            .map(|d| fill_from_target(d.config, &line.target));
        listing.servers.push(Server {
            name: line.name.clone(),
            scope,
            args_approx: config.as_ref().is_some_and(|c| !c.args.is_empty()),
            config,
            target: line.target.clone(),
            health: health_of(&line.status),
            status: line.status.clone(),
            enabled: None,
        });
    }
    listing
}

/// Writes one `.mcp.json` server (renaming `old` if given).
fn save_project(project: &Path, old: Option<&str>, name: &str, config: &Config) -> Result<()> {
    let path = mcp_json(project);
    let mut file = read_object(&path)?;
    let servers = file
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(crate::tr!(
            ".mcp.json의 mcpServers가 객체가 아니에요",
            "mcpServers in .mcp.json isn't an object"
        ));
    };
    if old != Some(name) && servers.contains_key(name) {
        return Err(crate::tr!(
            "같은 이름의 서버가 이미 있어요: {name}",
            "A server with this name already exists: {name}"
        ));
    }
    let previous = old.and_then(|o| servers.get(o)).cloned();
    let entry = merge_entry(previous.as_ref(), config);
    if let Some(o) = old
        && o != name
    {
        servers.remove(o);
    }
    servers.insert(name.to_string(), entry);
    write_object(&path, &file)
}

fn remove_project(project: &Path, name: &str) -> Result<()> {
    let path = mcp_json(project);
    let mut file = read_object(&path)?;
    if let Some(servers) = file.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove(name);
    }
    write_object(&path, &file)
}

/// Approves or turns off a `.mcp.json` server for this project, in
/// settings.local.json (read-merge-write; a new file is git-excluded).
fn set_enabled(project: &Path, config_dir: &Path, name: &str, on: bool) -> Result<()> {
    if on
        && approval(project, config_dir)
            .disabled_elsewhere
            .iter()
            .any(|n| n == name)
    {
        return Err(crate::tr!(
            "'{name}'은(는) 팀 설정이나 내 전체 설정(settings.json)에서 꺼져 있어서 여기서 켤 수 없어요",
            "'{name}' is turned off in a team or user settings.json, so it can't be turned on here"
        ));
    }
    let path = local_settings(project);
    let existed = path.exists();
    let mut map = read_object(&path)?;
    let (add, drop) = if on {
        ("enabledMcpjsonServers", "disabledMcpjsonServers")
    } else {
        ("disabledMcpjsonServers", "enabledMcpjsonServers")
    };
    let mut list = strings(map.get(drop));
    if list.iter().any(|n| n == name) {
        list.retain(|n| n != name);
        if list.is_empty() {
            map.remove(drop);
        } else {
            map.insert(drop.into(), Value::from(list));
        }
    }
    let mut list = strings(map.get(add));
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
    map.insert(add.into(), Value::from(list));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    write_object(&path, &map)?;
    if !existed {
        let _ = ops::exclude_add(&path, false);
    }
    Ok(())
}

/// Adds or changes a server. `old` is where it was before an edit.
fn save(
    project: &Path,
    config_dir: &Path,
    old: Option<(Scope, String, Option<Config>)>,
    scope: Scope,
    name: &str,
    config: &Config,
) -> Result<()> {
    check_name(name)?;
    check_config(config)?;
    if scope == Scope::Other {
        return Err(crate::tr!(
            "이 서버는 여기서 바꿀 수 없어요",
            "This server can't be changed here"
        ));
    }
    let same_place = old
        .as_ref()
        .is_some_and(|(s, n, _)| *s == scope && n == name);
    if scope == Scope::Project {
        let old_name = old
            .as_ref()
            .filter(|(s, ..)| *s == Scope::Project)
            .map(|(_, n, _)| n.as_str());
        save_project(project, old_name, name, config)?;
        // Added (or renamed) here by the user: approve it for this project,
        // rather than leaving Claude Code to ask on the next run.
        if old_name != Some(name) {
            let _ = set_enabled(project, config_dir, name, true);
        }
    } else if same_place {
        // `add` refuses an existing name: take it out, then put it back as
        // it was if the new one is refused.
        run(
            project,
            &[
                "mcp".into(),
                "remove".into(),
                name.into(),
                "-s".into(),
                scope.cli().into(),
            ],
            CHANGE_TIMEOUT,
        )?;
        if let Err(e) = run(project, &add_args(name, scope, config), CHANGE_TIMEOUT) {
            if let Some((_, _, Some(previous))) = &old {
                let _ = run(project, &add_args(name, scope, previous), CHANGE_TIMEOUT);
            }
            return Err(e);
        }
        return Ok(());
    } else {
        run(project, &add_args(name, scope, config), CHANGE_TIMEOUT)?;
    }
    // Moved or renamed: the new one is in place, so the old one can go.
    if let Some((old_scope, old_name, _)) = old
        && !(old_scope == scope && old_name == name)
        && !(old_scope == Scope::Project && scope == Scope::Project)
    {
        remove(project, old_scope, &old_name)?;
    }
    Ok(())
}

fn remove(project: &Path, scope: Scope, name: &str) -> Result<()> {
    match scope {
        Scope::Project => remove_project(project, name),
        Scope::Local | Scope::User => run(
            project,
            &[
                "mcp".into(),
                "remove".into(),
                name.into(),
                "-s".into(),
                scope.cli().into(),
            ],
            CHANGE_TIMEOUT,
        )
        .map(|_| ()),
        Scope::Other => Err(crate::tr!(
            "이 서버는 여기서 지울 수 없어요",
            "This server can't be removed here"
        )),
    }
}

// ---------------------------------------------------------------------------
// Pasted configuration

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parsed {
    pub name: String,
    pub scope: Option<Scope>,
    pub config: Config,
}

/// Servers in pasted text: `.mcp.json`-style JSON (with or without the
/// `mcpServers` wrapper, or a single entry), or `claude mcp add` commands.
pub fn parse_pasted(text: &str) -> Result<Vec<Parsed>> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|e| crate::tr!("JSON을 읽지 못했어요: {e}", "Couldn't read the JSON: {e}"))?;
        let Some(map) = value.as_object() else {
            return Ok(Vec::new());
        };
        let is_entry = |m: &Map<String, Value>| {
            m.contains_key("command") || m.contains_key("url") || m.contains_key("type")
        };
        if is_entry(map) {
            return Ok(vec![Parsed {
                name: String::new(),
                scope: None,
                config: config_of(&value),
            }]);
        }
        let servers = map
            .get("mcpServers")
            .and_then(Value::as_object)
            .unwrap_or(map);
        return Ok(servers
            .iter()
            .filter(|(_, v)| v.as_object().is_some_and(is_entry))
            .map(|(name, v)| Parsed {
                name: name.clone(),
                scope: None,
                config: config_of(v),
            })
            .collect());
    }
    // Commands, joined across `\` / `^` / backtick line continuations.
    let joined = text
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("^\r\n", " ")
        .replace("^\n", " ")
        .replace("`\r\n", " ")
        .replace("`\n", " ");
    let mut out = Vec::new();
    for line in joined.lines() {
        let words = shell_words(line);
        let Some(start) = words
            .windows(2)
            .position(|w| w[0] == "mcp" && (w[1] == "add" || w[1] == "add-json"))
        else {
            continue;
        };
        if let Some(p) = parse_add(&words[start + 1..])? {
            out.push(p);
        }
    }
    Ok(out)
}

/// Splits a command line the way a POSIX shell would (quotes, backslashes).
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') if matches!(chars.peek(), Some('"' | '\\' | '$')) => {
                cur.extend(chars.next());
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                has = true;
            }
            (None, '\\') => {
                cur.extend(chars.next());
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// `add …` or `add-json …` words after `mcp`.
fn parse_add(words: &[String]) -> Result<Option<Parsed>> {
    let json = words.first().is_some_and(|w| w == "add-json");
    let mut config = Config {
        transport: "stdio".into(),
        ..Config::default()
    };
    let mut scope = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1;
    let mut rest = false;
    while i < words.len() {
        let w = &words[i];
        i += 1;
        if rest {
            positional.push(w.clone());
            continue;
        }
        let (flag, inline) = match w.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (w.as_str(), None),
        };
        let mut value = || -> Option<String> {
            inline.clone().or_else(|| {
                let v = words.get(i).cloned();
                i += 1;
                v
            })
        };
        match flag {
            "--" => rest = true,
            "-t" | "--transport" => config.transport = value().unwrap_or_default(),
            "-s" | "--scope" => {
                scope = match value().as_deref() {
                    Some("project") => Some(Scope::Project),
                    Some("user") => Some(Scope::User),
                    _ => Some(Scope::Local),
                }
            }
            "-e" | "--env" => {
                // Variadic: every following KEY=value.
                let mut first = inline.clone();
                loop {
                    let item = match first.take() {
                        Some(v) => v,
                        None => match words.get(i) {
                            Some(v) if v.contains('=') && !v.starts_with('-') => {
                                i += 1;
                                v.clone()
                            }
                            _ => break,
                        },
                    };
                    if let Some((k, v)) = item.split_once('=') {
                        config.env.insert(k.to_string(), v.to_string());
                    }
                }
            }
            "-H" | "--header" => {
                let mut first = inline.clone();
                loop {
                    let item = match first.take() {
                        Some(v) => v,
                        None => match words.get(i) {
                            Some(v)
                                if v.contains(':') && !v.starts_with('-') && !v.contains("://") =>
                            {
                                i += 1;
                                v.clone()
                            }
                            _ => break,
                        },
                    };
                    if let Some((k, v)) = item.split_once(':') {
                        config
                            .headers
                            .insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
            "--client-id" | "--callback-port" => {
                value();
            }
            f if f.starts_with('-') && positional.len() < 2 => {}
            _ => positional.push(w.clone()),
        }
    }
    let mut positional = positional.into_iter();
    let Some(name) = positional.next() else {
        return Ok(None);
    };
    if json {
        let text = positional.next().unwrap_or_default();
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| crate::tr!("JSON을 읽지 못했어요: {e}", "Couldn't read the JSON: {e}"))?;
        return Ok(Some(Parsed {
            name,
            scope,
            config: config_of(&value),
        }));
    }
    let Some(target) = positional.next() else {
        return Ok(None);
    };
    if config.transport == "stdio" {
        config.command = target;
        config.args = positional.collect();
    } else {
        config.url = target;
    }
    Ok(Some(Parsed {
        name,
        scope,
        config,
    }))
}

// ---------------------------------------------------------------------------
// Recommended servers

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommended {
    pub id: &'static str,
    pub name: &'static str,
    pub repo: &'static str,
    pub license: &'static str,
    /// Needs a program installed first (`uv`, Node.js), or a sign-in.
    pub needs: &'static str,
    /// `{project}` stands for the project folder.
    pub config: Config,
}

fn npx(args: &[&str], env: &[(&str, &str)]) -> Config {
    let mut all: Vec<String> = Vec::new();
    // Claude Code's Windows docs launch npx through cmd /c.
    let command = if cfg!(windows) {
        all.extend(["/c".into(), "npx".into()]);
        "cmd"
    } else {
        "npx"
    };
    all.extend(args.iter().map(|s| s.to_string()));
    Config {
        transport: "stdio".into(),
        command: command.into(),
        args: all,
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Config::default()
    }
}

fn uvx(args: &[&str]) -> Config {
    uvx_env(args, &[])
}

fn uvx_env(args: &[&str], env: &[(&str, &str)]) -> Config {
    Config {
        transport: "stdio".into(),
        command: "uvx".into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Config::default()
    }
}

fn http(url: &str, headers: &[(&str, &str)]) -> Config {
    Config {
        transport: "http".into(),
        url: url.into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Config::default()
    }
}

/// Servers checked for license and content (docs/research.md §11). The app
/// only writes a config entry; the user's tools fetch the server itself.
pub fn recommended() -> Vec<Recommended> {
    vec![
        Recommended {
            id: "filesystem",
            name: "filesystem",
            repo: "modelcontextprotocol/servers",
            license: "MIT",
            needs: "node",
            config: npx(
                &["-y", "@modelcontextprotocol/server-filesystem", "{project}"],
                &[],
            ),
        },
        Recommended {
            id: "git",
            name: "git",
            repo: "modelcontextprotocol/servers",
            license: "MIT",
            needs: "uv",
            config: uvx(&["mcp-server-git", "--repository", "{project}"]),
        },
        Recommended {
            id: "fetch",
            name: "fetch",
            repo: "modelcontextprotocol/servers",
            license: "MIT",
            needs: "uv",
            config: uvx(&["mcp-server-fetch"]),
        },
        Recommended {
            id: "memory",
            name: "memory",
            repo: "modelcontextprotocol/servers",
            license: "MIT",
            needs: "node",
            config: npx(&["-y", "@modelcontextprotocol/server-memory"], &[]),
        },
        Recommended {
            id: "time",
            name: "time",
            repo: "modelcontextprotocol/servers",
            license: "MIT",
            needs: "uv",
            config: uvx(&["mcp-server-time", "--local-timezone=Asia/Seoul"]),
        },
        Recommended {
            id: "github",
            name: "github",
            repo: "github/github-mcp-server",
            license: "MIT",
            needs: "token",
            config: http(
                "https://api.githubcopilot.com/mcp/",
                &[("Authorization", "Bearer ${GITHUB_PAT}")],
            ),
        },
        Recommended {
            id: "playwright",
            name: "playwright",
            repo: "microsoft/playwright-mcp",
            license: "Apache-2.0",
            needs: "node",
            config: npx(&["@playwright/mcp@latest", "--isolated"], &[]),
        },
        Recommended {
            id: "chrome-devtools",
            name: "chrome-devtools",
            repo: "ChromeDevTools/chrome-devtools-mcp",
            license: "Apache-2.0",
            needs: "node",
            config: npx(
                &[
                    "-y",
                    "chrome-devtools-mcp@latest",
                    "--no-usage-statistics",
                    "--no-performance-crux",
                ],
                &[("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS", "1")],
            ),
        },
        Recommended {
            id: "context7",
            name: "context7",
            repo: "upstash/context7",
            license: "MIT",
            needs: "",
            config: http("https://mcp.context7.com/mcp", &[]),
        },
        Recommended {
            id: "notion",
            name: "notion",
            repo: "",
            license: "hosted",
            needs: "signin",
            config: http("https://mcp.notion.com/mcp", &[]),
        },
        Recommended {
            id: "linear",
            name: "linear",
            repo: "",
            license: "hosted",
            needs: "signin",
            config: http("https://mcp.linear.app/mcp/readonly", &[]),
        },
        Recommended {
            id: "cloudflare",
            name: "cloudflare",
            repo: "cloudflare/mcp",
            license: "Apache-2.0",
            needs: "signin",
            config: http("https://mcp.cloudflare.com/mcp", &[]),
        },
        Recommended {
            id: "supabase",
            name: "supabase",
            repo: "supabase-community/supabase-mcp",
            license: "Apache-2.0",
            needs: "signin",
            // Read-only until the user removes the flag.
            config: http("https://mcp.supabase.com/mcp?read_only=true", &[]),
        },
        Recommended {
            id: "stripe",
            name: "stripe",
            repo: "stripe/agent-toolkit",
            license: "MIT",
            needs: "signin",
            config: http("https://mcp.stripe.com", &[]),
        },
        Recommended {
            id: "aws-docs",
            name: "aws-docs",
            repo: "awslabs/mcp",
            license: "Apache-2.0",
            needs: "uv",
            config: uvx_env(
                &["awslabs.aws-documentation-mcp-server@latest"],
                &[("FASTMCP_LOG_LEVEL", "ERROR")],
            ),
        },
        Recommended {
            id: "postgres",
            name: "postgres",
            repo: "crystaldba/postgres-mcp",
            license: "MIT",
            needs: "token",
            // Restricted mode: read-only queries with a time limit.
            config: uvx_env(
                &["postgres-mcp", "--access-mode=restricted"],
                &[("DATABASE_URI", "${DATABASE_URI}")],
            ),
        },
        Recommended {
            id: "next-devtools",
            name: "next-devtools",
            repo: "vercel/next-devtools-mcp",
            license: "MIT",
            needs: "node",
            config: npx(
                &["-y", "next-devtools-mcp@latest"],
                &[("NEXT_TELEMETRY_DISABLED", "1")],
            ),
        },
        Recommended {
            id: "atlassian",
            name: "atlassian",
            repo: "sooperset/mcp-atlassian",
            license: "MIT",
            needs: "token",
            config: uvx_env(
                &["mcp-atlassian"],
                &[
                    ("JIRA_URL", "${JIRA_URL}"),
                    ("JIRA_USERNAME", "${JIRA_USERNAME}"),
                    ("JIRA_API_TOKEN", "${JIRA_API_TOKEN}"),
                    ("CONFLUENCE_URL", "${CONFLUENCE_URL}"),
                    ("CONFLUENCE_USERNAME", "${CONFLUENCE_USERNAME}"),
                    ("CONFLUENCE_API_TOKEN", "${CONFLUENCE_API_TOKEN}"),
                ],
            ),
        },
        Recommended {
            id: "figma",
            name: "figma",
            repo: "GLips/Figma-Context-MCP",
            license: "MIT",
            needs: "token",
            config: npx(
                &["-y", "figma-developer-mcp", "--stdio"],
                &[
                    ("FIGMA_API_KEY", "${FIGMA_API_KEY}"),
                    ("FRAMELINK_TELEMETRY", "off"),
                    ("DO_NOT_TRACK", "1"),
                ],
            ),
        },
    ]
}

// ---------------------------------------------------------------------------
// Commands

fn config_dir(app: &AppHandle) -> Result<PathBuf> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(crate::claude_config::Env::detect(home).config_dir)
}

fn project_dir(project: &str) -> Result<PathBuf> {
    let path = PathBuf::from(project);
    if path.is_dir() {
        Ok(path)
    } else {
        Err(crate::tr!(
            "폴더를 찾을 수 없어요: {project}",
            "Folder not found: {project}"
        ))
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mcp_list(app: AppHandle, project: String) -> Result<Listing> {
    let dir = config_dir(&app)?;
    blocking(move || Ok(list(&project_dir(&project)?, &dir))).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Original {
    pub scope: Scope,
    pub name: String,
    pub config: Option<Config>,
}

#[tauri::command]
pub async fn mcp_save(
    app: AppHandle,
    project: String,
    original: Option<Original>,
    scope: Scope,
    name: String,
    config: Config,
) -> Result<()> {
    let config_dir = config_dir(&app)?;
    blocking(move || {
        let dir = project_dir(&project)?;
        let old = original.map(|o| (o.scope, o.name, o.config));
        save(&dir, &config_dir, old, scope, name.trim(), &config)
    })
    .await
}

#[tauri::command]
pub async fn mcp_remove(project: String, scope: Scope, name: String) -> Result<()> {
    blocking(move || remove(&project_dir(&project)?, scope, &name)).await
}

#[tauri::command]
pub async fn mcp_set_enabled(
    app: AppHandle,
    project: String,
    name: String,
    enabled: bool,
) -> Result<()> {
    let dir = config_dir(&app)?;
    blocking(move || set_enabled(&project_dir(&project)?, &dir, &name, enabled)).await
}

#[tauri::command]
pub fn mcp_parse(text: String) -> Result<Vec<Parsed>> {
    parse_pasted(&text)
}

#[tauri::command]
pub fn mcp_recommended() -> Vec<Recommended> {
    recommended()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "Checking MCP server health…

echo-user: node -e process.exit(0) - × Failed to connect — CONNECTION_CLOSED: Connection closed
proj-srv: npx -y some-pkg - ⏸ Pending approval (run `claude` to approve)
remote-local: https://example.invalid/mcp (HTTP) - ✓ Connected
plugin:sentry:sentry: https://mcp.sentry.dev/mcp (HTTP) - ! Needs authentication

MCP config diagnostics ‼

[Contains warnings] Local config (private to you in this project)
Location: /home/x/.claude.json [project: /p]
 └ [Warning] [remote-local] mcpServers.remote-local: Missing environment variables: MY_KEY
";

    #[test]
    fn list_output_is_parsed_until_the_diagnostics() {
        let lines = parse_list(LIST);
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "echo-user",
                "proj-srv",
                "remote-local",
                "plugin:sentry:sentry"
            ]
        );
        assert_eq!(lines[2].target, "https://example.invalid/mcp (HTTP)");
        let health: Vec<Health> = lines.iter().map(|l| health_of(&l.status)).collect();
        assert_eq!(
            health,
            [
                Health::Failed,
                Health::Pending,
                Health::Connected,
                Health::NeedsAuth
            ]
        );
    }

    #[test]
    fn get_output_gives_scope_and_settings() {
        let d = parse_get(
            "echo-user:
  Scope: User config (available in all your projects)
  Status: × Failed to connect
  Issue: CONNECTION_CLOSED: Connection closed
  Type: stdio
  Command: node
  Args: -e process.exit(0)
  Environment:
    FOO=bar
    URL=a=b

To remove this server, run: claude mcp remove echo-user -s user
",
        );
        assert_eq!(d.scope, Some(Scope::User));
        assert_eq!(d.config.transport, "stdio");
        assert_eq!(d.config.command, "node");
        assert_eq!(d.config.args, ["-e", "process.exit(0)"]);
        assert_eq!(d.config.env["URL"], "a=b");
        let d = parse_get(
            "r:
  Scope: Local config (private to you in this project)
  Type: http
  URL: https://x/mcp
  Headers:
    Authorization: Bearer ${T}
",
        );
        assert_eq!(d.scope, Some(Scope::Local));
        assert_eq!(d.config.url, "https://x/mcp");
        assert_eq!(d.config.headers["Authorization"], "Bearer ${T}");
    }

    #[test]
    fn missing_type_comes_from_the_list_line() {
        let c = fill_from_target(Config::default(), "wss://x (WS)");
        assert_eq!((c.transport.as_str(), c.url.as_str()), ("ws", "wss://x"));
        let stdio = Config {
            transport: "stdio".into(),
            ..Config::default()
        };
        assert_eq!(fill_from_target(stdio.clone(), "node a (b)"), stdio);
    }

    #[test]
    fn add_arguments_put_the_name_before_variadic_flags() {
        let mut c = Config {
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            ..Config::default()
        };
        c.env.insert("A".into(), "1".into());
        assert_eq!(
            add_args("s", Scope::User, &c),
            [
                "mcp", "add", "s", "-s", "user", "-e", "A=1", "--", "npx", "-y", "pkg"
            ]
        );
        let mut h = Config {
            transport: "http".into(),
            url: "https://x/mcp".into(),
            ..Config::default()
        };
        h.headers
            .insert("Authorization".into(), "Bearer ${T}".into());
        assert_eq!(
            add_args("h", Scope::Local, &h),
            [
                "mcp",
                "add",
                "h",
                "-s",
                "local",
                "-t",
                "http",
                "-H",
                "Authorization: Bearer ${T}",
                "--",
                "https://x/mcp"
            ]
        );
    }

    #[test]
    fn project_entries_keep_unknown_fields_and_switch_transport() {
        let entry =
            serde_json::json!({"command": "npx", "args": ["a"], "env": {"K": "v"}, "timeout": 5});
        let c = config_of(&entry);
        assert_eq!(c.transport, "stdio");
        let http = Config {
            transport: "http".into(),
            url: "https://x".into(),
            ..Config::default()
        };
        assert_eq!(
            merge_entry(Some(&entry), &http),
            serde_json::json!({"type": "http", "url": "https://x", "timeout": 5})
        );
        assert_eq!(
            config_of(&serde_json::json!({"url": "https://y"})).transport,
            "http"
        );
    }

    #[test]
    fn project_servers_are_saved_renamed_toggled_and_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        let home = tmp.path().join("home");
        fs::write(
            p.join(".mcp.json"),
            r#"{"other": 1, "mcpServers": {"a": {"command": "x", "timeout": 3}}}"#,
        )
        .unwrap();
        let c = Config {
            transport: "stdio".into(),
            command: "node".into(),
            args: vec!["s.js".into()],
            ..Config::default()
        };
        save_project(p, Some("a"), "b", &c).unwrap();
        let v: Value =
            serde_json::from_str(&fs::read_to_string(p.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(v["other"], 1);
        assert!(v["mcpServers"].get("a").is_none());
        assert_eq!(v["mcpServers"]["b"]["timeout"], 3);
        assert_eq!(v["mcpServers"]["b"]["command"], "node");
        assert!(save_project(p, None, "b", &c).is_err(), "duplicate name");

        set_enabled(p, &home, "b", false).unwrap();
        assert_eq!(approval(p, &home).state("b"), Some(false));
        set_enabled(p, &home, "b", true).unwrap();
        let local: Value =
            serde_json::from_str(&fs::read_to_string(local_settings(p)).unwrap()).unwrap();
        assert_eq!(local, serde_json::json!({"enabledMcpjsonServers": ["b"]}));

        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join("settings.json"),
            r#"{"disabledMcpjsonServers": ["b"]}"#,
        )
        .unwrap();
        assert!(set_enabled(p, &home, "b", true).is_err());

        remove_project(p, "b").unwrap();
        let v: Value =
            serde_json::from_str(&fs::read_to_string(p.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(v["mcpServers"], serde_json::json!({}));

        fs::write(p.join(".mcp.json"), "{ broken").unwrap();
        assert!(save_project(p, None, "c", &c).is_err());
    }

    #[test]
    fn pasted_json_and_commands_become_servers() {
        let p = parse_pasted(r#"{"mcpServers": {"fs": {"command": "npx", "args": ["-y", "x"]}}}"#)
            .unwrap();
        assert_eq!(p[0].name, "fs");
        assert_eq!(p[0].config.args, ["-y", "x"]);
        let p = parse_pasted(r#"{"type": "http", "url": "https://x"}"#).unwrap();
        assert_eq!(p[0].name, "");
        assert_eq!(p[0].config.transport, "http");

        let p = parse_pasted(
            "claude mcp add --transport http github https://api.githubcopilot.com/mcp -H \"Authorization: Bearer ${PAT}\"\n\
             claude mcp add my-server -e API_KEY=xxx -e B=2 -s user -- npx my-mcp-server --flag\n\
             claude mcp add-json w '{\"type\":\"stdio\",\"command\":\"node\"}'\n\
             claude mcp add filesystem \\\n  -- npx -y @modelcontextprotocol/server-filesystem '/a b'",
        )
        .unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].name, "github");
        assert_eq!(p[0].config.url, "https://api.githubcopilot.com/mcp");
        assert_eq!(p[0].config.headers["Authorization"], "Bearer ${PAT}");
        assert_eq!(p[1].scope, Some(Scope::User));
        assert_eq!(p[1].config.env.len(), 2);
        assert_eq!(p[1].config.command, "npx");
        assert_eq!(p[1].config.args, ["my-mcp-server", "--flag"]);
        assert_eq!(p[2].config.command, "node");
        assert_eq!(p[3].config.args.last().unwrap(), "/a b");
    }

    #[test]
    fn names_and_configs_are_checked() {
        assert!(check_name("my_server-1").is_ok());
        assert!(check_name("bad name").is_err());
        assert!(check_name("").is_err());
        let mut c = Config {
            transport: "http".into(),
            url: "ftp://x".into(),
            ..Config::default()
        };
        assert!(check_config(&c).is_err());
        c.url = "https://x".into();
        assert!(check_config(&c).is_ok());
        c.transport = "stdio".into();
        assert!(check_config(&c).is_err());
    }

    /// Runs the real `claude`: `AIM_MCP_LIVE=<project dir> cargo test -- --ignored live`
    /// with HOME pointed at a scratch folder.
    #[test]
    #[ignore]
    fn live_cli_round_trip() {
        let project = PathBuf::from(std::env::var("AIM_MCP_LIVE").unwrap());
        let home = PathBuf::from(std::env::var("HOME").unwrap()).join(".claude");
        let mut c = Config {
            transport: "stdio".into(),
            command: "node".into(),
            args: vec!["-e".into(), "1".into()],
            ..Config::default()
        };
        c.env.insert("TOKEN".into(), "${MY_TOKEN}".into());
        save(&project, &home, None, Scope::User, "live-u", &c).unwrap();
        let h = Config {
            transport: "http".into(),
            url: "https://example.invalid/mcp".into(),
            ..Config::default()
        };
        save(&project, &home, None, Scope::Local, "live-l", &h).unwrap();
        save(&project, &home, None, Scope::Project, "live-p", &c).unwrap();
        let l = list(&project, &home);
        let find = |n: &str| l.servers.iter().find(|s| s.name == n).unwrap();
        assert_eq!(find("live-u").scope, Scope::User);
        assert_eq!(
            find("live-u").config.as_ref().unwrap().env["TOKEN"],
            "${MY_TOKEN}"
        );
        assert_eq!(find("live-l").scope, Scope::Local);
        assert_eq!(find("live-p").scope, Scope::Project);
        // Edit in place, then move user -> local under a new name.
        let mut c2 = c.clone();
        c2.args = vec!["-e".into(), "2".into()];
        save(
            &project,
            &home,
            Some((Scope::User, "live-u".into(), Some(c.clone()))),
            Scope::User,
            "live-u",
            &c2,
        )
        .unwrap();
        save(
            &project,
            &home,
            Some((Scope::User, "live-u".into(), Some(c2.clone()))),
            Scope::Local,
            "live-m",
            &c2,
        )
        .unwrap();
        let l = list(&project, &home);
        assert!(
            l.servers.iter().all(|s| s.name != "live-u"),
            "{:?}",
            l.servers
        );
        let m = l.servers.iter().find(|s| s.name == "live-m").unwrap();
        assert_eq!(m.scope, Scope::Local);
        assert_eq!(m.config.as_ref().unwrap().args, ["-e", "2"]);
        for (s, n) in [
            (Scope::Local, "live-m"),
            (Scope::Local, "live-l"),
            (Scope::Project, "live-p"),
        ] {
            remove(&project, s, n).unwrap();
        }
        assert!(
            list(&project, &home)
                .servers
                .iter()
                .all(|s| !s.name.starts_with("live-"))
        );
    }

    #[test]
    fn recommended_servers_are_valid() {
        for r in recommended() {
            check_name(r.name).unwrap();
            check_config(&r.config).unwrap();
        }
    }
}
