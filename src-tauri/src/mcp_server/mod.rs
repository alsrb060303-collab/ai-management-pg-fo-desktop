//! An MCP server Claude Code can start: `<app> mcp [--data-dir <dir>]`, over
//! stdio (newline-delimited JSON-RPC). It runs without the app's windows: it
//! reads what the app keeps (`bridge/state.json`, composer documents, the
//! block catalog) and leaves changes in the bridge inbox for the running app
//! to apply, so the app stays the only writer of its data (`bridge.rs`).
//!
//! Only the parts of MCP a tools-only server needs: `initialize`, `ping`,
//! `tools/list` and `tools/call`.

mod tools;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{Value, json};

/// The app's identifier (`tauri.conf.json`), which names its data folder.
const IDENTIFIER: &str = "io.github.alsrb060303collab.aimanagement";

/// Protocol versions this server answers to; others get the first one.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Encela keeps per-branch notes and to-dos, the branch's PR/CI status, \
and a catalog of instruction, rule, skill, subagent and hook blocks. Call get_branch_context at \
the start of work on a branch, keep its to-dos current (add_todo, set_todo_done) and leave notes \
for later sessions (append_note). To set up a project's CLAUDE.md, rules, skills or subagents, \
pick blocks with search_blocks/get_block and build a document with create_document; the user \
reviews it in the app before writing it to the project.";

/// Runs the server until stdin closes; the process exit code.
pub fn run(args: &[String]) -> i32 {
    let Some(data_dir) = data_dir(args) else {
        eprintln!("encela mcp: couldn't find the app's data folder; pass --data-dir <dir>");
        return 1;
    };
    let server = tools::Server::new(data_dir);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        for reply in handle_line(&server, &line) {
            if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                return 0;
            }
        }
    }
    0
}

/// `--data-dir <dir>`, or where Tauri puts the app's data on this OS.
fn data_dir(args: &[String]) -> Option<PathBuf> {
    if let Some(i) = args.iter().position(|a| a == "--data-dir") {
        return args.get(i + 1).map(PathBuf::from);
    }
    default_data_dir().map(|d| d.join(IDENTIFIER))
}

#[cfg(windows)]
fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

/// One line from the client: a message, or (2025-03-26) a batch of them.
fn handle_line(server: &tools::Server, line: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(line) {
        Err(_) => vec![error(Value::Null, -32700, "Parse error")],
        Ok(Value::Array(batch)) => batch.iter().filter_map(|m| handle(server, m)).collect(),
        Ok(message) => handle(server, &message).into_iter().collect(),
    }
}

/// The reply to a request; notifications and responses get none.
fn handle(server: &tools::Server, message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let Some(method) = message["method"].as_str() else {
        return Some(error(id, -32600, "Invalid request"));
    };
    let params = &message["params"];
    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            server.call(name, &params["arguments"])
        }
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, text)) => error(id, code, &text),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|v| **v == asked)
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "encela", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (tools::Server, PathBuf) {
        let dir = std::env::temp_dir().join(format!("encela-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (tools::Server::new(dir.clone()), dir)
    }

    #[test]
    fn the_identifier_matches_tauri_conf() {
        let conf: Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], IDENTIFIER);
    }

    #[test]
    fn initialize_agrees_on_a_version_and_offers_tools() {
        let (server, dir) = server();
        let reply = handle_line(
            &server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
        );
        assert_eq!(reply[0]["result"]["protocolVersion"], "2025-03-26");
        assert!(reply[0]["result"]["capabilities"]["tools"].is_object());

        let reply = handle_line(
            &server,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        );
        assert_eq!(reply[0]["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn notifications_get_no_reply_and_unknown_methods_an_error() {
        let (server, dir) = server();
        assert!(
            handle_line(
                &server,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .is_empty()
        );
        let reply = handle_line(&server, r#"{"jsonrpc":"2.0","id":"a","method":"nope"}"#);
        assert_eq!(reply[0]["id"], "a");
        assert_eq!(reply[0]["error"]["code"], -32601);
        let reply = handle_line(&server, "{not json");
        assert_eq!(reply[0]["error"]["code"], -32700);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn every_listed_tool_has_a_schema() {
        let (server, dir) = server();
        let reply = handle_line(&server, r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#);
        let tools = reply[0]["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 9);
        for tool in tools {
            assert!(tool["name"].is_string() && tool["description"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
