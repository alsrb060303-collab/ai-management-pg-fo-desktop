//! Runs one automation with the user's `claude -p`, without asking for
//! permissions: tools it was not allowed are refused (`dontAsk`), so an
//! unattended run never waits on a prompt nobody sees.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::Automation;
use crate::tools::ToolId;

/// A run that takes longer is stopped.
const TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub ok: bool,
    pub result: String,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
}

/// What the run is told to do: the prompt, then how to deliver the result.
pub fn prompt(a: &Automation) -> String {
    let mut text = a.prompt.trim().to_string();
    if !a.email.is_empty() {
        text.push_str(&format!(
            "\n\nWhen you are done, email the result to {email} with the email tools of your MCP \
             servers (for example Gmail), with a short subject that says what it is. If those \
             tools can only create a draft, create a draft addressed to {email}. Then reply with \
             the result and whether the email was sent or saved as a draft.",
            email = a.email
        ));
    }
    text
}

/// Tools the run may use: the chosen ones, subagents when it has any, and the
/// email server's tools when it mails the result.
pub fn allowed_tools(a: &Automation) -> Vec<String> {
    let mut tools = a.allowed_tools.clone();
    if !a.agents.is_empty() {
        // "Agent" is the subagent tool's name; older versions call it "Task".
        tools.extend(["Agent".to_string(), "Task".to_string()]);
    }
    if !a.email.is_empty() && !a.mail_tool.is_empty() {
        tools.push(a.mail_tool.clone());
    }
    let mut seen = std::collections::HashSet::new();
    tools.retain(|t| seen.insert(t.clone()));
    tools
}

pub fn args(a: &Automation, agents_file: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--permission-mode",
        "dontAsk",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !a.model.is_empty() {
        args.extend(["--model".into(), a.model.clone()]);
    }
    let tools = allowed_tools(a);
    if !tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(tools.join(","));
    }
    if !a.instructions.trim().is_empty() {
        args.extend(["--append-system-prompt".into(), a.instructions.clone()]);
    }
    if let Some(file) = agents_file {
        args.extend(["--agents".into(), file.to_string_lossy().into_owned()]);
    }
    args
}

/// `claude -p --output-format json` prints one object with the result.
pub fn parse(ok: bool, stdout: &str, stderr: &str) -> Outcome {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        let result = value["result"].as_str().unwrap_or_default().to_string();
        let is_error = value["is_error"].as_bool().unwrap_or(!ok);
        let subtype = value["subtype"].as_str().unwrap_or_default();
        let failed = !ok || is_error || (!subtype.is_empty() && subtype != "success");
        return Outcome {
            ok: !failed,
            error: failed.then(|| {
                let why = if !result.is_empty() {
                    result.clone()
                } else if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else {
                    subtype.to_string()
                };
                why.lines().next().unwrap_or_default().to_string()
            }),
            result,
            cost_usd: value["total_cost_usd"].as_f64(),
        };
    }
    let text = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    Outcome {
        ok,
        result: text.to_string(),
        error: (!ok).then(|| text.lines().next().unwrap_or_default().to_string()),
        cost_usd: None,
    }
}

fn agents_file(dir: &Path, a: &Automation) -> std::io::Result<Option<PathBuf>> {
    if a.agents.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join("agents.json");
    std::fs::write(&path, serde_json::to_string_pretty(&a.agents)?)?;
    Ok(Some(path))
}

pub fn execute(data_dir: &Path, a: &Automation) -> Outcome {
    let failed = |e: String| Outcome {
        ok: false,
        result: String::new(),
        error: Some(e),
        cost_usd: None,
    };
    let own = super::work_dir(data_dir, &a.id);
    let dir = if a.project.is_empty() {
        own.clone()
    } else {
        PathBuf::from(&a.project)
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return failed(e.to_string());
    }
    let agents = match agents_file(&own, a) {
        Ok(path) => path,
        Err(e) => return failed(e.to_string()),
    };
    let args = args(a, agents.as_deref());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match crate::tools::cli::run_with_input(ToolId::Claude, &dir, &refs, Some(prompt(a)), TIMEOUT) {
        Ok((ok, stdout, stderr)) => parse(ok, &stdout, &stderr),
        Err(e) => failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Automation {
        super::super::normalize(super::super::tests::sample(), None, 0).unwrap()
    }

    #[test]
    fn the_command_line_carries_model_tools_and_agents() {
        let a = sample();
        let args = args(&a, Some(Path::new("/d/agents.json")));
        let at = |flag: &str| {
            let i = args.iter().position(|x| x == flag).unwrap();
            args[i + 1].as_str()
        };
        assert_eq!(args[0], "-p");
        assert_eq!(at("--permission-mode"), "dontAsk");
        assert_eq!(at("--model"), "sonnet");
        assert_eq!(
            at("--allowedTools"),
            "WebSearch,WebFetch,Agent,Task,mcp__claude_ai_Gmail"
        );
        assert_eq!(at("--agents"), "/d/agents.json");
        assert!(!args.contains(&"--append-system-prompt".to_string()));
    }

    #[test]
    fn the_prompt_asks_for_mail_only_with_an_address() {
        let mut a = sample();
        assert!(prompt(&a).contains("me@example.com"));
        a.email.clear();
        assert_eq!(prompt(&a), "Collect today's AI news.");
        assert!(!allowed_tools(&a).iter().any(|t| t.starts_with("mcp__")));
    }

    #[test]
    fn results_are_read_from_json_or_plain_text() {
        let o = parse(
            true,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.12}"#,
            "",
        );
        assert_eq!(
            o,
            Outcome {
                ok: true,
                result: "Done.".into(),
                error: None,
                cost_usd: Some(0.12)
            }
        );
        let o = parse(
            false,
            r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":""}"#,
            "",
        );
        assert!(!o.ok && o.error.as_deref() == Some("error_max_turns"));
        let o = parse(false, "", "Not logged in\nmore");
        assert!(!o.ok && o.error.as_deref() == Some("Not logged in"));
    }
}
