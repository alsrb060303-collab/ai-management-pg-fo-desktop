//! Turns a composer graph into the files it describes. Pure: no IO.
//!
//! Output nodes decide where blocks go:
//! - CLAUDE.md (team) / CLAUDE.local.md (personal): a managed section holding
//!   the connected instruction blocks in the node's order
//! - rules: one `.claude/rules/<name>.md` per block
//! - skills: `.claude/skills/<name>/SKILL.md`
//! - agents: `.claude/agents/<name>.md`

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::catalog::{Block, BlockKind};
use crate::library::{Mode, check_id};

/// CLAUDE.md guidance: longer files are followed less reliably.
pub const CLAUDE_MD_LINE_BUDGET: usize = 200;

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
    /// Output nodes.
    pub target: Option<Target>,
    pub scope: Option<Mode>,
    /// Output nodes: block node ids in document order.
    #[serde(default)]
    pub order: Vec<String>,
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
}

impl Target {
    pub fn accepts(self, kind: BlockKind) -> bool {
        match self {
            Target::ClaudeMd => kind == BlockKind::Instruction,
            Target::Rules => matches!(kind, BlockKind::Rule | BlockKind::Instruction),
            Target::Skills => kind == BlockKind::Skill,
            Target::Agents => kind == BlockKind::Agent,
        }
    }
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
            let Some(block_id) = node.data.block_id.as_deref() else {
                continue;
            };
            let Some(block) = blocks.get(block_id) else {
                warn(
                    &mut out,
                    node_id,
                    format!("블록을 찾을 수 없어요: {block_id}"),
                );
                continue;
            };
            if !target.accepts(block.kind) {
                warn(
                    &mut out,
                    node_id,
                    format!("'{}'은(는) 여기에 연결할 수 없어요", block.title),
                );
                continue;
            }
            if !seen_blocks.insert(block_id) {
                warn(
                    &mut out,
                    node_id,
                    format!("'{}' 블록이 두 번 연결됐어요", block.title),
                );
                continue;
            }
            let body = node.data.body.as_deref().unwrap_or(&block.body).trim();
            if has_import_like(body) {
                warn(
                    &mut out,
                    node_id,
                    format!(
                        "'{}' 본문의 @경로는 Claude Code가 파일 불러오기로 읽어요. 의도한 게 아니면 `@...`처럼 백틱으로 감싸 주세요",
                        block.title
                    ),
                );
            }
            resolved.push((node_id, block, body.to_string()));
        }

        // An empty output shows its own hint on the canvas.
        if resolved.is_empty() {
            continue;
        }

        let mut files = Vec::new();
        match target {
            Target::ClaudeMd => {
                let content = resolved
                    .iter()
                    .map(|(_, _, body)| body.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let lines = content.lines().count();
                if lines > CLAUDE_MD_LINE_BUDGET {
                    warn(
                        &mut out,
                        &output.id,
                        format!(
                            "{lines}줄이에요. CLAUDE.md는 {CLAUDE_MD_LINE_BUDGET}줄 이하일 때 더 잘 지켜져요. 일부를 규칙이나 skill로 옮겨 보세요"
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
            Target::Rules | Target::Skills | Target::Agents => {
                for (node_id, block, body) in &resolved {
                    if check_id(&block.name).is_err() {
                        warn(
                            &mut out,
                            node_id,
                            format!("파일 이름으로 쓸 수 없어요: {}", block.name),
                        );
                        continue;
                    }
                    let (path, content) = match target {
                        Target::Rules => (
                            format!(".claude/rules/{}.md", block.name),
                            rule_file(&block.paths, body),
                        ),
                        Target::Skills => (
                            format!(".claude/skills/{}/SKILL.md", block.name),
                            skill_file(block, body),
                        ),
                        _ => (
                            format!(".claude/agents/{}.md", block.name),
                            agent_file(block, body),
                        ),
                    };
                    if block.extra_files > 0 {
                        warn(
                            &mut out,
                            node_id,
                            format!(
                                "'{}'의 부속 파일 {}개는 복사되지 않아요. 통째로 넣으려면 라이브러리의 '적용'을 쓰세요",
                                block.title, block.extra_files
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
                }
            }
        }

        for file in files {
            if paths.insert(file.path.clone()) {
                out.files.push(file);
            } else {
                let message = format!(
                    "{}을(를) 두 출력이 같이 쓰려고 해요. 하나만 남겨 주세요",
                    file.path
                );
                warn(&mut out, &output.id, message);
            }
        }
    }
    out
}

/// YAML double-quoted scalar, safe for any text.
fn quoted(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ");
    format!("\"{escaped}\"")
}

fn rule_file(paths: &[String], body: &str) -> String {
    if paths.is_empty() {
        return format!("{body}\n");
    }
    let list: String = paths
        .iter()
        .map(|p| format!("  - {}\n", quoted(p)))
        .collect();
    format!("---\npaths:\n{list}---\n\n{body}\n")
}

fn skill_file(block: &Block, body: &str) -> String {
    let description = block.description.as_deref().unwrap_or(&block.summary);
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{body}\n",
        block.name,
        quoted(description)
    )
}

fn agent_file(block: &Block, body: &str) -> String {
    let description = block.description.as_deref().unwrap_or(&block.summary);
    let mut fm = format!(
        "---\nname: {}\ndescription: {}\n",
        block.name,
        quoted(description)
    );
    if let Some(tools) = block.tools.as_deref().filter(|t| !t.is_empty()) {
        fm.push_str(&format!("tools: {tools}\n"));
    }
    if let Some(model) = block.model.as_deref().filter(|m| !m.is_empty()) {
        fm.push_str(&format!("model: {model}\n"));
    }
    format!("{fm}---\n\n{body}\n")
}

/// Whether text has an `@path`-like token outside code, which CLAUDE.md
/// treats as an import.
fn has_import_like(text: &str) -> bool {
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
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
            if let Some(rest) = word.strip_prefix('@')
                && rest.contains(['/', '.'])
                && !rest.contains('@')
            {
                return true;
            }
        }
    }
    false
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
            source: Source::Builtin,
            name: id.split(':').next_back().unwrap().into(),
            description: Some("Does: things".into()),
            paths: Vec::new(),
            tools: Some("Read, Grep".into()),
            model: None,
            body: body.into(),
            extra_files: 0,
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
    fn import_detection_skips_code_and_emails() {
        assert!(has_import_like("Read @README.md first"));
        assert!(has_import_like("@~/.claude/notes.md"));
        assert!(!has_import_like("Use `@scope/pkg` here"));
        assert!(!has_import_like("```\n@docs/x.md\n```"));
        assert!(!has_import_like("mail me@example.com or @someone"));
    }
}
