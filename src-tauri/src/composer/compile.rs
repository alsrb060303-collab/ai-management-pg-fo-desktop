//! Turns a composer graph into the files it describes. Pure: no IO.
//!
//! Output nodes decide where blocks go:
//! - CLAUDE.md (team) / CLAUDE.local.md (personal): a managed section holding
//!   the connected instruction blocks in the node's order
//! - rules: one `.claude/rules/<name>.md` per block
//! - skills: `.claude/skills/<name>/SKILL.md`
//! - agents: `.claude/agents/<name>.md`
//! - custom: a managed section in a file at a path the user types, holding
//!   the user's own text and the connected instruction and rule blocks
//! - hooks: the `hooks` of `.claude/settings.json` (or `settings.local.json`),
//!   merged into the file by `write`
//! - plans: a managed section in `PLANS.md` (`PLANS.local.md`) holding the
//!   connected plan nodes (`plans`), and a line in CLAUDE.md pointing to it

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::catalog::{Block, BlockKind, normalize};
use super::plans::{self, Plan, PlanItem};
use crate::library::{Mode, check_id};

/// CLAUDE.md guidance: longer files are followed less reliably.
pub const CLAUDE_MD_LINE_BUDGET: usize = 200;

/// Model aliases a subagent's `model` can be set to. Claude Code resolves
/// each to the newest model of that family.
pub const MODEL_ALIASES: &[&str] = &["fable", "opus", "sonnet", "haiku"];

/// Claude Code hook events (code.claude.com/docs/en/hooks, 2026-10).
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "Setup",
    "SessionEnd",
    "UserPromptSubmit",
    "UserPromptExpansion",
    "Stop",
    "StopFailure",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "SubagentStart",
    "SubagentStop",
    "TaskCreated",
    "TaskCompleted",
    "TeammateIdle",
    "PreCompact",
    "PostCompact",
    "PreModelSwitch",
    "PostModelSwitch",
    "ConfigChange",
    "InstructionsLoaded",
    "CwdChanged",
    "DirectoryAdded",
    "FileChanged",
    "WorktreeCreate",
    "WorktreeRemove",
    "Notification",
    "MessageDisplay",
    "Elicitation",
    "ElicitationResult",
];

/// Events whose `matcher` Claude Code ignores; it is left out for them.
const EVENTS_WITHOUT_MATCHER: &[&str] = &[
    "UserPromptSubmit",
    "PostToolBatch",
    "Stop",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "WorktreeCreate",
    "WorktreeRemove",
    "MessageDisplay",
    "CwdChanged",
];

/// The graph as saved by the frontend. Only the fields compilation needs are
/// read; React Flow's own fields are ignored.
#[derive(Debug, Default, Deserialize)]
pub struct Doc {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub data: NodeData,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeData {
    /// Block nodes: the catalog block.
    pub block_id: Option<String>,
    /// Block nodes: the user's edited text, replacing the catalog body.
    pub body: Option<String>,
    /// Block nodes switched off on the canvas: left out of every output.
    #[serde(default)]
    pub disabled: bool,
    /// Subagent block nodes: a model alias replacing the block's own model.
    pub model: Option<String>,
    /// Hook block nodes: the event and matcher, replacing the block's.
    pub event: Option<String>,
    pub matcher: Option<String>,
    /// Output nodes.
    pub target: Option<Target>,
    pub scope: Option<Mode>,
    /// Output nodes: block node ids in document order.
    #[serde(default)]
    pub order: Vec<String>,
    /// Custom output nodes: the file's path in the project, and the user's
    /// own text put before the connected blocks.
    pub path: Option<String>,
    pub text: Option<String>,
    /// Plan nodes (main goals): see `plans::Plan`.
    pub title: Option<String>,
    pub note: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    ClaudeMd,
    Rules,
    Skills,
    Agents,
    Custom,
    Hooks,
    Plans,
}

impl Target {
    pub fn accepts(self, kind: BlockKind) -> bool {
        match self {
            Target::ClaudeMd => kind == BlockKind::Instruction,
            Target::Rules | Target::Custom => {
                matches!(kind, BlockKind::Rule | BlockKind::Instruction)
            }
            Target::Skills => kind == BlockKind::Skill,
            Target::Agents => kind == BlockKind::Agent,
            Target::Hooks => kind == BlockKind::Hook,
            // Plan nodes, not catalog blocks.
            Target::Plans => false,
        }
    }
}

/// A custom output's path, `/`-separated, or why it can't be used. Only text
/// files the managed section's HTML comments suit: Markdown, `.txt`, or
/// files without an extension (`.cursorrules`).
pub fn custom_path(raw: &str) -> Result<String, String> {
    let path = raw.trim().replace('\\', "/");
    if path.is_empty() {
        return Err(crate::tr!("파일 경로를 입력해 주세요", "Enter a file path"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    let escapes = path.starts_with('/')
        || path.contains(':')
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..");
    if escapes || parts[0].eq_ignore_ascii_case(".git") {
        return Err(crate::tr!(
            "프로젝트 안의 상대 경로만 쓸 수 있어요: {path}",
            "Only a relative path inside the project works: {path}"
        ));
    }
    let name = parts[parts.len() - 1];
    let ext = name
        .rfind('.')
        .filter(|&i| i > 0)
        .map(|i| name[i + 1..].to_ascii_lowercase());
    if !matches!(
        ext.as_deref(),
        None | Some("md" | "mdc" | "markdown" | "txt")
    ) {
        return Err(crate::tr!(
            "Markdown(.md)이나 .txt 파일만 쓸 수 있어요: {path}",
            "Only Markdown (.md) or .txt files work: {path}"
        ));
    }
    Ok(path)
}

/// The text of section outputs (CLAUDE.md, custom): blocks in order, third-
/// party ones credited in an HTML comment (which Claude Code strips).
fn section_text(resolved: &[(&str, &Block, String)]) -> String {
    resolved
        .iter()
        .map(|(_, block, body)| match &block.origin {
            Some(o) => format!(
                "<!-- Source: {} ({}, {}) -->\n{body}",
                o.url(),
                o.license,
                o.copyright
            ),
            None => body.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutFile {
    /// Relative to the project, `/`-separated.
    pub path: String,
    /// The whole file, or for `section` files only the managed section's body.
    pub content: String,
    /// Written as a managed section inside a file the user may also edit.
    pub section: bool,
    pub personal: bool,
    pub target: Target,
    /// The output node and the block nodes that produced this file.
    pub node: String,
    pub blocks: Vec<String>,
    pub lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub node: Option<String>,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compiled {
    pub files: Vec<OutFile>,
    pub warnings: Vec<Warning>,
}

pub fn compile(doc: &Doc, catalog: &[Block]) -> Compiled {
    let blocks: HashMap<&str, &Block> = catalog.iter().map(|b| (b.id.as_str(), b)).collect();
    let nodes: HashMap<&str, &Node> = doc.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = Compiled::default();
    let mut paths = HashSet::new();

    let warn = |out: &mut Compiled, node: &str, message: String| {
        out.warnings.push(Warning {
            node: Some(node.to_string()),
            message,
        });
    };

    for output in doc.nodes.iter().filter(|n| n.kind == "output") {
        let Some(target) = output.data.target else {
            continue;
        };
        let personal = output.data.scope == Some(Mode::Personal);
        // After the others: steps check which skills and subagents are written.
        if target == Target::Plans {
            continue;
        }

        // Connected block nodes, in the node's saved order, then any others in
        // edge order (e.g. connected after the order was last saved).
        let mut inputs: Vec<&str> = Vec::new();
        let connected: Vec<&str> = doc
            .edges
            .iter()
            .filter(|e| e.target == output.id)
            .map(|e| e.source.as_str())
            .collect();
        for id in output
            .data
            .order
            .iter()
            .map(String::as_str)
            .chain(connected.iter().copied())
        {
            if connected.contains(&id) && !inputs.contains(&id) {
                inputs.push(id);
            }
        }

        let mut resolved: Vec<(&str, &Block, String)> = Vec::new();
        let mut seen_blocks = HashSet::new();
        for node_id in inputs {
            let Some(node) = nodes.get(node_id) else {
                continue;
            };
            if node.data.disabled {
                continue;
            }
            let Some(block_id) = node.data.block_id.as_deref() else {
                continue;
            };
            let Some(block) = blocks.get(block_id) else {
                warn(
                    &mut out,
                    node_id,
                    crate::tr!(
                        "블록을 찾을 수 없어요: {block_id}",
                        "Block not found: {block_id}"
                    ),
                );
                continue;
            };
            if !target.accepts(block.kind) {
                warn(
                    &mut out,
                    node_id,
                    crate::tr!(
                        "'{}'은(는) 여기에 연결할 수 없어요",
                        "'{}' can't be connected here",
                        block.display_title()
                    ),
                );
                continue;
            }
            if !seen_blocks.insert(block_id) {
                warn(
                    &mut out,
                    node_id,
                    crate::tr!(
                        "'{}' 블록이 두 번 연결됐어요",
                        "The '{}' block is connected twice",
                        block.display_title()
                    ),
                );
                continue;
            }
            let edited = node.data.body.as_deref().map(normalize);
            let body = edited.as_deref().unwrap_or(&block.body).trim();
            if has_import_like(body) {
                warn(
                    &mut out,
                    node_id,
                    crate::tr!(
                        "'{}' 본문의 @경로는 Claude Code가 파일 불러오기로 읽어요. 의도한 게 아니면 `@...`처럼 백틱으로 감싸 주세요",
                        "Claude Code reads @paths in the '{}' text as file imports. If that's not intended, wrap them in backticks like `@...`",
                        block.display_title()
                    ),
                );
            }
            resolved.push((node_id, block, body.to_string()));
        }

        let own_text = output
            .data
            .text
            .as_deref()
            .map(normalize)
            .filter(|t| target == Target::Custom && !t.trim().is_empty());
        // An empty output shows its own hint on the canvas.
        if resolved.is_empty() && own_text.is_none() {
            continue;
        }

        let mut files = Vec::new();
        match target {
            Target::ClaudeMd => {
                let content = section_text(&resolved);
                let lines = content.lines().count();
                if lines > CLAUDE_MD_LINE_BUDGET {
                    warn(
                        &mut out,
                        &output.id,
                        crate::tr!(
                            "{lines}줄이에요. CLAUDE.md는 {CLAUDE_MD_LINE_BUDGET}줄 이하일 때 더 잘 지켜져요. 일부를 규칙이나 skill로 옮겨 보세요",
                            "{lines} lines. CLAUDE.md is followed better at {CLAUDE_MD_LINE_BUDGET} lines or fewer. Try moving some of it into rules or skills"
                        ),
                    );
                }
                let path = if personal {
                    "CLAUDE.local.md"
                } else {
                    "CLAUDE.md"
                };
                files.push(OutFile {
                    path: path.into(),
                    lines,
                    content,
                    section: true,
                    personal,
                    target,
                    node: output.id.clone(),
                    blocks: resolved.iter().map(|(id, ..)| id.to_string()).collect(),
                });
            }
            Target::Custom => match custom_path(output.data.path.as_deref().unwrap_or("")) {
                Err(reason) => warn(&mut out, &output.id, reason),
                Ok(path) => {
                    let blocks = section_text(&resolved);
                    let content = match &own_text {
                        Some(text) if blocks.is_empty() => text.trim().to_string(),
                        Some(text) => format!("{}\n\n{blocks}", text.trim()),
                        None => blocks,
                    };
                    files.push(OutFile {
                        path,
                        lines: content.lines().count(),
                        content,
                        section: true,
                        personal,
                        target,
                        node: output.id.clone(),
                        blocks: resolved.iter().map(|(id, ..)| id.to_string()).collect(),
                    });
                }
            },
            Target::Hooks => {
                let mut used = Vec::new();
                let mut hooks = serde_json::Map::new();
                for (node_id, block, body) in &resolved {
                    let data = nodes.get(node_id).map(|n| &n.data);
                    let event = data
                        .and_then(|d| d.event.as_deref())
                        .or(block.event.as_deref())
                        .unwrap_or("")
                        .trim();
                    if !HOOK_EVENTS.contains(&event) {
                        warn(
                            &mut out,
                            node_id,
                            crate::tr!(
                                "'{}'의 이벤트를 골라 주세요",
                                "Pick an event for '{}'",
                                block.display_title()
                            ),
                        );
                        continue;
                    }
                    if body.trim().is_empty() {
                        warn(
                            &mut out,
                            node_id,
                            crate::tr!(
                                "'{}'의 실행할 명령이 비어 있어요",
                                "'{}' has no command to run",
                                block.display_title()
                            ),
                        );
                        continue;
                    }
                    let matcher = data
                        .and_then(|d| d.matcher.as_deref())
                        .or(block.matcher.as_deref())
                        .map(str::trim)
                        .filter(|m| !m.is_empty() && !EVENTS_WITHOUT_MATCHER.contains(&event));
                    add_hook(&mut hooks, event, matcher, body.trim());
                    used.push(node_id.to_string());
                }
                if !used.is_empty() {
                    let content =
                        serde_json::to_string_pretty(&serde_json::json!({ "hooks": hooks }))
                            .unwrap_or_default();
                    let path = if personal {
                        ".claude/settings.local.json"
                    } else {
                        ".claude/settings.json"
                    };
                    files.push(OutFile {
                        path: path.into(),
                        lines: content.lines().count(),
                        content,
                        section: false,
                        personal,
                        target,
                        node: output.id.clone(),
                        blocks: used,
                    });
                }
            }
            Target::Plans => {}
            Target::Rules | Target::Skills | Target::Agents => {
                for (node_id, block, body) in &resolved {
                    if check_id(&block.name).is_err() {
                        warn(
                            &mut out,
                            node_id,
                            crate::tr!(
                                "파일 이름으로 쓸 수 없어요: {}",
                                "Can't be used as a file name: {}",
                                block.name
                            ),
                        );
                        continue;
                    }
                    let (path, content) = match target {
                        Target::Rules => (
                            format!(".claude/rules/{}.md", block.name),
                            rule_file(block, body),
                        ),
                        Target::Skills => (
                            format!(".claude/skills/{}/SKILL.md", block.name),
                            skill_file(block, body),
                        ),
                        _ => {
                            let model = nodes
                                .get(node_id)
                                .and_then(|n| n.data.model.as_deref())
                                .filter(|m| !m.is_empty());
                            if let Some(m) = model.filter(|m| !MODEL_ALIASES.contains(m)) {
                                warn(
                                    &mut out,
                                    node_id,
                                    crate::tr!(
                                        "알 수 없는 모델이라 무시했어요: {m}",
                                        "Ignored an unknown model: {m}"
                                    ),
                                );
                            }
                            (
                                format!(".claude/agents/{}.md", block.name),
                                agent_file(
                                    block,
                                    body,
                                    model.filter(|m| MODEL_ALIASES.contains(m)),
                                ),
                            )
                        }
                    };
                    if block.extra_files > 0 {
                        warn(
                            &mut out,
                            node_id,
                            crate::tr!(
                                "'{}'의 부속 파일 {}개는 복사되지 않아요. 통째로 넣으려면 라이브러리의 '적용'을 쓰세요",
                                "The {1} extra file(s) of '{0}' aren't copied. To add the whole skill, use 'Apply' in the library",
                                block.display_title(),
                                block.extra_files
                            ),
                        );
                    }
                    files.push(OutFile {
                        path,
                        lines: content.lines().count(),
                        content,
                        section: false,
                        personal,
                        target,
                        node: output.id.clone(),
                        blocks: vec![node_id.to_string()],
                    });
                    // A bundled skill's references go next to its SKILL.md,
                    // at the same relative paths its body links to.
                    if target == Target::Skills {
                        for (rel, text) in &block.companions {
                            files.push(OutFile {
                                path: format!(".claude/skills/{}/{rel}", block.name),
                                lines: text.lines().count(),
                                content: text.to_string(),
                                section: false,
                                personal,
                                target,
                                node: output.id.clone(),
                                blocks: vec![node_id.to_string()],
                            });
                        }
                    }
                }
            }
        }

        for file in files {
            if paths.insert(file.path.clone()) {
                out.files.push(file);
            } else {
                let message = crate::tr!(
                    "{}을(를) 두 출력이 같이 쓰려고 해요. 하나만 남겨 주세요",
                    "Two outputs both write {}. Keep only one",
                    file.path
                );
                warn(&mut out, &output.id, message);
            }
        }
    }
    compile_plans(doc, &nodes, &blocks, &mut out);
    out
}

/// Connected node ids of an output, saved order first.
fn inputs_of<'a>(doc: &'a Doc, output: &'a Node) -> Vec<&'a str> {
    let connected: Vec<&str> = doc
        .edges
        .iter()
        .filter(|e| e.target == output.id)
        .map(|e| e.source.as_str())
        .collect();
    let mut inputs = Vec::new();
    for id in output
        .data
        .order
        .iter()
        .map(String::as_str)
        .chain(connected.iter().copied())
    {
        if connected.contains(&id) && !inputs.contains(&id) {
            inputs.push(id);
        }
    }
    inputs
}

/// PLANS outputs, and the line pointing CLAUDE.md to them.
fn compile_plans(
    doc: &Doc,
    nodes: &HashMap<&str, &Node>,
    blocks: &HashMap<&str, &Block>,
    out: &mut Compiled,
) {
    // Catalog ids of the skills and subagents this document writes.
    let written: Vec<String> = out
        .files
        .iter()
        .filter(|f| matches!(f.target, Target::Skills | Target::Agents))
        .flat_map(|f| f.blocks.iter())
        .filter_map(|id| nodes.get(id.as_str())?.data.block_id.clone())
        .collect();
    for output in doc
        .nodes
        .iter()
        .filter(|n| n.kind == "output" && n.data.target == Some(Target::Plans))
    {
        let personal = output.data.scope == Some(Mode::Personal);
        let mut issues = Vec::new();
        let mut parts = Vec::new();
        let mut used = Vec::new();
        for id in inputs_of(doc, output) {
            let Some(node) = nodes.get(id).filter(|n| n.kind == "plan") else {
                continue;
            };
            if node.data.disabled {
                continue;
            }
            let plan = Plan {
                title: node.data.title.clone().unwrap_or_default(),
                note: node.data.note.clone(),
                status: node.data.status.clone(),
                items: node.data.items.clone(),
            };
            parts.push(plans::render(id, &plan, blocks, &written, &mut issues));
            used.push(id.to_string());
        }
        for issue in issues {
            out.warnings.push(Warning {
                node: Some(issue.node),
                message: issue.message,
            });
        }
        if parts.is_empty() {
            continue;
        }
        let path = if personal {
            "PLANS.local.md"
        } else {
            "PLANS.md"
        };
        if out.files.iter().any(|f| f.path == path) {
            out.warnings.push(Warning {
                node: Some(output.id.clone()),
                message: crate::tr!(
                    "{path}을(를) 두 출력이 같이 쓰려고 해요. 하나만 남겨 주세요",
                    "Two outputs both write {path}. Keep only one"
                ),
            });
            continue;
        }
        let content = format!("{}\n\n{}", plans::header(), parts.join("\n\n"));
        out.files.push(OutFile {
            path: path.into(),
            lines: content.lines().count(),
            content,
            section: true,
            personal,
            target: Target::Plans,
            node: output.id.clone(),
            blocks: used,
        });

        // Claude Code doesn't read PLANS.md by itself: CLAUDE.md points to it.
        let pointer = plans::pointer(path);
        let claude_md = if personal {
            "CLAUDE.local.md"
        } else {
            "CLAUDE.md"
        };
        if let Some(file) = out.files.iter_mut().find(|f| f.path == claude_md) {
            file.content = format!("{}\n\n{pointer}", file.content);
            file.lines = file.content.lines().count();
        } else if let Some(node) = doc.nodes.iter().find(|n| {
            n.kind == "output"
                && n.data.target == Some(Target::ClaudeMd)
                && (n.data.scope == Some(Mode::Personal)) == personal
        }) {
            out.files.push(OutFile {
                path: claude_md.into(),
                lines: pointer.lines().count(),
                content: pointer,
                section: true,
                personal,
                target: Target::ClaudeMd,
                node: node.id.clone(),
                blocks: Vec::new(),
            });
        } else {
            out.warnings.push(Warning {
                node: Some(output.id.clone()),
                message: crate::tr!(
                    "Claude Code는 {path}를 스스로 읽지 않아요. {claude_md} 출력({})을 추가하면 계획을 읽으라는 안내가 들어가요",
                    "Claude Code doesn't read {path} by itself. Add a {claude_md} output ({}) and it gets a line pointing to the plan",
                    if personal { crate::tr!("나만", "Just me") } else { crate::tr!("팀", "Team") }
                ),
            });
        }
    }
}

/// YAML double-quoted scalar, safe for any text.
fn quoted(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ");
    format!("\"{escaped}\"")
}

fn rule_file(block: &Block, body: &str) -> String {
    let credit = credit_comment(block);
    if block.paths.is_empty() && credit.is_empty() {
        return format!("{body}\n");
    }
    let mut fm = format!("---\n{credit}");
    if !block.paths.is_empty() {
        fm.push_str("paths:\n");
        for p in &block.paths {
            fm.push_str(&format!("  - {}\n", quoted(p)));
        }
    }
    format!("{fm}---\n\n{body}\n")
}

fn skill_file(block: &Block, body: &str) -> String {
    if let Some(raw) = &block.frontmatter {
        return format!("---\n{raw}\n---\n\n{body}\n");
    }
    let description = block.description.as_deref().unwrap_or(&block.summary);
    let mut fm = format!(
        "---\nname: {}\ndescription: {}\n",
        block.name,
        quoted(description)
    );
    if block.manual {
        fm.push_str("disable-model-invocation: true\n");
    }
    // `license` and `metadata` are portable Agent Skills fields.
    if let Some(origin) = &block.origin {
        fm.push_str(&format!(
            "license: {}\nmetadata:\n  source: {}\n  copyright: {}\n",
            origin.license,
            quoted(&origin.url()),
            quoted(&origin.copyright)
        ));
    }
    format!("{fm}---\n\n{body}\n")
}

/// A YAML comment line crediting a third-party block, for frontmatter.
fn credit_comment(block: &Block) -> String {
    block
        .origin
        .as_ref()
        .map(|o| format!("# Source: {} ({}, {})\n", o.url(), o.license, o.copyright))
        .unwrap_or_default()
}

/// Adds one command hook under `event` (and `matcher`), next to the others
/// with the same matcher, as settings.json lays them out.
pub(super) fn add_hook(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    matcher: Option<&str>,
    command: &str,
) {
    use serde_json::{Value, json};
    let groups = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(groups) = groups.as_array_mut() else {
        return;
    };
    let same = |g: &Value| g.get("matcher").and_then(Value::as_str) == matcher;
    let index = match groups.iter().position(same) {
        Some(i) => i,
        None => {
            let mut group = serde_json::Map::new();
            if let Some(m) = matcher {
                group.insert("matcher".into(), json!(m));
            }
            group.insert("hooks".into(), json!([]));
            groups.push(Value::Object(group));
            groups.len() - 1
        }
    };
    let hook = json!({ "type": "command", "command": command });
    if let Some(list) = groups[index].get_mut("hooks").and_then(Value::as_array_mut)
        && !list.contains(&hook)
    {
        list.push(hook);
    }
}

/// Frontmatter with its top-level `model:` line set to `model`, or the line
/// added at the end when it has none.
fn set_model(frontmatter: &str, model: &str) -> String {
    let mut found = false;
    let mut lines: Vec<String> = frontmatter
        .lines()
        .map(|line| {
            if !found && line.starts_with("model:") {
                found = true;
                format!("model: {model}")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(format!("model: {model}"));
    }
    lines.join("\n")
}

fn agent_file(block: &Block, body: &str, model: Option<&str>) -> String {
    if let Some(raw) = &block.frontmatter {
        let raw = match model {
            Some(m) => set_model(raw, m),
            None => raw.clone(),
        };
        return format!("---\n{raw}\n---\n\n{body}\n");
    }
    let description = block.description.as_deref().unwrap_or(&block.summary);
    let mut fm = format!(
        "---\n{}name: {}\ndescription: {}\n",
        credit_comment(block),
        block.name,
        quoted(description)
    );
    if let Some(tools) = block.tools.as_deref().filter(|t| !t.is_empty()) {
        fm.push_str(&format!("tools: {tools}\n"));
    }
    if let Some(model) = model.or(block.model.as_deref()).filter(|m| !m.is_empty()) {
        fm.push_str(&format!("model: {model}\n"));
    }
    format!("{fm}---\n\n{body}\n")
}

/// `@token`s outside code (fenced blocks with ``` or ~~~, and inline code
/// spans), which CLAUDE.md treats as imports. E-mail addresses are skipped.
pub fn import_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) => {
                fence = Some(m);
                continue;
            }
            (Some(open), Some(m)) if open == m => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        // Drop inline code spans.
        let mut plain = String::new();
        for (i, part) in line.split('`').enumerate() {
            if i % 2 == 0 {
                plain.push_str(part);
                plain.push(' ');
            }
        }
        for word in plain.split_whitespace() {
            if let Some(rest) = word.strip_prefix('@') {
                let rest = rest.trim_end_matches(['.', ',', ';', ':', ')', '!', '?']);
                if !rest.is_empty() && !rest.contains('@') {
                    tokens.push(rest.to_string());
                }
            }
        }
    }
    tokens
}

/// Whether text has an `@path`-like token (with `/` or `.`) outside code.
/// Bare `@word`s are only flagged at write time, when the project shows
/// whether such a file exists.
fn has_import_like(text: &str) -> bool {
    import_tokens(text).iter().any(|t| t.contains(['/', '.']))
}

#[cfg(test)]
mod tests {
    use super::super::catalog::Source;
    use super::*;

    fn block(id: &str, kind: BlockKind, body: &str) -> Block {
        Block {
            id: id.into(),
            kind,
            title: id.into(),
            summary: "summary".into(),
            category: String::new(),
            detail: None,
            en: None,
            source: Source::Builtin,
            name: id.split(':').next_back().unwrap().into(),
            description: Some("Does: things".into()),
            paths: Vec::new(),
            tools: Some("Read, Grep".into()),
            model: None,
            event: None,
            matcher: None,
            manual: false,
            body: body.into(),
            extra_files: 0,
            companions: Vec::new(),
            size: 0,
            origin: None,
            frontmatter: None,
        }
    }

    fn doc(json: &str) -> Doc {
        serde_json::from_str(json).unwrap()
    }

    fn catalog() -> Vec<Block> {
        let mut ts = block("builtin:ts", BlockKind::Rule, "# TS");
        ts.paths = vec!["**/*.ts".into(), "src/**/*.{ts,tsx}".into()];
        vec![
            block("builtin:a", BlockKind::Instruction, "## A\n- one"),
            block("builtin:b", BlockKind::Instruction, "## B"),
            ts,
            block("builtin:review", BlockKind::Skill, "Steps"),
            block("builtin:reviewer", BlockKind::Agent, "You review."),
        ]
    }

    #[test]
    fn plans_render_goals_steps_and_point_claude_md_to_them() {
        let d = doc(r#"{
            "nodes": [
                {"id": "r", "type": "block", "data": {"blockId": "builtin:reviewer"}},
                {"id": "ag", "type": "output", "data": {"target": "agents", "scope": "team"}},
                {"id": "c", "type": "output", "data": {"target": "claudeMd", "scope": "team"}},
                {"id": "g", "type": "plan", "data": {"title": "App account", "status": "p", "note": "Sync follows the account", "items": [
                    {"kind": "detail", "text": "Orphan becomes a sub-goal"},
                    {"kind": "sub", "text": "Sign-in", "status": "y"},
                    {"kind": "step", "action": "agent", "block": "builtin:reviewer", "text": "check the token handling"},
                    {"kind": "detail", "text": "Google", "status": "p", "note": "loopback"},
                    {"kind": "step", "action": "ask"},
                    {"kind": "step", "action": "skill", "block": "builtin:review"},
                    {"kind": "step", "action": "text", "text": "  "}
                ]}},
                {"id": "p", "type": "output", "data": {"target": "plans", "scope": "team", "order": ["g"]}}
            ],
            "edges": [{"source": "r", "target": "ag"}, {"source": "g", "target": "p"}]
        }"#);
        let out = compile(&d, &catalog());
        let plan = out.files.iter().find(|f| f.path == "PLANS.md").unwrap();
        assert!(plan.section);
        assert_eq!(plan.blocks, ["g"]);
        assert!(plan.content.starts_with("# PLANS\n\n"));
        let body = &plan.content[plan.content.find("## [p]").unwrap()..];
        assert_eq!(
            body,
            "## [p] App account\n\nSync follows the account\n\n### [n] 1. Orphan becomes a sub-goal\n\n### [y] 2. Sign-in\n\n> **단계 지침**: 이 일은 `reviewer` subagent에게 맡겨요: check the token handling\n\n- [p] 2.1 Google\n  loopback\n\n  > **단계 지침**: 여기서 멈추고, 다음 목표로 넘어가기 전에 사용자에게 확인받아요\n\n  > **단계 지침**: `review` skill을 써요"
        );
        // The skill step names a skill no output writes.
        assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
        assert_eq!(out.warnings[0].node.as_deref(), Some("g"));
        // CLAUDE.md had no blocks; it still gets the pointer.
        let claude = out.files.iter().find(|f| f.path == "CLAUDE.md").unwrap();
        assert!(claude.content.contains("`PLANS.md`"));
        assert_eq!(claude.node, "c");
    }

    #[test]
    fn plans_without_a_claude_md_output_warn() {
        let d = doc(r#"{
            "nodes": [
                {"id": "g", "type": "plan", "data": {"title": "Goal"}},
                {"id": "p", "type": "output", "data": {"target": "plans", "scope": "personal"}}
            ],
            "edges": [{"source": "g", "target": "p"}]
        }"#);
        let out = compile(&d, &catalog());
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].path, "PLANS.local.md");
        assert!(out.files[0].personal);
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.warnings[0].node.as_deref(), Some("p"));
    }

    #[test]
    fn claude_md_follows_the_saved_order_then_edges() {
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "y", "type": "block", "data": {"blockId": "builtin:b"}},
                {"id": "o", "type": "output", "data": {"target": "claudeMd", "scope": "team", "order": ["y", "gone"]}}
            ],
            "edges": [{"source": "x", "target": "o"}, {"source": "y", "target": "o"}]
        }"#);
        let out = compile(&d, &catalog());
        assert_eq!(out.files.len(), 1);
        let f = &out.files[0];
        assert_eq!(f.path, "CLAUDE.md");
        assert!(f.section && !f.personal);
        assert_eq!(f.content, "## B\n\n## A\n- one");
        assert_eq!(f.blocks, ["y", "x"]);
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn personal_claude_md_is_the_local_file_and_edits_replace_the_body() {
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a", "body": "Mine\n"}},
                {"id": "o", "type": "output", "data": {"target": "claudeMd", "scope": "personal"}}
            ],
            "edges": [{"source": "x", "target": "o"}]
        }"#);
        let f = &compile(&d, &catalog()).files[0];
        assert_eq!(f.path, "CLAUDE.local.md");
        assert_eq!(f.content, "Mine");
    }

    #[test]
    fn a_skill_writes_its_companion_files_next_to_skill_md() {
        let mut skill = block("builtin:guide", BlockKind::Skill, "See references/a.md");
        skill.companions = vec![("references/a.md".into(), "# A\n")];
        let d = doc(r#"{
            "nodes": [
                {"id": "s", "type": "block", "data": {"blockId": "builtin:guide"}},
                {"id": "S", "type": "output", "data": {"target": "skills"}}
            ],
            "edges": [{"source": "s", "target": "S"}]
        }"#);
        let out = compile(&d, &[skill]);
        let paths: Vec<_> = out.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                ".claude/skills/guide/SKILL.md",
                ".claude/skills/guide/references/a.md"
            ]
        );
        assert_eq!(out.files[1].content, "# A\n");
        assert_eq!(out.files[1].blocks, ["s"]);
    }

    #[test]
    fn writes_rule_skill_and_agent_files_with_frontmatter() {
        let d = doc(r#"{
            "nodes": [
                {"id": "r", "type": "block", "data": {"blockId": "builtin:ts"}},
                {"id": "i", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "s", "type": "block", "data": {"blockId": "builtin:review"}},
                {"id": "g", "type": "block", "data": {"blockId": "builtin:reviewer"}},
                {"id": "R", "type": "output", "data": {"target": "rules"}},
                {"id": "S", "type": "output", "data": {"target": "skills", "scope": "personal"}},
                {"id": "G", "type": "output", "data": {"target": "agents"}}
            ],
            "edges": [
                {"source": "r", "target": "R"}, {"source": "i", "target": "R"},
                {"source": "s", "target": "S"}, {"source": "g", "target": "G"}
            ]
        }"#);
        let out = compile(&d, &catalog());
        let by_path: HashMap<_, _> = out.files.iter().map(|f| (f.path.as_str(), f)).collect();
        assert_eq!(
            by_path[".claude/rules/ts.md"].content,
            "---\npaths:\n  - \"**/*.ts\"\n  - \"src/**/*.{ts,tsx}\"\n---\n\n# TS\n"
        );
        assert_eq!(by_path[".claude/rules/a.md"].content, "## A\n- one\n");
        let skill = by_path[".claude/skills/review/SKILL.md"];
        assert!(skill.personal);
        assert_eq!(
            skill.content,
            "---\nname: review\ndescription: \"Does: things\"\n---\n\nSteps\n"
        );
        assert_eq!(
            by_path[".claude/agents/reviewer.md"].content,
            "---\nname: reviewer\ndescription: \"Does: things\"\ntools: Read, Grep\n---\n\nYou review.\n"
        );
    }

    #[test]
    fn warns_about_bad_links_duplicates_budget_and_imports() {
        let long = (0..=CLAUDE_MD_LINE_BUDGET)
            .map(|i| format!("- {i}\n"))
            .collect::<String>();
        let d = doc(&format!(
            r#"{{
            "nodes": [
                {{"id": "x", "type": "block", "data": {{"blockId": "builtin:a", "body": {long:?}}}}},
                {{"id": "x2", "type": "block", "data": {{"blockId": "builtin:a"}}}},
                {{"id": "s", "type": "block", "data": {{"blockId": "builtin:review"}}}},
                {{"id": "m", "type": "block", "data": {{"blockId": "builtin:missing"}}}},
                {{"id": "b", "type": "block", "data": {{"blockId": "builtin:b", "body": "See @docs/api.md and `@not/this`"}}}},
                {{"id": "o", "type": "output", "data": {{"target": "claudeMd"}}}},
                {{"id": "o2", "type": "output", "data": {{"target": "claudeMd"}}}},
                {{"id": "empty", "type": "output", "data": {{"target": "agents"}}}}
            ],
            "edges": [
                {{"source": "x", "target": "o"}}, {{"source": "x2", "target": "o"}},
                {{"source": "s", "target": "o"}}, {{"source": "m", "target": "o"}},
                {{"source": "b", "target": "o2"}}
            ]
        }}"#
        ));
        let out = compile(&d, &catalog());
        let messages: Vec<_> = out.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("두 번 연결")));
        assert!(messages.iter().any(|m| m.contains("연결할 수 없어요")));
        assert!(messages.iter().any(|m| m.contains("찾을 수 없어요")));
        assert!(messages.iter().any(|m| m.contains("줄이에요")));
        assert!(messages.iter().any(|m| m.contains("@경로")));
        assert!(messages.iter().any(|m| m.contains("두 출력이")));
        assert_eq!(out.files.len(), 1);
    }

    #[test]
    fn third_party_blocks_carry_their_source_and_license() {
        use super::super::catalog::Origin;
        let origin = Origin {
            repo: "o/r".into(),
            commit: "a".repeat(40),
            path: "skills/x/SKILL.md".into(),
            license: "MIT".into(),
            copyright: "Copyright (c) 2025 X".into(),
        };
        let mut skill = block("external:x", BlockKind::Skill, "Steps");
        skill.origin = Some(origin.clone());
        let mut agent = block("external:y", BlockKind::Agent, "You help.");
        agent.origin = Some(origin);
        let url = format!(
            "https://github.com/o/r/blob/{}/skills/x/SKILL.md",
            "a".repeat(40)
        );
        assert_eq!(
            skill_file(&skill, "Steps"),
            format!(
                "---\nname: x\ndescription: \"Does: things\"\nlicense: MIT\nmetadata:\n  source: \"{url}\"\n  copyright: \"Copyright (c) 2025 X\"\n---\n\nSteps\n"
            )
        );
        assert!(agent_file(&agent, "You help.", None).starts_with(&format!(
            "---\n# Source: {url} (MIT, Copyright (c) 2025 X)\nname: y\n"
        )));
    }

    #[test]
    fn library_blocks_keep_their_own_frontmatter() {
        let mut agent = block("library:agent:reviewer", BlockKind::Agent, "Review.");
        agent.frontmatter =
            Some("name: code-reviewer\ndescription: x\npermissionMode: plan".into());
        assert_eq!(
            agent_file(&agent, "Review.", None),
            "---\nname: code-reviewer\ndescription: x\npermissionMode: plan\n---\n\nReview.\n"
        );
    }

    #[test]
    fn a_chosen_model_replaces_or_adds_the_model_line() {
        let mut agent = block("library:agent:r", BlockKind::Agent, "Review.");
        agent.frontmatter = Some("name: r\nmodel: sonnet\ndescription: x".into());
        assert_eq!(
            agent_file(&agent, "Review.", Some("opus")),
            "---\nname: r\nmodel: opus\ndescription: x\n---\n\nReview.\n"
        );
        agent.frontmatter = Some("name: r\ndescription: x".into());
        assert!(
            agent_file(&agent, "Review.", Some("haiku"))
                .contains("description: x\nmodel: haiku\n---")
        );

        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:reviewer", "model": "fable"}},
                {"id": "y", "type": "block", "data": {"blockId": "builtin:reviewer", "model": "gpt"}},
                {"id": "o", "type": "output", "data": {"target": "agents"}},
                {"id": "p", "type": "output", "data": {"target": "agents", "scope": "personal"}}
            ],
            "edges": [{"source": "x", "target": "o"}, {"source": "y", "target": "p"}]
        }"#);
        let out = compile(&d, &catalog());
        assert!(out.files[0].content.contains("\nmodel: fable\n"));
        assert_eq!(out.warnings.len(), 2, "{:?}", out.warnings); // unknown model + same path twice
    }

    #[test]
    fn custom_outputs_hold_the_users_text_then_blocks() {
        let d = doc(r##"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "o", "type": "output", "data": {"target": "custom", "path": "docs\\AGENTS.md", "text": "# Mine\r\n"}},
                {"id": "e", "type": "output", "data": {"target": "custom", "path": "", "text": "x"}}
            ],
            "edges": [{"source": "x", "target": "o"}]
        }"##);
        let out = compile(&d, &catalog());
        assert_eq!(out.files.len(), 1);
        let f = &out.files[0];
        assert_eq!(f.path, "docs/AGENTS.md");
        assert!(f.section);
        assert_eq!(f.content, "# Mine\n\n## A\n- one");
        assert_eq!(out.warnings.len(), 1);
        for bad in [
            "../x.md",
            "/etc/x.md",
            "C:/x.md",
            ".git/config",
            "a//b.md",
            ".claude/settings.json",
        ] {
            assert!(custom_path(bad).is_err(), "{bad}");
        }
        for good in ["AGENTS.md", ".cursorrules", "docs/notes.txt", "GEMINI.md"] {
            assert!(custom_path(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn hook_blocks_become_settings_hooks() {
        let mut fmt = block("builtin:fmt", BlockKind::Hook, "pnpm format");
        fmt.event = Some("PostToolUse".into());
        fmt.matcher = Some("Edit|Write".into());
        let mut stop = block("builtin:stop", BlockKind::Hook, "pnpm lint");
        stop.event = Some("Stop".into());
        stop.matcher = Some("ignored".into());
        let blank = block("builtin:blank", BlockKind::Hook, "");
        let d = doc(r#"{
            "nodes": [
                {"id": "a", "type": "block", "data": {"blockId": "builtin:fmt"}},
                {"id": "b", "type": "block", "data": {"blockId": "builtin:stop"}},
                {"id": "c", "type": "block", "data": {"blockId": "builtin:blank", "event": "PreToolUse", "body": "echo hi"}},
                {"id": "d", "type": "block", "data": {"blockId": "builtin:blank", "event": "Nope", "body": "x"}},
                {"id": "o", "type": "output", "data": {"target": "hooks", "scope": "personal"}}
            ],
            "edges": [{"source": "a", "target": "o"}, {"source": "b", "target": "o"},
                      {"source": "c", "target": "o"}, {"source": "d", "target": "o"}]
        }"#);
        let out = compile(&d, &[fmt, stop, blank]);
        let f = &out.files[0];
        assert_eq!(f.path, ".claude/settings.local.json");
        assert_eq!(f.blocks, ["a", "b", "c"]);
        let v: serde_json::Value = serde_json::from_str(&f.content).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"hooks": {
                "PostToolUse": [{"matcher": "Edit|Write", "hooks": [{"type": "command", "command": "pnpm format"}]}],
                "Stop": [{"hooks": [{"type": "command", "command": "pnpm lint"}]}],
                "PreToolUse": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
            }})
        );
        // "d": the same block twice, and an unknown event.
        assert!(!out.warnings.is_empty());
    }

    #[test]
    fn switched_off_blocks_are_left_out() {
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a", "disabled": true}},
                {"id": "y", "type": "block", "data": {"blockId": "builtin:b"}},
                {"id": "o", "type": "output", "data": {"target": "claudeMd"}}
            ],
            "edges": [{"source": "x", "target": "o"}, {"source": "y", "target": "o"}]
        }"#);
        let out = compile(&d, &catalog());
        assert_eq!(out.files[0].content, "## B");
        assert_eq!(out.files[0].blocks, ["y"]);
    }

    #[test]
    fn edited_bodies_are_normalized_to_lf() {
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a", "body": "One\r\nTwo\r\n"}},
                {"id": "o", "type": "output", "data": {"target": "claudeMd"}}
            ],
            "edges": [{"source": "x", "target": "o"}]
        }"#);
        assert_eq!(compile(&d, &catalog()).files[0].content, "One\nTwo");
    }

    #[test]
    fn import_detection_skips_code_and_emails() {
        assert!(has_import_like("~~~\n@docs/x.md\n~~~\nSee @a/b.md"));
        assert!(!has_import_like("~~~\n@docs/x.md\n~~~"));
        assert_eq!(
            import_tokens("Terms in @LICENSE. Ask @someone, mail a@b.c"),
            ["LICENSE", "someone"]
        );
        assert!(has_import_like("Read @README.md first"));
        assert!(has_import_like("@~/.claude/notes.md"));
        assert!(!has_import_like("Use `@scope/pkg` here"));
        assert!(!has_import_like("```\n@docs/x.md\n```"));
        assert!(!has_import_like("mail me@example.com or @someone"));
    }
}
