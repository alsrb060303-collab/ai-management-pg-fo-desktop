//! Codex MCP servers: `[mcp_servers.<name>]` tables in the user
//! `$CODEX_HOME/config.toml` (scope "user") and the project
//! `.codex/config.toml` (scope "project", read by Codex only when the user
//! trusted the project). Facts follow docs/research.md section 13.
//!
//! `codex mcp add/remove` only write the user file and `codex mcp list`
//! doesn't check connections, so the app reads and edits both files itself:
//! read-merge-write with toml_edit, keeping comments, other keys and the
//! keys of a server entry the app doesn't edit (`cwd`, timeouts, tool
//! filters…). `auth.json` is never read.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::{Config, Health, Listing, Result, Scope, Server, check_config, check_name, target_of};
use crate::library::ops;
use crate::tools::codex::util::{get, get_str, get_strings, norm};

/// Whether Codex reads the project's `.codex/config.toml`
/// (`[projects."<path>"] trust_level` in the user config).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Trust {
    Trusted,
    Untrusted,
    /// Not decided yet: Codex asks the first time it opens the project.
    Unknown,
}

/// Keys only a command (stdio) server has.
const STDIO_KEYS: &[&str] = &["command", "args", "env", "env_vars", "cwd"];
/// Keys only a URL (streamable HTTP) server has.
const HTTP_KEYS: &[&str] = &[
    "url",
    "http_headers",
    "env_http_headers",
    "bearer_token_env_var",
];

pub fn user_file(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

pub fn project_file(project: &Path) -> PathBuf {
    project.join(".codex").join("config.toml")
}

fn file_for(project: &Path, codex_home: &Path, scope: Scope) -> Result<PathBuf> {
    match scope {
        Scope::User => Ok(user_file(codex_home)),
        Scope::Project => Ok(project_file(project)),
        Scope::Local | Scope::Other => Err(crate::tr!(
            "Codex에는 이 범위가 없어요",
            "Codex has no such scope"
        )),
    }
}

// ---------------------------------------------------------------------------
// Files

/// The file's document, empty when missing, or why it can't be used.
fn read_doc(path: &Path) -> Result<DocumentMut> {
    let text = match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DocumentMut::new()),
        Err(e) => {
            return Err(crate::tr!(
                "{}을(를) 읽지 못했어요: {e}",
                "Couldn't read {}: {e}",
                path.display()
            ));
        }
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    text.parse::<DocumentMut>().map_err(|e| {
        let e = e.to_string();
        let first = e.lines().next().unwrap_or_default().trim().to_string();
        crate::tr!(
            "{}의 TOML 오류: {first}",
            "TOML error in {}: {first}",
            path.display()
        )
    })
}

fn write_doc(path: &Path, doc: &DocumentMut) -> Result<()> {
    ops::write_atomic(path, doc.to_string())
}

/// `[mcp_servers]` of `doc`, if there is one.
fn servers(doc: &DocumentMut) -> Option<&dyn TableLike> {
    doc.get("mcp_servers").and_then(Item::as_table_like)
}

/// `[mcp_servers]` of `doc`, created (as a header-less table) when missing.
fn servers_mut(doc: &mut DocumentMut) -> Result<&mut Item> {
    if doc.get("mcp_servers").is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        doc.insert("mcp_servers", Item::Table(table));
    }
    let item = doc.get_mut("mcp_servers").expect("inserted above");
    if item.is_table_like() {
        Ok(item)
    } else {
        Err(crate::tr!(
            "config.toml의 mcp_servers가 표(table)가 아니에요",
            "mcp_servers in config.toml isn't a table"
        ))
    }
}

/// `entry` in the shape its new parent holds: a `[table]` under a table, an
/// inline table under an inline table.
fn entry_for(parent: &Item, entry: Item) -> Item {
    let inline_parent = parent.as_inline_table().is_some();
    match entry {
        Item::Table(t) if inline_parent => Item::Value(Value::InlineTable(t.into_inline_table())),
        Item::Value(Value::InlineTable(t)) if !inline_parent => Item::Table(t.into_table()),
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Trust

/// Trust of `project` (or of its root, the nearest folder above holding a
/// `project_root_markers` entry, `.git` by default), the way the config map
/// reads it. Codex writes the keys resolved, lowercased on Windows.
pub fn trust(user: &DocumentMut, project: &Path) -> Trust {
    let root_item = user.as_item();
    let Some(projects) = get(root_item, &["projects"]).and_then(Item::as_table_like) else {
        return Trust::Unknown;
    };
    let markers =
        get_strings(root_item, &["project_root_markers"]).unwrap_or_else(|| vec![".git".into()]);
    let root = if markers.is_empty() {
        None
    } else {
        project
            .ancestors()
            .find(|dir| markers.iter().any(|m| dir.join(m).exists()))
    };
    let entries: Vec<(PathBuf, Option<&str>)> = projects
        .iter()
        .map(|(key, value)| (norm(Path::new(key)), get_str(value, &["trust_level"])))
        .collect();
    let lookup = |dir: &Path| {
        let dir = norm(dir);
        entries
            .iter()
            .find(|(key, _)| *key == dir)
            .and_then(|(_, level)| *level)
    };
    match lookup(project).or_else(|| root.and_then(lookup)) {
        Some("trusted") => Trust::Trusted,
        Some("untrusted") => Trust::Untrusted,
        _ => Trust::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Entry <-> Config

/// A string table's entries (non-string values as written).
fn string_map(item: Option<&Item>) -> BTreeMap<String, String> {
    item.and_then(Item::as_table_like)
        .map(|t| {
            t.iter()
                .map(|(k, v)| {
                    let text = v
                        .as_str()
                        .map_or_else(|| v.to_string().trim().to_string(), str::to_string);
                    (k.to_string(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `env_vars`: names, or `{ name = … }` tables.
fn env_var_names(item: Option<&Item>) -> Vec<String> {
    let Some(array) = item.and_then(Item::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(s.value().clone()),
            Value::InlineTable(t) => t.get("name").and_then(Value::as_str).map(str::to_string),
            _ => None,
        })
        .collect()
}

fn reference(name: &str) -> String {
    format!("${{{name}}}")
}

/// `NAME` of a whole-value `${NAME}`.
fn referenced(value: &str) -> Option<&str> {
    let name = value.strip_prefix("${")?.strip_suffix('}')?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then_some(name)
}

/// A Codex server entry as the app edits it. Variables Codex passes on
/// (`env_vars`, `env_http_headers`, `bearer_token_env_var`) become
/// `${NAME}` values, as in `.mcp.json`; [`apply`] turns them back.
pub fn config_of(entry: &dyn TableLike) -> Config {
    let text = |key: &str| {
        entry
            .get(key)
            .and_then(Item::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let command = text("command");
    let url = text("url");
    let transport = if command.is_empty() && !url.is_empty() {
        "http"
    } else {
        "stdio"
    };
    let mut env = string_map(entry.get("env"));
    for name in env_var_names(entry.get("env_vars")) {
        env.entry(name.clone()).or_insert_with(|| reference(&name));
    }
    let mut headers = string_map(entry.get("http_headers"));
    for (header, var) in string_map(entry.get("env_http_headers")) {
        headers.entry(header).or_insert_with(|| reference(&var));
    }
    let bearer = text("bearer_token_env_var");
    if !bearer.is_empty()
        && !headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("authorization"))
    {
        headers.insert(
            "Authorization".into(),
            format!("Bearer {}", reference(&bearer)),
        );
    }
    Config {
        transport: transport.into(),
        command,
        args: entry
            .get("args")
            .and_then(Item::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        env,
        url,
        headers,
    }
}

/// Sets `key` to `value`, keeping the comments around an existing value.
fn set_value(entry: &mut dyn TableLike, key: &str, value: Value) {
    match entry.get_mut(key) {
        Some(Item::Value(old)) => {
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
        }
        _ => {
            entry.insert(key, Item::Value(value));
        }
    }
}

fn set_str(entry: &mut dyn TableLike, key: &str, value: &str) {
    if value.is_empty() {
        entry.remove(key);
    } else if entry.get(key).and_then(Item::as_str) != Some(value) {
        set_value(entry, key, Value::from(value));
    }
}

fn set_strings(entry: &mut dyn TableLike, key: &str, values: &[String]) {
    let current: Option<Vec<&str>> = entry
        .get(key)
        .and_then(Item::as_array)
        .map(|a| a.iter().map(|v| v.as_str().unwrap_or("\u{0}")).collect());
    if values.is_empty() {
        entry.remove(key);
    } else if current.as_deref() != Some(&values.iter().map(String::as_str).collect::<Vec<_>>()) {
        set_value(entry, key, Value::Array(values.iter().collect::<Array>()));
    }
}

/// A string table: an existing one is edited key by key (its form and
/// comments stay), a new one is written inline.
fn set_map(entry: &mut dyn TableLike, key: &str, map: &BTreeMap<String, String>) {
    if map.is_empty() {
        entry.remove(key);
        return;
    }
    if string_map(entry.get(key)) == *map {
        return;
    }
    if let Some(table) = entry.get_mut(key).and_then(Item::as_table_like_mut) {
        let gone: Vec<String> = table
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| !map.contains_key(k))
            .collect();
        for k in gone {
            table.remove(&k);
        }
        for (k, v) in map {
            set_str(table, k, v);
        }
        return;
    }
    let mut table = InlineTable::new();
    for (k, v) in map {
        table.insert(k, Value::from(v.as_str()));
    }
    set_value(entry, key, Value::InlineTable(table));
}

/// Writes `config` into `entry`. Keys the app doesn't edit stay; those of
/// the other transport are dropped.
pub fn apply(entry: &mut dyn TableLike, config: &Config) {
    let stdio = config.transport == "stdio";
    for key in if stdio { HTTP_KEYS } else { STDIO_KEYS } {
        entry.remove(key);
    }
    if stdio {
        set_str(entry, "command", &config.command);
        set_strings(entry, "args", &config.args);
        // `KEY = "${KEY}"` passes the variable on: `env_vars`.
        let mut plain = BTreeMap::new();
        let mut passed = Vec::new();
        for (k, v) in &config.env {
            if referenced(v) == Some(k.as_str()) {
                passed.push(k.clone());
            } else {
                plain.insert(k.clone(), v.clone());
            }
        }
        set_map(entry, "env", &plain);
        let current = env_var_names(entry.get("env_vars"));
        let mut sorted_current = current.clone();
        sorted_current.sort();
        if sorted_current != passed {
            set_strings(entry, "env_vars", &passed);
        }
    } else {
        set_str(entry, "url", &config.url);
        let mut plain = BTreeMap::new();
        let mut from_env = BTreeMap::new();
        let mut bearer = String::new();
        for (k, v) in &config.headers {
            let bearer_var = k
                .eq_ignore_ascii_case("authorization")
                .then(|| v.strip_prefix("Bearer ").and_then(referenced))
                .flatten();
            if let Some(var) = bearer_var {
                bearer = var.to_string();
            } else if let Some(var) = referenced(v) {
                from_env.insert(k.clone(), var.to_string());
            } else {
                plain.insert(k.clone(), v.clone());
            }
        }
        set_str(entry, "bearer_token_env_var", &bearer);
        set_map(entry, "http_headers", &plain);
        set_map(entry, "env_http_headers", &from_env);
    }
}

fn is_enabled(entry: &dyn TableLike) -> bool {
    entry.get("enabled").and_then(Item::as_bool).unwrap_or(true)
}

// ---------------------------------------------------------------------------
// Listing and changes

fn push_servers(doc: &DocumentMut, scope: Scope, out: &mut Vec<Server>) {
    let Some(servers) = servers(doc) else {
        return;
    };
    for (name, item) in servers.iter() {
        let Some(entry) = item.as_table_like() else {
            continue;
        };
        let config = config_of(entry);
        let enabled = is_enabled(entry);
        out.push(Server {
            name: name.to_string(),
            scope,
            target: target_of(&config),
            config: Some(config),
            health: if enabled {
                Health::NoCheck
            } else {
                Health::Off
            },
            status: String::new(),
            enabled: Some(enabled),
            args_approx: false,
        });
    }
}

pub fn list(project: &Path, codex_home: &Path) -> Listing {
    let mut listing = Listing::default();
    let user = read_doc(&user_file(codex_home)).unwrap_or_else(|e| {
        listing.user_error = Some(e);
        DocumentMut::new()
    });
    listing.trust = Some(trust(&user, project));
    match read_doc(&project_file(project)) {
        Ok(doc) => push_servers(&doc, Scope::Project, &mut listing.servers),
        Err(e) => listing.project_error = Some(e),
    }
    push_servers(&user, Scope::User, &mut listing.servers);
    listing
}

fn check_codex_config(config: &Config) -> Result<()> {
    if !matches!(config.transport.as_str(), "stdio" | "http") {
        return Err(crate::tr!(
            "Codex는 명령 실행(stdio)과 HTTP 서버만 지원해요",
            "Codex only supports command (stdio) and HTTP servers"
        ));
    }
    check_config(config)
}

fn exists_error(name: &str) -> String {
    crate::tr!(
        "같은 이름의 서버가 이미 있어요: {name}",
        "A server with this name already exists: {name}"
    )
}

/// Adds or changes a server. `old` is where it was before an edit; a moved
/// or renamed entry keeps the keys the app doesn't edit.
pub fn save(
    project: &Path,
    codex_home: &Path,
    old: Option<(Scope, String)>,
    scope: Scope,
    name: &str,
    config: &Config,
) -> Result<()> {
    check_name(name)?;
    check_codex_config(config)?;
    let path = file_for(project, codex_home, scope)?;
    let old = old.map(|(s, n)| file_for(project, codex_home, s).map(|p| (p, s, n)));
    let old = old.transpose()?;
    let same_file = old.as_ref().is_some_and(|(_, s, _)| *s == scope);

    let mut doc = read_doc(&path)?;
    // Edited where it is: changed in place, so it keeps its position.
    if same_file && old.as_ref().is_some_and(|(_, _, n)| n == name) {
        let entry = servers_mut(&mut doc)?
            .get_mut(name)
            .and_then(Item::as_table_like_mut);
        if let Some(entry) = entry {
            apply(entry, config);
            return write_doc(&path, &doc);
        }
    }
    // The entry being edited, taken out of wherever it was.
    let mut previous: Option<Item> = None;
    let mut old_doc: Option<(PathBuf, DocumentMut)> = None;
    if let Some((old_path, _, old_name)) = &old {
        if same_file {
            previous = servers_mut(&mut doc)?
                .as_table_like_mut()
                .and_then(|t| t.remove(old_name));
        } else {
            let mut other = read_doc(old_path)?;
            previous = servers_mut(&mut other)?
                .as_table_like_mut()
                .and_then(|t| t.remove(old_name));
            old_doc = Some((old_path.clone(), other));
        }
    }
    let servers = servers_mut(&mut doc)?;
    if servers
        .as_table_like()
        .is_some_and(|t| t.contains_key(name))
    {
        return Err(exists_error(name));
    }
    let mut entry = entry_for(
        servers,
        previous
            .filter(Item::is_table_like)
            .unwrap_or_else(|| Item::Table(Table::new())),
    );
    if let Some(table) = entry.as_table_like_mut() {
        apply(table, config);
    }
    if let Some(table) = servers.as_table_like_mut() {
        table.insert(name, entry);
    }
    write_doc(&path, &doc)?;
    // Moved: the new one is in place, so the old one can go.
    if let Some((old_path, other)) = old_doc {
        write_doc(&old_path, &other)?;
    }
    Ok(())
}

pub fn remove(project: &Path, codex_home: &Path, scope: Scope, name: &str) -> Result<()> {
    let path = file_for(project, codex_home, scope)?;
    let mut doc = read_doc(&path)?;
    let removed = doc
        .get_mut("mcp_servers")
        .and_then(Item::as_table_like_mut)
        .and_then(|t| t.remove(name));
    if removed.is_some() {
        write_doc(&path, &doc)?;
    }
    Ok(())
}

/// Turns a server on or off (`enabled = false`; on drops the key, or sets
/// an existing one to true).
pub fn set_enabled(
    project: &Path,
    codex_home: &Path,
    scope: Scope,
    name: &str,
    on: bool,
) -> Result<()> {
    let path = file_for(project, codex_home, scope)?;
    let mut doc = read_doc(&path)?;
    let Some(entry) = doc
        .get_mut("mcp_servers")
        .and_then(|s| s.get_mut(name))
        .and_then(Item::as_table_like_mut)
    else {
        return Err(crate::tr!(
            "서버를 찾을 수 없어요: {name}",
            "Server not found: {name}"
        ));
    };
    if on {
        if entry.contains_key("enabled") {
            set_value(entry, "enabled", Value::from(true));
        }
    } else {
        set_value(entry, "enabled", Value::from(false));
    }
    write_doc(&path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("aim-mcp-codex-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            Self { dir }
        }
        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }
        fn project(&self) -> PathBuf {
            self.dir.join("proj")
        }
        fn user_text(&self) -> String {
            fs::read_to_string(user_file(&self.home())).unwrap()
        }
        fn project_text(&self) -> String {
            fs::read_to_string(project_file(&self.project())).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn stdio(command: &str, args: &[&str], env: &[(&str, &str)]) -> Config {
        Config {
            transport: "stdio".into(),
            command: command.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Config::default()
        }
    }

    const USER: &str = r#"# my settings
model = "gpt-5"

[mcp_servers.docs]
# the docs server
command = "npx" # runs it
args = ["-y", "docs-mcp"]
cwd = "/tmp"
startup_timeout_sec = 20
enabled_tools = ["search"]

[mcp_servers.docs.env]
LEVEL = "debug" # keep this comment

[mcp_servers.remote]
url = "https://example.com/mcp"
bearer_token_env_var = "REMOTE_TOKEN"
http_headers = { "X-Team" = "core" }
env_http_headers = { "X-Key" = "REMOTE_KEY" }
enabled = false
"#;

    #[test]
    fn entries_map_to_the_app_model() {
        let doc: DocumentMut = USER.parse().unwrap();
        let servers = servers(&doc).unwrap();
        let docs = config_of(servers.get("docs").unwrap().as_table_like().unwrap());
        assert_eq!(docs.transport, "stdio");
        assert_eq!(docs.args, ["-y", "docs-mcp"]);
        assert_eq!(docs.env["LEVEL"], "debug");
        let remote_item = servers.get("remote").unwrap().as_table_like().unwrap();
        let remote = config_of(remote_item);
        assert_eq!(remote.transport, "http");
        assert_eq!(remote.headers["Authorization"], "Bearer ${REMOTE_TOKEN}");
        assert_eq!(remote.headers["X-Key"], "${REMOTE_KEY}");
        assert_eq!(remote.headers["X-Team"], "core");
        assert!(!is_enabled(remote_item));

        // And back: variables become Codex's own keys.
        let mut table = Table::new();
        apply(&mut table, &remote);
        let text = table.to_string();
        assert!(
            text.contains("bearer_token_env_var = \"REMOTE_TOKEN\""),
            "{text}"
        );
        assert!(text.contains("X-Key = \"REMOTE_KEY\""), "{text}");
        assert!(!text.contains("Authorization"), "{text}");

        let mut table = Table::new();
        apply(
            &mut table,
            &stdio(
                "uvx",
                &["x"],
                &[("TOKEN", "${TOKEN}"), ("MODE", "${OTHER}")],
            ),
        );
        let back = config_of(&table);
        assert_eq!(back.env["TOKEN"], "${TOKEN}");
        let text = table.to_string();
        assert!(text.contains("env_vars = [\"TOKEN\"]"), "{text}");
        assert!(text.contains("MODE = \"${OTHER}\""), "{text}");
    }

    #[test]
    fn listing_reads_both_files() {
        let f = Fixture::new("list");
        fs::write(user_file(&f.home()), USER).unwrap();
        fs::create_dir_all(f.project().join(".codex")).unwrap();
        fs::write(
            project_file(&f.project()),
            "[mcp_servers.local-db]\ncommand = \"db-mcp\"\n",
        )
        .unwrap();
        let listing = list(&f.project(), &f.home());
        let found: Vec<_> = listing
            .servers
            .iter()
            .map(|s| (s.name.as_str(), s.scope, s.health.clone(), s.enabled))
            .collect();
        assert_eq!(
            found,
            [
                ("local-db", Scope::Project, Health::NoCheck, Some(true)),
                ("docs", Scope::User, Health::NoCheck, Some(true)),
                ("remote", Scope::User, Health::Off, Some(false)),
            ]
        );
        assert_eq!(listing.trust, Some(Trust::Unknown));

        fs::write(project_file(&f.project()), "[mcp_servers\n").unwrap();
        let listing = list(&f.project(), &f.home());
        assert!(listing.project_error.is_some());
        assert_eq!(listing.servers.len(), 2);
    }

    #[test]
    fn edits_keep_comments_and_unknown_keys() {
        let f = Fixture::new("edit");
        fs::write(user_file(&f.home()), USER).unwrap();
        let (p, h) = (f.project(), f.home());

        // Change the args only: everything else stays as written.
        save(
            &p,
            &h,
            Some((Scope::User, "docs".into())),
            Scope::User,
            "docs",
            &stdio("npx", &["-y", "docs-mcp@2"], &[("LEVEL", "debug")]),
        )
        .unwrap();
        let text = f.user_text();
        assert!(
            text.starts_with("# my settings\nmodel = \"gpt-5\"\n"),
            "{text}"
        );
        assert!(
            text.contains("# the docs server\ncommand = \"npx\" # runs it\n"),
            "{text}"
        );
        assert!(text.contains("args = [\"-y\", \"docs-mcp@2\"]"), "{text}");
        assert!(text.contains("cwd = \"/tmp\""), "{text}");
        assert!(text.contains("startup_timeout_sec = 20"), "{text}");
        assert!(text.contains("enabled_tools = [\"search\"]"), "{text}");
        assert!(
            text.contains("LEVEL = \"debug\" # keep this comment"),
            "{text}"
        );
        assert!(text.contains("enabled = false"), "{text}");

        // A rename keeps the unknown keys; a taken name is refused.
        save(
            &p,
            &h,
            Some((Scope::User, "docs".into())),
            Scope::User,
            "remote",
            &stdio("npx", &[], &[]),
        )
        .unwrap_err();
        save(
            &p,
            &h,
            Some((Scope::User, "docs".into())),
            Scope::User,
            "docs2",
            &stdio("npx", &["-y", "docs-mcp@2"], &[]),
        )
        .unwrap();
        let doc: DocumentMut = f.user_text().parse().unwrap();
        let entry = servers(&doc).unwrap().get("docs2").unwrap();
        assert_eq!(get_str(entry, &["cwd"]), Some("/tmp"));
        assert!(get(entry, &["env"]).is_none());
        assert!(servers(&doc).unwrap().get("docs").is_none());

        // Switching to HTTP drops the command keys only.
        save(
            &p,
            &h,
            Some((Scope::User, "docs2".into())),
            Scope::User,
            "docs2",
            &Config {
                transport: "http".into(),
                url: "https://docs.example/mcp".into(),
                ..Config::default()
            },
        )
        .unwrap();
        let doc: DocumentMut = f.user_text().parse().unwrap();
        let entry = servers(&doc).unwrap().get("docs2").unwrap();
        assert!(get(entry, &["command"]).is_none() && get(entry, &["cwd"]).is_none());
        assert_eq!(get_str(entry, &["url"]), Some("https://docs.example/mcp"));
        assert!(get(entry, &["startup_timeout_sec"]).is_some());

        // SSE and WebSocket aren't Codex transports.
        let sse = Config {
            transport: "sse".into(),
            url: "https://x".into(),
            ..Config::default()
        };
        assert!(save(&p, &h, None, Scope::User, "sse", &sse).is_err());
        assert!(save(&p, &h, None, Scope::Local, "x", &stdio("a", &[], &[])).is_err());
    }

    #[test]
    fn servers_move_between_files_and_toggle() {
        let f = Fixture::new("move");
        fs::write(user_file(&f.home()), USER).unwrap();
        let (p, h) = (f.project(), f.home());

        // A new project file is created with a plain `[mcp_servers.x]` table.
        save(
            &p,
            &h,
            None,
            Scope::Project,
            "fs",
            &stdio("npx", &["fs"], &[]),
        )
        .unwrap();
        assert_eq!(
            f.project_text(),
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"fs\"]\n"
        );

        // Moving to the user file takes the entry's other keys along.
        save(
            &p,
            &h,
            Some((Scope::User, "docs".into())),
            Scope::Project,
            "docs",
            &stdio("npx", &["-y", "docs-mcp"], &[("LEVEL", "debug")]),
        )
        .unwrap();
        assert!(!f.user_text().contains("[mcp_servers.docs]"));
        let doc: DocumentMut = f.project_text().parse().unwrap();
        let entry = servers(&doc).unwrap().get("docs").unwrap();
        assert_eq!(get_str(entry, &["cwd"]), Some("/tmp"));

        // Off writes `enabled = false`; on drops it back to the default…
        set_enabled(&p, &h, Scope::Project, "fs", false).unwrap();
        assert!(f.project_text().contains("enabled = false"));
        let listing = list(&p, &h);
        let fs_server = listing.servers.iter().find(|s| s.name == "fs").unwrap();
        assert_eq!(fs_server.health, Health::Off);
        set_enabled(&p, &h, Scope::Project, "fs", true).unwrap();
        // …once it was there, it stays and says true.
        assert!(f.project_text().contains("enabled = true"));
        set_enabled(&p, &h, Scope::User, "remote", true).unwrap();
        assert!(f.user_text().contains("enabled = true"));
        assert!(set_enabled(&p, &h, Scope::User, "missing", true).is_err());

        remove(&p, &h, Scope::Project, "fs").unwrap();
        assert!(!f.project_text().contains("mcp_servers.fs"));
        remove(&p, &h, Scope::User, "remote").unwrap();
        assert!(f.user_text().starts_with("# my settings\n"));
        assert!(!f.user_text().contains("remote"));
    }

    #[test]
    fn project_trust_is_read_from_the_user_config() {
        let f = Fixture::new("trust");
        let project = f.project();
        fs::create_dir_all(project.join(".git")).unwrap();
        let sub = project.join("pkg");
        fs::create_dir_all(&sub).unwrap();
        let key = norm(&project).to_string_lossy().into_owned();
        let user = |level: &str| -> DocumentMut {
            let mut doc = DocumentMut::new();
            let mut table = Table::new();
            table.insert("trust_level", toml_edit::value(level));
            let mut projects = Table::new();
            projects.set_implicit(true);
            projects.insert(&key, Item::Table(table));
            doc.insert("projects", Item::Table(projects));
            doc
        };
        assert_eq!(trust(&DocumentMut::new(), &project), Trust::Unknown);
        assert_eq!(trust(&user("trusted"), &project), Trust::Trusted);
        assert_eq!(trust(&user("untrusted"), &project), Trust::Untrusted);
        // A subfolder of a trusted repository is trusted through its root.
        assert_eq!(trust(&user("trusted"), &sub), Trust::Trusted);
        // Written by hand as text, too.
        let text = format!(
            "[projects.{}]\ntrust_level = \"trusted\"\n",
            toml_edit::Key::new(key.clone())
        );
        let doc: DocumentMut = text.parse().unwrap();
        assert_eq!(trust(&doc, &project), Trust::Trusted);
    }
}
