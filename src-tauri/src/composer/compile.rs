//! Turns a composer graph into the files it describes, for one tool (the
//! project's: Claude Code or Codex). Pure: no IO. Graphs themselves are
//! tool-independent; the same graph compiles for either.
//!
//! Output nodes decide where blocks go:
//! - instructions: a managed section holding the connected blocks in the
//!   node's order, in CLAUDE.md (team) / CLAUDE.local.md (personal), or for
//!   Codex in AGENTS.md (team only: Codex has no personal instructions file,
//!   and the team shares AGENTS.md). Codex has no rule files, so there
//!   instructions also take rule blocks.
//! - rules: one `.claude/rules/<name>.md` per block (Claude Code only)
//! - skills: `<skills dir>/<name>/SKILL.md` (`.claude/skills`, `.agents/skills`)
//! - agents: `.claude/agents/<name>.md`, or Codex `.codex/agents/<name>.toml`
//! - custom: a managed section in a file at a path the user types, holding
//!   the user's own text and the connected instruction and rule blocks
//! - hooks: the `hooks` of `.claude/settings.json` (or `settings.local.json`),
//!   or Codex `.codex/hooks.json` (team only), merged into the file by `write`
//! - plans: a managed section in `PLANS.md` (`PLANS.local.md`) holding the
//!   connected plan nodes (`plans`), and a line in the instructions file
//!   pointing to it (for Codex always the team AGENTS.md)

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::catalog::{Block, BlockKind, normalize};
use super::plans::{self, Plan, PlanItem};
use crate::library::{Mode, check_id};
use crate::tools::{ToolId, frontmatter};

/// CLAUDE.md guidance: longer files are followed less reliably.
pub const CLAUDE_MD_LINE_BUDGET: usize = 200;

/// Codex reads at most this much project AGENTS.md text in total
/// (`project_doc_max_bytes`) and cuts the rest.
pub const AGENTS_MD_BYTE_LIMIT: usize = 32 * 1024;

/// Warn a little before the limit: the user's own text around the section and
/// AGENTS.md files in subfolders count too.
pub const AGENTS_MD_BYTE_BUDGET: usize = 28 * 1024;

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

/// Codex hook events (`codex-rs` hooks, 2026-10; docs/research.md §13).
pub const CODEX_HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "Interrupt",
];

/// The hook events `tool` knows.
pub fn hook_events(tool: ToolId) -> &'static [&'static str] {
    match tool {
        ToolId::Claude => HOOK_EVENTS,
        ToolId::Codex => CODEX_HOOK_EVENTS,
    }
}

/// Product name for messages; not translated.
pub fn tool_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Claude => "Claude Code",
        ToolId::Codex => "Codex",
    }
}

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
    /// The always-loaded instructions file (CLAUDE.md, AGENTS.md). Saved as
    /// `claudeMd` before other tools were supported.
    #[serde(alias = "claudeMd")]
    Instructions,
    Rules,
    Skills,
    Agents,
    Custom,
    Hooks,
    Plans,
    /// Not a file: instructions, skills and subagents for an automation the
    /// app runs on a schedule (`automation_parts`).
    Automation,
}

impl Target {
    /// Whether the output exists for `tool` (Codex has no rule files).
    pub fn available(self, tool: ToolId) -> bool {
        self != Target::Rules || tool.has_rules()
    }

    pub fn accepts(self, tool: ToolId, kind: BlockKind) -> bool {
        match self {
            // Without rule files, rules become always-on instructions.
            Target::Instructions => {
                kind == BlockKind::Instruction || (!tool.has_rules() && kind == BlockKind::Rule)
            }
            Target::Rules => {
                tool.has_rules() && matches!(kind, BlockKind::Rule | BlockKind::Instruction)
            }
            Target::Custom => matches!(kind, BlockKind::Rule | BlockKind::Instruction),
            Target::Skills => kind == BlockKind::Skill,
            Target::Agents => kind == BlockKind::Agent,
            Target::Hooks => kind == BlockKind::Hook,
            // Plan nodes, not catalog blocks.
            Target::Plans => false,
            Target::Automation => !matches!(kind, BlockKind::Hook),
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

/// The text of section outputs (instructions, custom): blocks in order,
/// third-party ones credited in an HTML comment (which Claude Code strips),
/// or with `plain_credit` in an italic line (Codex keeps HTML comments, so
/// there a comment would cost as much context and read worse).
fn section_text(resolved: &[(&str, &Block, String)], plain_credit: bool) -> String {
    resolved
        .iter()
        .map(|(_, block, body)| match &block.origin {
            Some(o) if plain_credit => format!(
                "*Source: {} ({}, {})*\n\n{body}",
                o.url(),
                o.license,
                o.copyright
            ),
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
    /// The tool the files are laid out for.
    pub tool: ToolId,
    pub files: Vec<OutFile>,
    pub warnings: Vec<Warning>,
}

/// What an automation output's connected blocks give its runs: instructions,
/// rules and skills as added system prompt, subagents for `--agents`.
#[derive(Debug, Default, PartialEq)]
pub struct AutomationParts {
    pub instructions: String,
    pub agents: std::collections::BTreeMap<String, crate::automation::AgentDef>,
}

pub fn automation_parts(doc: &Doc, catalog: &[Block], output_id: &str) -> AutomationParts {
    let mut parts = AutomationParts::default();
    let Some(output) = doc.nodes.iter().find(|n| n.id == output_id) else {
        return parts;
    };
    let mut sections: Vec<String> = Vec::new();
    for node_id in inputs_of(doc, output) {
        let Some(node) = doc.nodes.iter().find(|n| n.id == node_id) else {
            continue;
        };
        if node.data.disabled {
            continue;
        }
        let Some(block) = node
            .data
            .block_id
            .as_deref()
            .and_then(|id| catalog.iter().find(|b| b.id == id))
        else {
            continue;
        };
        let edited = node.data.body.as_deref().map(normalize);
        let body = edited.as_deref().unwrap_or(&block.body).trim().to_string();
        match block.kind {
            BlockKind::Instruction | BlockKind::Rule => {
                sections.push(format!("## {}\n\n{body}", block.display_title()));
            }
            BlockKind::Skill => {
                let about = block.description.as_deref().unwrap_or(&block.summary);
                sections.push(format!("## Skill: {}\n\n{about}\n\n{body}", block.name));
            }
            BlockKind::Agent => {
                let model = node
                    .data
                    .model
                    .as_deref()
                    .filter(|m| !m.is_empty())
                    .or(block.model.as_deref())
                    .filter(|m| *m != "inherit")
                    .unwrap_or_default();
                let tools = block
                    .tools
                    .as_deref()
                    .map(|t| {
                        t.split(',')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                parts.agents.insert(
                    block.name.clone(),
                    crate::automation::AgentDef {
                        description: block
                            .description
                            .clone()
                            .unwrap_or_else(|| block.summary.clone()),
                        prompt: body,
                        model: model.to_string(),
                        tools,
                    },
                );
            }
            BlockKind::Hook => {}
        }
    }
    parts.instructions = sections.join("\n\n");
    parts
}

pub fn compile(doc: &Doc, catalog: &[Block], tool: ToolId) -> Compiled {
    let blocks: HashMap<&str, &Block> = catalog.iter().map(|b| (b.id.as_str(), b)).collect();
    let nodes: HashMap<&str, &Node> = doc.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = Compiled {
        tool,
        ..Compiled::default()
    };
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
        if matches!(target, Target::Plans | Target::Automation) {
            continue;
        }
        if !target.available(tool) {
            warn(
                &mut out,
                &output.id,
                crate::tr!(
                    "{}에는 규칙 파일이 없어서 이 출력은 아무것도 쓰지 않아요. 블록을 지침({})에 연결해 주세요",
                    "{} has no rule files, so this output writes nothing. Connect the blocks to the instructions ({}) instead",
                    tool_name(tool),
                    tool.instructions_file(false)
                ),
            );
            continue;
        }

        let inputs = inputs_of(doc, output);

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
            if !target.accepts(tool, block.kind) {
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
            // Only Claude Code reads `@path` as an import.
            if tool == ToolId::Claude && has_import_like(body) {
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
            if target == Target::Instructions && !block.paths.is_empty() {
                warn(
                    &mut out,
                    node_id,
                    crate::tr!(
                        "{}에는 경로별 지침이 없어서 '{}'은(는) 항상 적용돼요",
                        "{} has no path-scoped instructions, so '{}' always applies",
                        tool_name(tool),
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
            Target::Instructions if tool == ToolId::Codex && personal => {
                // Codex reads only AGENTS.md, which the team shares; keeping it
                // out of git would hide the team's file too.
                warn(
                    &mut out,
                    &output.id,
                    crate::tr!(
                        "Codex에는 개인용 지침 파일이 없어서 '나만' AGENTS.md는 쓰지 않아요(AGENTS.md는 팀과 같이 쓰는 파일이에요). '팀'으로 바꾸거나 ~/.codex/AGENTS.md에 직접 넣어 주세요",
                        "Codex has no personal instructions file, so a 'Just me' AGENTS.md output writes nothing (AGENTS.md is shared with the team). Switch it to 'Team', or add the text to ~/.codex/AGENTS.md yourself"
                    ),
                );
            }
            Target::Instructions => {
                let content = section_text(&resolved, tool == ToolId::Codex);
                let lines = content.lines().count();
                let bytes = content.len();
                if tool == ToolId::Codex && bytes > AGENTS_MD_BYTE_BUDGET {
                    let kib = format!("{:.1}", bytes as f64 / 1024.0);
                    let limit = AGENTS_MD_BYTE_LIMIT / 1024;
                    warn(
                        &mut out,
                        &output.id,
                        crate::tr!(
                            "{kib} KiB예요. Codex는 프로젝트의 AGENTS.md를 모두 합쳐 {limit} KiB까지만 읽고 나머지는 잘라요. 일부를 skill로 옮겨 보세요",
                            "{kib} KiB. Codex reads at most {limit} KiB of a project's AGENTS.md files in total and cuts the rest. Try moving some of it into skills"
                        ),
                    );
                }
                if tool == ToolId::Claude && lines > CLAUDE_MD_LINE_BUDGET {
                    warn(
                        &mut out,
                        &output.id,
                        crate::tr!(
                            "{lines}줄이에요. CLAUDE.md는 {CLAUDE_MD_LINE_BUDGET}줄 이하일 때 더 잘 지켜져요. 일부를 규칙이나 skill로 옮겨 보세요",
                            "{lines} lines. CLAUDE.md is followed better at {CLAUDE_MD_LINE_BUDGET} lines or fewer. Try moving some of it into rules or skills"
                        ),
                    );
                }
                files.push(OutFile {
                    path: tool.instructions_file(personal).into(),
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
                    let blocks = section_text(&resolved, false);
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
            Target::Hooks if tool == ToolId::Codex && personal => {
                // Codex has no personal hooks file in a project; a git-excluded
                // `.codex/hooks.json` would clash with the team's one.
                warn(
                    &mut out,
                    &output.id,
                    crate::tr!(
                        "Codex에는 프로젝트의 개인용 hook 파일이 없어서 '나만' HOOKS는 쓰지 않아요. '팀'으로 바꾸거나 ~/.codex/hooks.json에 직접 넣어 주세요",
                        "Codex has no personal hooks file in a project, so a 'Just me' HOOKS output writes nothing. Switch it to 'Team', or add the hooks to ~/.codex/hooks.json yourself"
                    ),
                );
            }
            Target::Hooks => {
                let events = hook_events(tool);
                let mut used = Vec::new();
                let mut hooks = serde_json::Map::new();
                for (node_id, block, body) in &resolved {
                    let data = nodes.get(node_id).map(|n| &n.data);
                    let event = data
                        .and_then(|d| d.event.as_deref())
                        .or(block.event.as_deref())
                        .unwrap_or("")
                        .trim();
                    if !events.contains(&event) && HOOK_EVENTS.contains(&event) {
                        warn(
                            &mut out,
                            node_id,
                            crate::tr!(
                                "'{}'의 {} 이벤트는 {}에 없어요. 다른 이벤트를 골라 주세요",
                                "'{}': {} has no {} event. Pick another event",
                                block.display_title(),
                                tool_name(tool),
                                event
                            ),
                        );
                        continue;
                    }
                    if !events.contains(&event) {
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
                    if tool == ToolId::Codex {
                        codex_hook_contract(&mut out, node_id, block, event);
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
                    let path = match (tool, personal) {
                        (ToolId::Codex, _) => ".codex/hooks.json",
                        (ToolId::Claude, true) => ".claude/settings.local.json",
                        (ToolId::Claude, false) => ".claude/settings.json",
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
            Target::Plans | Target::Automation => {}
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
                        Target::Skills => {
                            if tool == ToolId::Codex && skill_is_manual(block) {
                                warn(
                                    &mut out,
                                    node_id,
                                    crate::tr!(
                                        "'{}'은(는) 직접 불러야 쓰는 skill이지만 Codex에는 그 설정이 없어서, Codex가 알아서 쓸 수도 있어요",
                                        "'{}' is a skill you call yourself, but Codex has no such setting, so Codex may use it on its own",
                                        block.display_title()
                                    ),
                                );
                            }
                            (
                                format!("{}/{}/SKILL.md", tool.skills_dir(), block.name),
                                skill_file(block, body, tool),
                            )
                        }
                        _ if tool.agents_are_toml() => {
                            // A model picked on the canvas is Claude Code's (the
                            // picker is hidden for Codex), so it's left out silently.
                            let (content, dropped) = agent_toml(block, body);
                            if !dropped.is_empty() {
                                warn(
                                    &mut out,
                                    node_id,
                                    crate::tr!(
                                        "'{}'의 {} 설정은 Codex subagent에 없어서 빠져요",
                                        "'{}': {} isn't supported by Codex subagents, so it's left out",
                                        block.display_title(),
                                        dropped.join(", ")
                                    ),
                                );
                            }
                            (tool.agent_file(&block.name), content)
                        }
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
                                tool.agent_file(&block.name),
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
                                path: format!("{}/{}/{rel}", tool.skills_dir(), block.name),
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
    compile_plans(doc, &nodes, &blocks, tool, &mut out);
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

/// PLANS outputs, and the line pointing the instructions file to them.
fn compile_plans(
    doc: &Doc,
    nodes: &HashMap<&str, &Node>,
    blocks: &HashMap<&str, &Block>,
    tool: ToolId,
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

        // Neither tool reads PLANS.md by itself: the instructions file points
        // to it. Codex has only the team's AGENTS.md, which takes the pointer
        // whatever the plan's scope.
        let pointer = plans::pointer(path);
        let in_personal = personal && tool != ToolId::Codex;
        let claude_md = tool.instructions_file(in_personal);
        if let Some(file) = out.files.iter_mut().find(|f| {
            f.target == Target::Instructions && f.path == claude_md && f.personal == in_personal
        }) {
            file.content = format!("{}\n\n{pointer}", file.content);
            file.lines = file.content.lines().count();
        } else if let Some(node) = doc.nodes.iter().find(|n| {
            n.kind == "output"
                && n.data.target == Some(Target::Instructions)
                && (n.data.scope == Some(Mode::Personal)) == in_personal
        }) && !out.files.iter().any(|f| f.path == claude_md)
        {
            out.files.push(OutFile {
                path: claude_md.into(),
                lines: pointer.lines().count(),
                content: pointer,
                section: true,
                personal: in_personal,
                target: Target::Instructions,
                node: node.id.clone(),
                blocks: Vec::new(),
            });
        } else {
            out.warnings.push(Warning {
                node: Some(output.id.clone()),
                message: crate::tr!(
                    "{}는 {path}를 스스로 읽지 않아요. {claude_md} 출력({})을 추가하면 계획을 읽으라는 안내가 들어가요",
                    "{} doesn't read {path} by itself. Add a {claude_md} output ({}) and it gets a line pointing to the plan",
                    tool_name(tool),
                    if in_personal { crate::tr!("나만", "Just me") } else { crate::tr!("팀", "Team") }
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

/// Whether a top-level `key:` line is in raw frontmatter (without `---`).
fn has_key(raw: &str, key: &str) -> bool {
    raw.lines().any(|l| {
        l.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

/// Whether a skill is meant to be called only by the user
/// (`disable-model-invocation`), which Codex has no setting for.
fn skill_is_manual(block: &Block) -> bool {
    match &block.frontmatter {
        Some(raw) => frontmatter::parse(&format!("---\n{raw}\n---\n"))
            .and_then(|f| f.get("disable-model-invocation").cloned())
            .is_some_and(|v| v.trim() == "true"),
        None => block.manual,
    }
}

/// Warnings for a hook written to Codex, whose hooks follow a stricter
/// output contract than Claude Code's (docs/research.md §13).
fn codex_hook_contract(out: &mut Compiled, node_id: &str, block: &Block, event: &str) {
    let message = match event {
        "Stop" => crate::tr!(
            "'{}': Codex의 Stop hook은 stdout에 JSON 객체를 출력해야 해요. 일반 텍스트를 출력하는 명령이면 Codex에서 hook 오류가 나요",
            "'{}': Codex Stop hooks must print a JSON object on stdout. A command that prints plain text makes the hook fail in Codex",
            block.display_title()
        ),
        "PreToolUse" => crate::tr!(
            "'{}': Codex의 PreToolUse hook은 막기(permissionDecision \"deny\", 이유 필수)만 지원하고 \"ask\" 같은 다른 결정은 거부해요",
            "'{}': Codex PreToolUse hooks only support blocking (permissionDecision \"deny\" with a reason) and reject other decisions such as \"ask\"",
            block.display_title()
        ),
        _ => return,
    };
    out.warnings.push(Warning {
        node: Some(node_id.to_string()),
        message,
    });
}

fn skill_file(block: &Block, body: &str, tool: ToolId) -> String {
    if let Some(raw) = &block.frontmatter {
        // Claude Code falls back to the body without a description; Codex
        // skips such a skill, so `name` and `description` are added for it.
        let mut fm = String::new();
        if tool == ToolId::Codex {
            if !has_key(raw, "name") {
                fm.push_str(&format!("name: {}\n", block.name));
            }
            if !has_key(raw, "description") {
                let description = block
                    .description
                    .as_deref()
                    .filter(|d| !d.trim().is_empty())
                    .unwrap_or(&block.summary);
                fm.push_str(&format!("description: {}\n", quoted(description)));
            }
        }
        return format!("---\n{fm}{raw}\n---\n\n{body}\n");
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

/// A Codex subagent (`.codex/agents/<name>.toml`): `name`, `description` and
/// `developer_instructions` (the block body), with third-party credit as TOML
/// comments. Also returns the Claude Code fields that have no Codex
/// counterpart and were left out.
fn agent_toml(block: &Block, body: &str) -> (String, Vec<String>) {
    let fields = block
        .frontmatter
        .as_ref()
        .and_then(|raw| frontmatter::parse(&format!("---\n{raw}\n---\n")))
        .unwrap_or_default();
    let name = fields
        .get("name")
        .filter(|n| !n.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| block.name.clone());
    let description = fields
        .get("description")
        .cloned()
        .or_else(|| block.description.clone())
        .unwrap_or_else(|| block.summary.clone());

    // Library agents: every other frontmatter field is Claude Code's.
    let mut dropped: Vec<String> = fields
        .iter()
        .filter(|(k, v)| !matches!(k.as_str(), "name" | "description") && !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    for (key, value) in [("tools", &block.tools), ("model", &block.model)] {
        if value.as_deref().is_some_and(|v| !v.is_empty()) && !dropped.iter().any(|d| d == key) {
            dropped.push(key.to_string());
        }
    }

    let mut doc = toml_edit::DocumentMut::new();
    doc["name"] = toml_edit::value(name);
    doc["description"] = toml_edit::value(description);
    doc["developer_instructions"] = toml_edit::value(format!("{}\n", body.trim_end()));
    let credit = block
        .origin
        .as_ref()
        .map(|o| {
            format!(
                "# Source: {}\n# License: {}, {}\n",
                o.url(),
                o.license,
                o.copyright
            )
        })
        .unwrap_or_default();
    (format!("{credit}{doc}"), dropped)
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
            rating: None,
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let f = &compile(&d, &catalog(), ToolId::Claude).files[0];
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
        let out = compile(&d, &[skill], ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
            skill_file(&skill, "Steps", ToolId::Claude),
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        let out = compile(&d, &[fmt, stop, blank], ToolId::Claude);
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
        let out = compile(&d, &catalog(), ToolId::Claude);
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
        assert_eq!(
            compile(&d, &catalog(), ToolId::Claude).files[0].content,
            "One\nTwo"
        );
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

    #[test]
    fn old_claude_md_targets_still_load() {
        let d =
            doc(r#"{"nodes": [{"id": "o", "type": "output", "data": {"target": "claudeMd"}}]}"#);
        assert_eq!(d.nodes[0].data.target, Some(Target::Instructions));
        let d = doc(
            r#"{"nodes": [{"id": "o", "type": "output", "data": {"target": "instructions"}}]}"#,
        );
        assert_eq!(d.nodes[0].data.target, Some(Target::Instructions));
        assert_eq!(
            serde_json::to_string(&Target::Instructions).unwrap(),
            "\"instructions\""
        );
    }

    fn all_outputs_doc(scope: &str) -> Doc {
        doc(&format!(
            r#"{{
            "nodes": [
                {{"id": "i", "type": "block", "data": {{"blockId": "builtin:a"}}}},
                {{"id": "s", "type": "block", "data": {{"blockId": "builtin:review"}}}},
                {{"id": "g", "type": "block", "data": {{"blockId": "builtin:reviewer"}}}},
                {{"id": "I", "type": "output", "data": {{"target": "instructions", "scope": "{scope}"}}}},
                {{"id": "S", "type": "output", "data": {{"target": "skills"}}}},
                {{"id": "G", "type": "output", "data": {{"target": "agents"}}}}
            ],
            "edges": [
                {{"source": "i", "target": "I"}}, {{"source": "s", "target": "S"}},
                {{"source": "g", "target": "G"}}
            ]
        }}"#
        ))
    }

    fn paths(out: &Compiled) -> Vec<&str> {
        out.files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn paths_follow_the_tool() {
        let claude = compile(&all_outputs_doc("team"), &catalog(), ToolId::Claude);
        assert_eq!(
            paths(&claude),
            [
                "CLAUDE.md",
                ".claude/skills/review/SKILL.md",
                ".claude/agents/reviewer.md"
            ]
        );
        assert_eq!(claude.tool, ToolId::Claude);
        let codex = compile(&all_outputs_doc("team"), &catalog(), ToolId::Codex);
        assert_eq!(
            paths(&codex),
            [
                "AGENTS.md",
                ".agents/skills/review/SKILL.md",
                ".codex/agents/reviewer.toml"
            ]
        );
        assert_eq!(codex.tool, ToolId::Codex);
        // The skill file itself is the same for both tools.
        assert_eq!(claude.files[1].content, codex.files[1].content);

        let personal = compile(&all_outputs_doc("personal"), &catalog(), ToolId::Claude);
        assert_eq!(personal.files[0].path, "CLAUDE.local.md");
        // Codex has no personal instructions file: a warning, nothing written.
        let personal = compile(&all_outputs_doc("personal"), &catalog(), ToolId::Codex);
        assert_eq!(
            paths(&personal),
            [
                ".agents/skills/review/SKILL.md",
                ".codex/agents/reviewer.toml"
            ]
        );
        // (and the subagent's `tools`, which Codex has no field for)
        assert_eq!(personal.warnings.len(), 2, "{:?}", personal.warnings);
        assert_eq!(personal.warnings[0].node.as_deref(), Some("I"));
        assert!(personal.warnings[0].message.contains("개인용 지침 파일"));
    }

    #[test]
    fn codex_keeps_team_instructions_when_a_personal_output_comes_first() {
        // A document made for Claude Code: CLAUDE.local.md before CLAUDE.md.
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "y", "type": "block", "data": {"blockId": "builtin:b"}},
                {"id": "P", "type": "output", "data": {"target": "instructions", "scope": "personal"}},
                {"id": "T", "type": "output", "data": {"target": "instructions", "scope": "team"}}
            ],
            "edges": [{"source": "x", "target": "P"}, {"source": "y", "target": "T"}]
        }"#);
        let out = compile(&d, &catalog(), ToolId::Codex);
        assert_eq!(paths(&out), ["AGENTS.md"]);
        assert_eq!(out.files[0].content, "## B");
        assert!(!out.files[0].personal);
        assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
        assert_eq!(out.warnings[0].node.as_deref(), Some("P"));
        assert!(!out.warnings[0].message.contains("두 출력이"));

        let claude = compile(&d, &catalog(), ToolId::Claude);
        assert_eq!(paths(&claude), ["CLAUDE.local.md", "CLAUDE.md"]);
        assert!(claude.warnings.is_empty());
    }

    #[test]
    fn codex_skill_companions_follow_the_skills_dir() {
        let mut skill = block("builtin:guide", BlockKind::Skill, "See references/a.md");
        skill.companions = vec![("references/a.md".into(), "# A\n")];
        let d = doc(r#"{
            "nodes": [
                {"id": "s", "type": "block", "data": {"blockId": "builtin:guide"}},
                {"id": "S", "type": "output", "data": {"target": "skills"}}
            ],
            "edges": [{"source": "s", "target": "S"}]
        }"#);
        let out = compile(&d, &[skill], ToolId::Codex);
        assert_eq!(
            paths(&out),
            [
                ".agents/skills/guide/SKILL.md",
                ".agents/skills/guide/references/a.md"
            ]
        );
    }

    #[test]
    fn codex_instructions_take_rules_and_warn_about_paths() {
        let d = doc(r#"{
            "nodes": [
                {"id": "i", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "r", "type": "block", "data": {"blockId": "builtin:ts"}},
                {"id": "o", "type": "output", "data": {"target": "instructions"}}
            ],
            "edges": [{"source": "i", "target": "o"}, {"source": "r", "target": "o"}]
        }"#);
        let out = compile(&d, &catalog(), ToolId::Codex);
        assert_eq!(out.files[0].content, "## A\n- one\n\n# TS");
        assert_eq!(out.files[0].blocks, ["i", "r"]);
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.warnings[0].node.as_deref(), Some("r"));
        assert!(out.warnings[0].message.contains("항상 적용"));

        // Claude Code keeps rules out of CLAUDE.md.
        let out = compile(&d, &catalog(), ToolId::Claude);
        assert_eq!(out.files[0].content, "## A\n- one");
        assert!(out.warnings[0].message.contains("연결할 수 없어요"));
    }

    #[test]
    fn codex_has_no_rules_output() {
        let d = doc(r#"{
            "nodes": [
                {"id": "r", "type": "block", "data": {"blockId": "builtin:ts"}},
                {"id": "R", "type": "output", "data": {"target": "rules"}}
            ],
            "edges": [{"source": "r", "target": "R"}]
        }"#);
        let out = compile(&d, &catalog(), ToolId::Codex);
        assert!(out.files.is_empty());
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.warnings[0].node.as_deref(), Some("R"));
        assert!(out.warnings[0].message.contains("규칙 파일이 없어서"));
        assert!(out.warnings[0].message.contains("AGENTS.md"));
        assert_eq!(compile(&d, &catalog(), ToolId::Claude).files.len(), 1);
    }

    #[test]
    fn budgets_and_import_checks_depend_on_the_tool() {
        let instructions = |body: String| {
            doc(&format!(
                r#"{{
                "nodes": [
                    {{"id": "x", "type": "block", "data": {{"blockId": "builtin:a", "body": {body:?}}}}},
                    {{"id": "o", "type": "output", "data": {{"target": "instructions"}}}}
                ],
                "edges": [{{"source": "x", "target": "o"}}]
            }}"#
            ))
        };
        let messages = |out: &Compiled| -> Vec<String> {
            out.warnings.iter().map(|w| w.message.clone()).collect()
        };

        // 201 short lines: too long for CLAUDE.md, fine for AGENTS.md.
        let lines = instructions(
            (0..=CLAUDE_MD_LINE_BUDGET)
                .map(|i| format!("- {i}\n"))
                .collect(),
        );
        assert!(messages(&compile(&lines, &catalog(), ToolId::Claude))[0].contains("줄이에요"));
        assert!(
            compile(&lines, &catalog(), ToolId::Codex)
                .warnings
                .is_empty()
        );

        // A few long lines: over Codex's byte budget, under CLAUDE.md's line budget.
        let big = instructions(format!("{}\n", "x".repeat(1000)).repeat(30));
        let codex = compile(&big, &catalog(), ToolId::Codex);
        assert!(codex.files[0].content.len() > AGENTS_MD_BYTE_BUDGET);
        let m = messages(&codex);
        assert_eq!(m.len(), 1);
        assert!(m[0].contains("KiB") && m[0].contains("32"), "{}", m[0]);
        assert!(
            compile(&big, &catalog(), ToolId::Claude)
                .warnings
                .is_empty()
        );

        // `@path` is an import only for Claude Code.
        let at = instructions("See @docs/api.md".into());
        assert!(messages(&compile(&at, &catalog(), ToolId::Claude))[0].contains("@경로"));
        assert!(compile(&at, &catalog(), ToolId::Codex).warnings.is_empty());
    }

    fn origin(path: &str, c: char) -> super::super::catalog::Origin {
        super::super::catalog::Origin {
            repo: "o/r".into(),
            commit: c.to_string().repeat(40),
            path: path.into(),
            license: "MIT".into(),
            copyright: "Copyright (c) 2025 X".into(),
        }
    }

    #[test]
    fn codex_credits_third_party_text_without_html_comments() {
        let mut ext = block("external:x", BlockKind::Instruction, "## X");
        ext.origin = Some(origin("x.md", 'a'));
        let d = doc(r#"{
            "nodes": [
                {"id": "x", "type": "block", "data": {"blockId": "external:x"}},
                {"id": "o", "type": "output", "data": {"target": "instructions"}}
            ],
            "edges": [{"source": "x", "target": "o"}]
        }"#);
        let url = format!("https://github.com/o/r/blob/{}/x.md", "a".repeat(40));
        let codex = compile(&d, std::slice::from_ref(&ext), ToolId::Codex);
        assert_eq!(
            codex.files[0].content,
            format!("*Source: {url} (MIT, Copyright (c) 2025 X)*\n\n## X")
        );
        let claude = compile(&d, &[ext], ToolId::Claude);
        assert!(claude.files[0].content.starts_with("<!-- Source: "));
    }

    fn parse_toml(text: &str) -> toml_edit::DocumentMut {
        text.parse().unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    #[test]
    fn codex_agents_are_toml_with_escaped_instructions() {
        let body = "You review.\nSay \"\"\" and ''' and \\ freely.\n\tTabbed \"quote\"";
        let mut agent = block("external:reviewer", BlockKind::Agent, body);
        agent.description = Some("Reviews \"code\"\nline two".into());
        agent.model = Some("sonnet".into());
        agent.origin = Some(origin("agents/reviewer.md", 'b'));
        let d = doc(r#"{
            "nodes": [
                {"id": "g", "type": "block", "data": {"blockId": "external:reviewer"}},
                {"id": "G", "type": "output", "data": {"target": "agents"}}
            ],
            "edges": [{"source": "g", "target": "G"}]
        }"#);
        let out = compile(&d, &[agent], ToolId::Codex);
        let file = &out.files[0];
        assert_eq!(file.path, ".codex/agents/reviewer.toml");
        assert!(
            file.content.starts_with(&format!(
                "# Source: https://github.com/o/r/blob/{}/agents/reviewer.md\n# License: MIT, Copyright (c) 2025 X\n",
                "b".repeat(40)
            )),
            "{}",
            file.content
        );
        let toml = parse_toml(&file.content);
        assert_eq!(toml["name"].as_str(), Some("reviewer"));
        assert_eq!(
            toml["description"].as_str(),
            Some("Reviews \"code\"\nline two")
        );
        assert_eq!(
            toml["developer_instructions"].as_str(),
            Some(format!("{body}\n").as_str())
        );
        // Claude Code's `tools` and `model` have no Codex field.
        assert_eq!(out.warnings.len(), 1);
        let m = &out.warnings[0].message;
        assert!(m.contains("tools") && m.contains("model"), "{m}");
    }

    #[test]
    fn toml_instructions_survive_triple_quotes_at_the_end() {
        for body in [
            "ends with \"\"\"",
            "ends with '''",
            "\"\"\"\"\"\"",
            "a\\\"\"\"b",
        ] {
            let agent = block("builtin:q", BlockKind::Agent, body);
            let (text, _) = agent_toml(&agent, body);
            let toml = parse_toml(&text);
            assert_eq!(
                toml["developer_instructions"].as_str(),
                Some(format!("{body}\n").as_str()),
                "{text}"
            );
        }
    }

    #[test]
    fn library_agents_for_codex_keep_their_name_and_drop_claude_fields() {
        let mut agent = block("library:agent:reviewer", BlockKind::Agent, "Review.");
        agent.tools = None;
        agent.frontmatter =
            Some("name: code-reviewer\ndescription: Reviews diffs\npermissionMode: plan".into());
        let (text, dropped) = agent_toml(&agent, "Review.");
        let toml = parse_toml(&text);
        assert_eq!(toml["name"].as_str(), Some("code-reviewer"));
        assert_eq!(toml["description"].as_str(), Some("Reviews diffs"));
        assert_eq!(toml["developer_instructions"].as_str(), Some("Review.\n"));
        assert_eq!(dropped, ["permissionMode"]);
        assert!(!text.contains("permissionMode"));

        // Without frontmatter fields, the block's own name and description.
        let mut plain = block("builtin:helper", BlockKind::Agent, "Help.");
        plain.tools = None;
        let (text, dropped) = agent_toml(&plain, "Help.");
        let toml = parse_toml(&text);
        assert_eq!(toml["name"].as_str(), Some("helper"));
        assert_eq!(toml["description"].as_str(), Some("Does: things"));
        assert!(dropped.is_empty());
    }

    #[test]
    fn codex_hooks_go_to_hooks_json_with_codex_events_only() {
        let mut fmt = block("builtin:fmt", BlockKind::Hook, "pnpm format");
        fmt.event = Some("PostToolUse".into());
        fmt.matcher = Some("apply_patch".into());
        let blank = block("builtin:blank", BlockKind::Hook, "");
        let other = block("builtin:other", BlockKind::Hook, "");
        let d = doc(r#"{
            "nodes": [
                {"id": "a", "type": "block", "data": {"blockId": "builtin:fmt"}},
                {"id": "b", "type": "block", "data": {"blockId": "builtin:blank", "event": "Interrupt", "body": "echo stop"}},
                {"id": "c", "type": "block", "data": {"blockId": "builtin:other", "event": "Notification", "body": "x"}},
                {"id": "o", "type": "output", "data": {"target": "hooks", "scope": "team"}}
            ],
            "edges": [{"source": "a", "target": "o"}, {"source": "b", "target": "o"}, {"source": "c", "target": "o"}]
        }"#);
        let out = compile(
            &d,
            &[fmt.clone(), blank.clone(), other.clone()],
            ToolId::Codex,
        );
        let f = &out.files[0];
        assert_eq!(f.path, ".codex/hooks.json");
        assert!(!f.personal);
        assert_eq!(f.blocks, ["a", "b"]);
        let v: serde_json::Value = serde_json::from_str(&f.content).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"hooks": {
                "PostToolUse": [{"matcher": "apply_patch", "hooks": [{"type": "command", "command": "pnpm format"}]}],
                "Interrupt": [{"hooks": [{"type": "command", "command": "echo stop"}]}]
            }})
        );
        // "c": a Claude Code-only event.
        assert!(
            out.warnings
                .iter()
                .any(|w| w.node.as_deref() == Some("c") && w.message.contains("Notification")),
            "{:?}",
            out.warnings
        );

        // Claude Code doesn't know Codex's Interrupt.
        let claude = compile(&d, &[fmt.clone(), blank, other], ToolId::Claude);
        assert_eq!(claude.files[0].path, ".claude/settings.json");
        assert!(!claude.files[0].content.contains("Interrupt"));

        // No personal hooks file for Codex: a warning, nothing written.
        let d = doc(r#"{
            "nodes": [
                {"id": "a", "type": "block", "data": {"blockId": "builtin:fmt"}},
                {"id": "o", "type": "output", "data": {"target": "hooks", "scope": "personal"}}
            ],
            "edges": [{"source": "a", "target": "o"}]
        }"#);
        let out = compile(&d, &[fmt], ToolId::Codex);
        assert!(out.files.is_empty());
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.warnings[0].node.as_deref(), Some("o"));
    }

    #[test]
    fn codex_plans_point_from_agents_md() {
        let d = doc(r#"{
            "nodes": [
                {"id": "i", "type": "block", "data": {"blockId": "builtin:a"}},
                {"id": "c", "type": "output", "data": {"target": "instructions", "scope": "team"}},
                {"id": "g", "type": "plan", "data": {"title": "Goal"}},
                {"id": "p", "type": "output", "data": {"target": "plans", "scope": "team"}}
            ],
            "edges": [{"source": "i", "target": "c"}, {"source": "g", "target": "p"}]
        }"#);
        let out = compile(&d, &catalog(), ToolId::Codex);
        assert_eq!(paths(&out), ["AGENTS.md", "PLANS.md"]);
        assert!(out.files[0].content.starts_with("## A\n- one\n\n"));
        assert!(out.files[0].content.contains("`PLANS.md`"));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);

        // A personal plan: Codex has only the team AGENTS.md, which points to it.
        let personal_plan = |instructions: &str| {
            doc(&format!(
                r#"{{
                "nodes": [
                    {{"id": "i", "type": "block", "data": {{"blockId": "builtin:a"}}}},
                    {{"id": "c", "type": "output", "data": {{"target": "instructions", "scope": "{instructions}"}}}},
                    {{"id": "g", "type": "plan", "data": {{"title": "Goal"}}}},
                    {{"id": "p", "type": "output", "data": {{"target": "plans", "scope": "personal"}}}}
                ],
                "edges": [{{"source": "i", "target": "c"}}, {{"source": "g", "target": "p"}}]
            }}"#
            ))
        };
        let out = compile(&personal_plan("team"), &catalog(), ToolId::Codex);
        assert_eq!(paths(&out), ["AGENTS.md", "PLANS.local.md"]);
        assert!(!out.files[0].personal);
        assert!(out.files[0].content.contains("`PLANS.local.md`"));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);

        // Claude Code keeps it in the matching scope: no CLAUDE.local.md output.
        let out = compile(&personal_plan("team"), &catalog(), ToolId::Claude);
        assert!(!out.files[0].content.contains("PLANS"));
        assert_eq!(out.warnings.len(), 1);

        // Only a personal AGENTS.md output: it writes nothing, and the plan
        // asks for a team one (which can coexist with nothing else).
        let out = compile(&personal_plan("personal"), &catalog(), ToolId::Codex);
        assert_eq!(paths(&out), ["PLANS.local.md"]);
        assert_eq!(out.warnings.len(), 2, "{:?}", out.warnings);
        let plan_warning = &out.warnings[1];
        assert_eq!(plan_warning.node.as_deref(), Some("p"));
        assert!(plan_warning.message.contains("AGENTS.md"));
        assert!(
            plan_warning.message.contains("(팀)"),
            "{}",
            plan_warning.message
        );
    }

    #[test]
    fn codex_stop_and_pre_tool_use_hooks_warn_about_the_output_contract() {
        let mut stop = block("builtin:stop", BlockKind::Hook, "npm run lint --if-present");
        stop.event = Some("Stop".into());
        let mut pre = block("builtin:pre", BlockKind::Hook, "./check.sh");
        pre.event = Some("PreToolUse".into());
        let mut post = block("builtin:post", BlockKind::Hook, "pnpm format");
        post.event = Some("PostToolUse".into());
        let d = doc(r#"{
            "nodes": [
                {"id": "s", "type": "block", "data": {"blockId": "builtin:stop"}},
                {"id": "p", "type": "block", "data": {"blockId": "builtin:pre"}},
                {"id": "q", "type": "block", "data": {"blockId": "builtin:post"}},
                {"id": "o", "type": "output", "data": {"target": "hooks", "scope": "team"}}
            ],
            "edges": [{"source": "s", "target": "o"}, {"source": "p", "target": "o"}, {"source": "q", "target": "o"}]
        }"#);
        let catalog = [stop, pre, post];
        let out = compile(&d, &catalog, ToolId::Codex);
        // Still written: these are warnings only.
        assert_eq!(out.files[0].blocks, ["s", "p", "q"]);
        let nodes: Vec<_> = out.warnings.iter().map(|w| w.node.as_deref()).collect();
        assert_eq!(nodes, [Some("s"), Some("p")]);
        assert!(out.warnings[0].message.contains("JSON"));
        assert!(out.warnings[1].message.contains("deny"));
        assert!(compile(&d, &catalog, ToolId::Claude).warnings.is_empty());
    }

    #[test]
    fn codex_skills_get_a_name_and_description_in_their_frontmatter() {
        let mut skill = block("library:skill:fix", BlockKind::Skill, "Steps");
        skill.description = None;
        skill.summary = "Fixes things".into();
        skill.frontmatter = Some("allowed-tools: Read\ndisable-model-invocation: true".into());
        assert_eq!(
            skill_file(&skill, "Steps", ToolId::Codex),
            "---\nname: fix\ndescription: \"Fixes things\"\nallowed-tools: Read\ndisable-model-invocation: true\n---\n\nSteps\n"
        );
        // Claude Code gets the frontmatter as it is.
        assert_eq!(
            skill_file(&skill, "Steps", ToolId::Claude),
            "---\nallowed-tools: Read\ndisable-model-invocation: true\n---\n\nSteps\n"
        );
        // Existing fields are kept, not doubled.
        skill.frontmatter = Some("name: my-fix\ndescription: Mine".into());
        assert_eq!(
            skill_file(&skill, "Steps", ToolId::Codex),
            "---\nname: my-fix\ndescription: Mine\n---\n\nSteps\n"
        );

        // A manual-only skill warns for Codex, which has no such setting.
        skill.frontmatter = Some("disable-model-invocation: true".into());
        let d = doc(r#"{
            "nodes": [
                {"id": "s", "type": "block", "data": {"blockId": "library:skill:fix"}},
                {"id": "S", "type": "output", "data": {"target": "skills"}}
            ],
            "edges": [{"source": "s", "target": "S"}]
        }"#);
        let catalog = [skill];
        let out = compile(&d, &catalog, ToolId::Codex);
        assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
        assert_eq!(out.warnings[0].node.as_deref(), Some("s"));
        assert!(compile(&d, &catalog, ToolId::Claude).warnings.is_empty());
    }

    #[test]
    fn a_model_picked_for_claude_code_is_dropped_silently_for_codex() {
        let mut agent = block("builtin:helper", BlockKind::Agent, "Help.");
        agent.tools = None;
        let d = doc(r#"{
            "nodes": [
                {"id": "g", "type": "block", "data": {"blockId": "builtin:helper", "model": "sonnet"}},
                {"id": "G", "type": "output", "data": {"target": "agents"}}
            ],
            "edges": [{"source": "g", "target": "G"}]
        }"#);
        let out = compile(&d, &[agent], ToolId::Codex);
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert!(!out.files[0].content.contains("sonnet"));
    }

    #[test]
    fn automation_outputs_collect_instructions_and_subagents() {
        let catalog = vec![
            block("builtin:brief", BlockKind::Instruction, "Be brief."),
            Block {
                tools: Some("WebFetch, WebSearch".into()),
                ..block("builtin:scraper", BlockKind::Agent, "Scrape the pages.")
            },
            block("builtin:hook", BlockKind::Hook, "echo"),
        ];
        let d = doc(&serde_json::json!({
            "nodes": [
                { "id": "a", "type": "block", "data": { "blockId": "builtin:brief" } },
                { "id": "b", "type": "block", "data": { "blockId": "builtin:scraper", "model": "haiku" } },
                { "id": "h", "type": "block", "data": { "blockId": "builtin:hook" } },
                { "id": "o", "type": "output", "data": { "target": "automation", "order": ["b", "a"] } }
            ],
            "edges": [
                { "source": "a", "target": "o" }, { "source": "b", "target": "o" }, { "source": "h", "target": "o" }
            ]
        })
        .to_string());
        let parts = automation_parts(&d, &catalog, "o");
        assert!(parts.instructions.contains("Be brief."));
        let agent = &parts.agents["scraper"];
        assert_eq!(agent.model, "haiku");
        assert_eq!(agent.tools, ["WebFetch", "WebSearch"]);
        assert_eq!(agent.prompt, "Scrape the pages.");
        // The automation output writes no files.
        assert!(compile(&d, &catalog, ToolId::Claude).files.is_empty());
    }
}
