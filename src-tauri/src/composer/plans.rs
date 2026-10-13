//! PLANS.md: the user's plan as main goals, sub-goals and detail goals, with
//! step instructions between them (free text, a subagent to hand work to, a
//! skill to use, or a point to stop and ask). Each main goal is a plan node
//! on the canvas; the PLANS output joins them in its order.
//!
//! Claude Code does not load PLANS.md by itself: a line in CLAUDE.md points
//! to it, so it is read when work starts rather than in every session.

use std::collections::HashMap;

use serde::Deserialize;

use super::catalog::{Block, BlockKind};

/// One row under a main goal.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    /// `sub`, `detail` or `step`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    pub note: Option<String>,
    /// Goals: `y` done, `p` in progress, `n` not yet (the default).
    pub status: Option<String>,
    /// Steps: `text`, `agent`, `skill` or `ask`.
    pub action: Option<String>,
    /// Agent and skill steps: the catalog block to use.
    pub block: Option<String>,
}

/// A main goal as the plan node saves it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(default)]
    pub title: String,
    pub note: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub items: Vec<PlanItem>,
}

/// The status mark, `[y]`, `[p]` or `[n]`.
fn mark(status: Option<&str>) -> &'static str {
    match status {
        Some("y") => "[y]",
        Some("p") => "[p]",
        _ => "[n]",
    }
}

pub fn header() -> String {
    crate::tr!(
        "# PLANS\n\n상태: `[y]` 완료 · `[p]` 진행 중 · `[n]` 아직 안 함. 목표는 위에서부터 차례로 진행하고, 상태가 바뀌면 괄호 안 글자를 고쳐요. `단계 지침`은 바로 위 목표를 마친 뒤, 다음 목표로 넘어가기 전에 따라요.",
        "# PLANS\n\nStatus: `[y]` done · `[p]` in progress · `[n]` not yet. Work through the goals from the top and update the letter in the brackets as their status changes. Follow a `Step` right after the goal above it is done, before moving on to the next goal."
    )
}

/// The line CLAUDE.md gets so Claude knows the plan is there.
pub fn pointer(path: &str) -> String {
    crate::tr!(
        "## 작업 계획\n작업 계획과 진행 상태는 `{path}`에 있어요. 작업을 시작할 때 읽고, 목표 사이의 '단계 지침'은 그 단계에서만 따르며, 목표를 마치면 상태를 `[y]`로 바꿔요.",
        "## Work plan\nThe work plan and its progress are in `{path}`. Read it when starting work, follow its steps only at the point where they sit, and mark a goal `[y]` once it is done."
    )
}

/// Problems the PLANS output reports on the canvas.
pub struct Issue {
    pub node: String,
    pub message: String,
}

/// Turns the plan into Markdown. `written` holds the catalog ids of the
/// skills and subagents this document writes; steps naming others warn.
pub fn render(
    node: &str,
    plan: &Plan,
    blocks: &HashMap<&str, &Block>,
    written: &[String],
    issues: &mut Vec<Issue>,
) -> String {
    let mut out = Vec::new();
    let title = plan.title.trim();
    let untitled = crate::tr!("(제목 없는 목표)", "(Untitled goal)");
    out.push(format!(
        "## {} {}",
        mark(plan.status.as_deref()),
        if title.is_empty() { &untitled } else { title }
    ));
    if let Some(note) = plan
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        out.push(note.to_string());
    }

    // Where the next row goes: 0 under the main goal, 1 under a sub-goal,
    // 2 under a detail goal (indented into its list item).
    let mut level = 0;
    let (mut sub, mut detail) = (0, 0);
    for item in &plan.items {
        let text = item.text.trim();
        let note = item
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        // A detail goal needs a sub-goal above it; the first one becomes one.
        let kind = match item.kind.as_str() {
            "detail" if sub == 0 => "sub",
            k => k,
        };
        match kind {
            "sub" => {
                sub += 1;
                detail = 0;
                level = 1;
                out.push(format!(
                    "### {} {sub}. {text}",
                    mark(item.status.as_deref())
                ));
                if let Some(n) = note {
                    out.push(n.to_string());
                }
            }
            "detail" => {
                detail += 1;
                level = 2;
                let mut line = format!("- {} {sub}.{detail} {text}", mark(item.status.as_deref()));
                if let Some(n) = note {
                    for l in n.lines() {
                        line.push_str(&format!("\n  {l}"));
                    }
                }
                out.push(line);
            }
            "step" => {
                let Some(step) = step_text(node, item, blocks, written, issues) else {
                    continue;
                };
                let quoted = step
                    .lines()
                    .map(|l| format!("> {l}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(if level == 2 {
                    quoted
                        .lines()
                        .map(|l| format!("  {l}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    quoted
                });
            }
            _ => {}
        }
    }
    out.join("\n\n")
}

fn step_text(
    node: &str,
    item: &PlanItem,
    blocks: &HashMap<&str, &Block>,
    written: &[String],
    issues: &mut Vec<Issue>,
) -> Option<String> {
    let label = crate::tr!("**단계 지침**", "**Step**");
    let text = item.text.trim();
    let extra = if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    };
    let action = item.action.as_deref().unwrap_or("text");
    match action {
        "agent" | "skill" => {
            let want = if action == "agent" {
                BlockKind::Agent
            } else {
                BlockKind::Skill
            };
            let Some(block) = item
                .block
                .as_deref()
                .and_then(|id| blocks.get(id))
                .filter(|b| b.kind == want)
            else {
                issues.push(Issue {
                    node: node.into(),
                    message: if action == "agent" {
                        crate::tr!(
                            "단계 지침에서 맡길 subagent를 골라 주세요",
                            "Pick the subagent a step hands work to"
                        )
                    } else {
                        crate::tr!(
                            "단계 지침에서 쓸 skill을 골라 주세요",
                            "Pick the skill a step uses"
                        )
                    },
                });
                return None;
            };
            if !written.iter().any(|id| id == &block.id) {
                issues.push(Issue {
                    node: node.into(),
                    message: if action == "agent" {
                        crate::tr!(
                            "'{}' subagent가 이 문서에서 써지지 않아요. SUBAGENTS 출력에 연결해야 Claude가 맡길 수 있어요",
                            "The '{}' subagent isn't written by this document. Connect it to a SUBAGENTS output so Claude can use it",
                            block.display_title()
                        )
                    } else {
                        crate::tr!(
                            "'{}' skill이 이 문서에서 써지지 않아요. SKILLS 출력에 연결해야 Claude가 쓸 수 있어요",
                            "The '{}' skill isn't written by this document. Connect it to a SKILLS output so Claude can use it",
                            block.display_title()
                        )
                    },
                });
            }
            Some(if action == "agent" {
                crate::tr!(
                    "{label}: 이 일은 `{}` subagent에게 맡겨요{extra}",
                    "{label}: hand this to the `{}` subagent{extra}",
                    block.name
                )
            } else {
                crate::tr!(
                    "{label}: `{}` skill을 써요{extra}",
                    "{label}: use the `{}` skill{extra}",
                    block.name
                )
            })
        }
        "ask" => Some(crate::tr!(
            "{label}: 여기서 멈추고, 다음 목표로 넘어가기 전에 사용자에게 확인받아요{extra}",
            "{label}: stop here and check with the user before moving on to the next goal{extra}"
        )),
        _ if text.is_empty() => None,
        _ => Some(format!("{label}: {text}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_default_to_not_yet() {
        assert_eq!(mark(Some("y")), "[y]");
        assert_eq!(mark(Some("p")), "[p]");
        assert_eq!(mark(None), "[n]");
        assert_eq!(mark(Some("x")), "[n]");
    }
}
