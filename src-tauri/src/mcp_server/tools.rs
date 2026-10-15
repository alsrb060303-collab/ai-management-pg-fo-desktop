//! The MCP server's tools: branch notes, to-dos and PR/CI status from the
//! bridge, and composer documents built from the block catalog.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::bridge::{self, Request};
use crate::composer::catalog::{self, Block, BlockKind};
use crate::composer::compile;
use crate::tools::ToolId;

/// `state.json` is rewritten every 30 seconds while the app runs.
const RUNNING_WITHIN_MS: i64 = 90_000;
const SEARCH_LIMIT: usize = 60;

type ToolResult = Result<Value, String>;

pub struct Server {
    data_dir: PathBuf,
}

/// A tool's input schema: an object with these properties, `required` ones first.
fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn tool(name: &str, description: &str, input: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input,
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false },
    })
}

pub fn definitions() -> Vec<Value> {
    let path = json!({ "type": "string", "description": "Project folder (default: the current directory)" });
    let branch =
        json!({ "type": "string", "description": "Branch (default: the checked-out one)" });
    let tool_id = json!({ "type": "string", "enum": ["claude", "codex"], "description": "Lay files out for this tool (default claude)" });
    vec![
        tool(
            "get_branch_context",
            "The branch's note, to-dos (with ids) and PR/CI status as kept in the Encela app, plus other branches with open to-dos.",
            schema(json!({ "path": path, "branch": branch }), &[]),
            true,
        ),
        tool(
            "add_todo",
            "Adds a to-do to the branch in Encela.",
            schema(
                json!({ "text": { "type": "string" }, "path": path, "branch": branch }),
                &["text"],
            ),
            false,
        ),
        tool(
            "set_todo_done",
            "Marks a to-do done or not done (id from get_branch_context).",
            schema(
                json!({ "id": { "type": "string" }, "done": { "type": "boolean" } }),
                &["id", "done"],
            ),
            false,
        ),
        tool(
            "append_note",
            "Adds lines to the end of the branch's note in Encela (decisions, progress, what is left).",
            schema(
                json!({ "text": { "type": "string" }, "path": path, "branch": branch }),
                &["text"],
            ),
            false,
        ),
        tool(
            "search_blocks",
            "Searches Encela's block catalog (instructions, rules, skills, subagents, hooks, and the user's library). Returns ids for get_block and create_document.",
            schema(
                json!({
                    "query": { "type": "string", "description": "Words to match in the title, summary or category; empty lists all" },
                    "kind": { "type": "string", "enum": ["instruction", "rule", "skill", "agent", "hook"] },
                }),
                &[],
            ),
            true,
        ),
        tool(
            "get_block",
            "A catalog block in full: what it does, its text and where it comes from.",
            schema(json!({ "id": { "type": "string" } }), &["id"]),
            true,
        ),
        tool(
            "list_documents",
            "The composer documents saved in Encela.",
            schema(json!({}), &[]),
            true,
        ),
        tool(
            "preview_document",
            "The files a composer document writes (CLAUDE.md section, rules, skills, subagents, hooks, PLANS.md), with their content and warnings.",
            schema(
                json!({ "id": { "type": "string" }, "tool": tool_id }),
                &["id"],
            ),
            true,
        ),
        tool(
            "create_document",
            "Creates a composer document in Encela from catalog blocks: each block is connected to the output its kind goes to, and an optional main goal becomes PLANS.md. Returns the files it would write. The user reviews and writes it from the app.",
            schema(
                json!({
                    "name": { "type": "string" },
                    "blockIds": { "type": "array", "items": { "type": "string" } },
                    "goal": {
                        "type": "object",
                        "description": "Main goal for PLANS.md, e.g. from the branch's to-dos",
                        "properties": {
                            "title": { "type": "string" },
                            "note": { "type": "string" },
                            "subGoals": { "type": "array", "items": { "type": "string" } },
                        },
                        "required": ["title"],
                    },
                    "tool": tool_id,
                }),
                &["name", "blockIds"],
            ),
            false,
        ),
    ]
}

fn text_result(value: &Value) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn error_result(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn short_id(prefix: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}{}", &id[..12])
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    str_arg(args, key).ok_or_else(|| format!("`{key}` is required"))
}

fn tool_arg(args: &Value) -> Result<ToolId, String> {
    match str_arg(args, "tool") {
        None => Ok(ToolId::Claude),
        Some(t) => serde_json::from_value(json!(t)).map_err(|_| format!("Unknown tool: {t}")),
    }
}

/// The repository and branch memos are filed under, as the app files them.
struct Target {
    repo_key: String,
    repo_name: String,
    branch: String,
}

impl Server {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn bridge(&self) -> PathBuf {
        bridge::dir_in(&self.data_dir)
    }

    /// What the app last exported, if it ever ran.
    fn state(&self) -> Option<Value> {
        let bytes = std::fs::read(self.bridge().join("state.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn app_running(state: Option<&Value>) -> bool {
        state
            .and_then(|s| s["heartbeat"].as_i64())
            .is_some_and(|t| now_ms() - t < RUNNING_WITHIN_MS)
    }

    fn catalog(&self) -> Vec<Block> {
        let library = crate::library::items_at(&self.data_dir);
        catalog::all(&library, Some(&self.data_dir.join("catalog-cache")))
    }

    fn docs_dir(&self) -> PathBuf {
        crate::composer::docs_dir(&self.data_dir)
    }

    /// Answers `tools/call`: a tool result, or a JSON-RPC error for an unknown tool.
    pub fn call(&self, name: &str, args: &Value) -> Result<Value, (i64, String)> {
        let result = match name {
            "get_branch_context" => self.branch_context(args),
            "add_todo" => self.add_todo(args),
            "set_todo_done" => self.set_todo_done(args),
            "append_note" => self.append_note(args),
            "search_blocks" => self.search_blocks(args),
            "get_block" => self.get_block(args),
            "list_documents" => self.list_documents(),
            "preview_document" => self.preview_document(args),
            "create_document" => self.create_document(args),
            _ => return Err((-32602, format!("Unknown tool: {name}"))),
        };
        Ok(match result {
            Ok(value) => text_result(&value),
            Err(message) => error_result(&message),
        })
    }

    fn target(&self, args: &Value, state: Option<&Value>) -> Result<Target, String> {
        let dir = match str_arg(args, "path") {
            Some(p) => PathBuf::from(p),
            None => std::env::current_dir().map_err(|e| e.to_string())?,
        };
        let info = crate::memo::repo::inspect(&dir)?;
        let mut repo_key = info.key.clone();
        // A repository that got its remote after memos were written under its path.
        if let Some(state) = state
            && state["repos"].get(&repo_key).is_none()
            && let Some(path_key) = &info.path_key
            && let Some(key) = state["paths"][path_key].as_str()
        {
            repo_key = key.to_string();
        }
        let branch = match str_arg(args, "branch") {
            Some(b) => b.to_string(),
            None if !info.is_git => String::new(),
            None => info.branch.clone().ok_or_else(|| {
                "No branch is checked out (detached HEAD); pass `branch`.".to_string()
            })?,
        };
        Ok(Target {
            repo_key,
            repo_name: info.name,
            branch,
        })
    }

    fn branch_context(&self, args: &Value) -> ToolResult {
        let state = self.state();
        let target = self.target(args, state.as_ref())?;
        let branches = state
            .as_ref()
            .and_then(|s| s["repos"].get(&target.repo_key))
            .and_then(Value::as_object);
        let here = branches.and_then(|b| b.get(&target.branch));
        let others: Vec<Value> = branches
            .into_iter()
            .flatten()
            .filter(|(name, _)| **name != target.branch)
            .filter_map(|(name, data)| {
                let open = data["todos"]
                    .as_array()?
                    .iter()
                    .filter(|t| t["done"] != true)
                    .count();
                (open > 0).then(|| json!({ "branch": name, "openTodos": open }))
            })
            .collect();
        Ok(json!({
            "appRunning": Self::app_running(state.as_ref()),
            "repo": { "key": target.repo_key, "name": target.repo_name },
            "branch": target.branch,
            "note": here.map(|h| h["note"].clone()).unwrap_or(json!("")),
            "todos": here.map(|h| h["todos"].clone()).unwrap_or(json!([])),
            "github": here.map(|h| h["github"].clone()).unwrap_or(Value::Null),
            "otherBranchesWithOpenTodos": others,
            "appNeverRan": state.is_none(),
        }))
    }

    /// Leaves `request` for the app; says whether it applies now or on next start.
    fn send(&self, request: Request, done: Value) -> ToolResult {
        bridge::enqueue(&self.bridge(), &request).map_err(|e| e.to_string())?;
        let running = Self::app_running(self.state().as_ref());
        let mut done = done;
        done["applied"] = json!(if running {
            "Encela applies it within a few seconds"
        } else {
            "Encela isn't running; it applies this the next time it starts"
        });
        Ok(done)
    }

    fn add_todo(&self, args: &Value) -> ToolResult {
        let text = required(args, "text")?.to_string();
        let target = self.target(args, self.state().as_ref())?;
        let done = json!({ "added": text, "branch": target.branch });
        self.send(
            Request::AddTodo {
                repo_key: target.repo_key,
                branch: target.branch,
                text,
            },
            done,
        )
    }

    fn set_todo_done(&self, args: &Value) -> ToolResult {
        let id = required(args, "id")?.to_string();
        let done = args["done"]
            .as_bool()
            .ok_or("`done` must be true or false")?;
        self.send(
            Request::SetDone {
                id: id.clone(),
                done,
            },
            json!({ "id": id, "done": done }),
        )
    }

    fn append_note(&self, args: &Value) -> ToolResult {
        let text = required(args, "text")?.to_string();
        let target = self.target(args, self.state().as_ref())?;
        let done = json!({ "appended": text, "branch": target.branch });
        self.send(
            Request::AppendNote {
                repo_key: target.repo_key,
                branch: target.branch,
                text,
            },
            done,
        )
    }

    fn search_blocks(&self, args: &Value) -> ToolResult {
        let kind: Option<BlockKind> = match str_arg(args, "kind") {
            None => None,
            Some(k) => {
                Some(serde_json::from_value(json!(k)).map_err(|_| format!("Unknown kind: {k}"))?)
            }
        };
        let catalog = self.catalog();
        let found = search(&catalog, str_arg(args, "query").unwrap_or_default(), kind);
        let total = found.len();
        let blocks: Vec<Value> = found
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|b| {
                let mut v = json!({
                    "id": b.id,
                    "kind": b.kind,
                    "title": b.title,
                    "summary": b.summary,
                    "category": b.category,
                });
                if let Some(en) = &b.en {
                    v["titleEn"] = json!(en.title);
                }
                v
            })
            .collect();
        Ok(json!({ "total": total, "blocks": blocks }))
    }

    fn get_block(&self, args: &Value) -> ToolResult {
        let id = required(args, "id")?;
        let catalog = self.catalog();
        let block = catalog
            .iter()
            .find(|b| b.id == id)
            .ok_or_else(|| format!("No block with id {id}; use search_blocks"))?;
        serde_json::to_value(block).map_err(|e| e.to_string())
    }

    fn list_documents(&self) -> ToolResult {
        let docs = crate::composer::list_docs(&self.docs_dir());
        serde_json::to_value(docs).map_err(|e| e.to_string())
    }

    fn preview_document(&self, args: &Value) -> ToolResult {
        let id = required(args, "id")?;
        if !crate::composer::valid_doc_id(id) {
            return Err(format!("Invalid document id: {id}"));
        }
        let bytes = std::fs::read(self.docs_dir().join(format!("{id}.json")))
            .map_err(|_| format!("No document with id {id}; use list_documents"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let doc = crate::composer::parse_doc(value)?;
        let compiled = compile::compile(&doc, &self.catalog(), tool_arg(args)?);
        serde_json::to_value(compiled).map_err(|e| e.to_string())
    }

    fn create_document(&self, args: &Value) -> ToolResult {
        let name = required(args, "name")?;
        let tool = tool_arg(args)?;
        let ids: Vec<&str> = args["blockIds"]
            .as_array()
            .ok_or("`blockIds` must be a list of block ids")?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let catalog = self.catalog();
        let mut picked: Vec<&Block> = Vec::new();
        let mut unknown = Vec::new();
        for id in ids {
            match catalog.iter().find(|b| b.id == id) {
                Some(b) if !picked.iter().any(|p| p.id == b.id) => picked.push(b),
                Some(_) => {}
                None => unknown.push(id),
            }
        }
        if !unknown.is_empty() {
            return Err(format!(
                "Unknown block ids: {}; use search_blocks",
                unknown.join(", ")
            ));
        }
        let goal = Goal::from_arg(&args["goal"]);
        if picked.is_empty() && goal.is_none() {
            return Err("Give at least one block id or a goal".into());
        }
        let doc = build_doc(name, &picked, goal.as_ref(), tool);
        let id = doc["id"].as_str().unwrap_or_default().to_string();
        let compiled = compile::compile(&crate::composer::parse_doc(doc.clone())?, &catalog, tool);
        let files: Vec<Value> = compiled
            .files
            .iter()
            .map(|f| json!({ "path": f.path, "lines": f.lines }))
            .collect();
        self.send(
            Request::SaveDoc { doc },
            json!({ "id": id, "name": name, "files": files, "warnings": compiled.warnings }),
        )
    }
}

/// Blocks whose id, titles, summary or category hold every word of `query`.
fn search<'a>(catalog: &'a [Block], query: &str, kind: Option<BlockKind>) -> Vec<&'a Block> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    catalog
        .iter()
        .filter(|b| kind.is_none_or(|k| b.kind == k))
        .filter(|b| {
            let mut hay = format!("{} {} {} {}", b.id, b.title, b.summary, b.category);
            if let Some(en) = &b.en {
                hay.push_str(&format!(" {} {} {}", en.title, en.summary, en.category));
            }
            let hay = hay.to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect()
}

pub struct Goal {
    title: String,
    note: Option<String>,
    sub_goals: Vec<String>,
}

impl Goal {
    fn from_arg(arg: &Value) -> Option<Self> {
        let title = arg["title"].as_str()?.trim();
        if title.is_empty() {
            return None;
        }
        Some(Self {
            title: title.to_string(),
            note: arg["note"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            sub_goals: arg["subGoals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        })
    }
}

/// The output each kind of block goes to (rules join the instructions in
/// tools without rule files, as the composer allows).
fn target_of(kind: BlockKind, tool: ToolId) -> &'static str {
    match kind {
        BlockKind::Instruction => "instructions",
        BlockKind::Rule if tool.has_rules() => "rules",
        BlockKind::Rule => "instructions",
        BlockKind::Skill => "skills",
        BlockKind::Agent => "agents",
        BlockKind::Hook => "hooks",
    }
}

const TARGET_ORDER: &[&str] = &["instructions", "rules", "skills", "agents", "hooks"];

fn kind_name(kind: BlockKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn edge(source: &str, target: &str, kind: &str) -> Value {
    json!({
        "id": format!("xy-edge__{source}-{target}"),
        "source": source,
        "target": target,
        "style": { "stroke": format!("var(--c-kind-{kind})"), "strokeWidth": 2 },
    })
}

/// A composer document as the app saves one: blocks in a column, each kind's
/// output to their right, and the goal with its PLANS.md output on top.
pub fn build_doc(name: &str, blocks: &[&Block], goal: Option<&Goal>, tool: ToolId) -> Value {
    const BLOCK_X: f64 = 0.0;
    const OUTPUT_X: f64 = 340.0;
    const ROW: f64 = 130.0;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut y = 0.0;

    if let Some(goal) = goal {
        let plan = short_id("p");
        let output = short_id("o");
        let items: Vec<Value> = goal
            .sub_goals
            .iter()
            .map(|text| json!({ "id": short_id("i"), "kind": "sub", "text": text, "status": "n" }))
            .collect();
        let mut data = json!({ "title": goal.title, "status": "n", "items": items });
        if let Some(note) = &goal.note {
            data["note"] = json!(note);
        }
        nodes.push(json!({ "id": plan, "type": "plan", "position": { "x": BLOCK_X - 120.0, "y": y }, "data": data }));
        nodes.push(json!({
            "id": output, "type": "output", "position": { "x": OUTPUT_X + 80.0, "y": y },
            "data": { "target": "plans", "scope": "team", "order": [plan] },
        }));
        edges.push(edge(&plan, &output, "plan"));
        y += 200.0 + 40.0 * goal.sub_goals.len() as f64;
    }

    let mut groups: BTreeMap<usize, Vec<&Block>> = BTreeMap::new();
    for block in blocks {
        let target = target_of(block.kind, tool);
        let index = TARGET_ORDER.iter().position(|t| *t == target).unwrap_or(0);
        groups.entry(index).or_default().push(block);
    }
    for (index, group) in groups {
        let output = short_id("o");
        let mut order = Vec::new();
        for (row, block) in group.iter().enumerate() {
            let id = short_id("b");
            nodes.push(json!({
                "id": id, "type": "block",
                "position": { "x": BLOCK_X, "y": y + row as f64 * ROW },
                "data": { "blockId": block.id },
            }));
            edges.push(edge(&id, &output, &kind_name(block.kind)));
            order.push(id);
        }
        nodes.push(json!({
            "id": output, "type": "output", "position": { "x": OUTPUT_X, "y": y },
            "data": { "target": TARGET_ORDER[index], "scope": "team", "order": order },
        }));
        y += (group.len() as f64 * ROW).max(260.0) + 60.0;
    }

    json!({
        "version": 1,
        "id": format!("d{}", uuid::Uuid::new_v4().simple()),
        "name": name,
        "updatedAt": now_ms(),
        "nodes": nodes,
        "edges": edges,
    })
}

/// Lets tests point `Server` at a folder laid out like the app's.
#[cfg(test)]
pub fn write_state(data_dir: &std::path::Path, state: &Value) {
    let dir = bridge::dir_in(data_dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("state.json"), state.to_string()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("encela-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn inbox(dir: &std::path::Path) -> Vec<Value> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(bridge::dir_in(dir).join("inbox"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        files
            .iter()
            .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
            .collect()
    }

    fn text(result: &Value) -> Value {
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn branch_context_reads_the_export_for_a_non_git_folder() {
        let dir = temp();
        let project = temp();
        let key = format!(
            "path:{}",
            std::fs::canonicalize(&project).unwrap().display()
        );
        write_state(
            &dir,
            &json!({
                "version": 1, "heartbeat": now_ms(), "locale": "ko", "paths": {},
                "repos": { key.clone(): { "": { "note": "n1", "todos": [{ "id": "t1", "text": "x", "done": false }] } } },
            }),
        );
        let server = Server::new(dir.clone());
        let args = json!({ "path": project.display().to_string() });
        let out = text(&server.call("get_branch_context", &args).unwrap());
        assert_eq!(out["repo"]["key"], key);
        assert_eq!(out["note"], "n1");
        assert_eq!(out["todos"][0]["id"], "t1");
        assert_eq!(out["appRunning"], true);

        // Writes go to the inbox in the app's request format.
        let added = server
            .call(
                "add_todo",
                &json!({ "text": "y", "path": project.display().to_string() }),
            )
            .unwrap();
        assert!(added.get("isError").is_none());
        let done = server
            .call("set_todo_done", &json!({ "id": "t1", "done": true }))
            .unwrap();
        assert!(done.get("isError").is_none());
        let requests = inbox(&dir);
        assert_eq!(requests[0]["op"], "addTodo");
        assert_eq!(requests[0]["repoKey"], key);
        assert_eq!(
            requests[1],
            json!({ "op": "setDone", "id": "t1", "done": true })
        );
        for r in requests {
            serde_json::from_value::<Request>(r).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn bad_input_is_a_tool_error_not_a_protocol_error() {
        let dir = temp();
        let server = Server::new(dir.clone());
        let out = server.call("add_todo", &json!({})).unwrap();
        assert_eq!(out["isError"], true);
        assert!(server.call("no_such_tool", &json!({})).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn search_matches_every_word_and_filters_by_kind() {
        let catalog = catalog::all(&[], None);
        let all = search(&catalog, "", None);
        assert_eq!(all.len(), catalog.len());
        let rules = search(&catalog, "", Some(BlockKind::Rule));
        assert!(!rules.is_empty() && rules.iter().all(|b| b.kind == BlockKind::Rule));
        let ts = search(&catalog, "typescript", Some(BlockKind::Rule));
        assert!(ts.iter().any(|b| b.id == "builtin:typescript"));
        assert!(search(&catalog, "typescript zzzz-nothing", None).is_empty());
    }

    #[test]
    fn a_created_document_compiles_and_reaches_the_inbox() {
        let dir = temp();
        let server = Server::new(dir.clone());
        let args = json!({
            "name": "From MCP",
            "blockIds": ["builtin:concise-replies", "builtin:typescript", "builtin:review-checklist", "builtin:debugger", "builtin:concise-replies"],
            "goal": { "title": "Ship login", "subGoals": ["API", "UI"] },
        });
        let out = server.call("create_document", &args).unwrap();
        assert!(out.get("isError").is_none(), "{out}");
        let out = text(&out);
        let paths: Vec<&str> = out["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"CLAUDE.md"), "{paths:?}");
        assert!(paths.contains(&"PLANS.md"), "{paths:?}");
        assert!(
            paths.iter().any(|p| p.starts_with(".claude/rules/")),
            "{paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.starts_with(".claude/agents/")),
            "{paths:?}"
        );

        let requests = inbox(&dir);
        assert_eq!(requests.len(), 1);
        let doc = &requests[0]["doc"];
        assert!(crate::composer::valid_doc_id(doc["id"].as_str().unwrap()));
        // The duplicate id became one block node.
        let blocks = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["type"] == "block")
            .count();
        assert_eq!(blocks, 4);
        match serde_json::from_value::<Request>(requests[0].clone()).unwrap() {
            Request::SaveDoc { doc } => {
                crate::composer::parse_doc(doc).unwrap();
            }
            other => panic!("{other:?}"),
        }

        let unknown = server
            .call(
                "create_document",
                &json!({ "name": "x", "blockIds": ["builtin:nope"] }),
            )
            .unwrap();
        assert_eq!(unknown["isError"], true);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn codex_rules_go_to_the_instructions() {
        let catalog = catalog::all(&[], None);
        let ts = catalog
            .iter()
            .find(|b| b.id == "builtin:typescript")
            .unwrap();
        let doc = build_doc("x", &[ts], None, ToolId::Codex);
        let output = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["type"] == "output")
            .unwrap();
        assert_eq!(output["data"]["target"], "instructions");
    }
}
