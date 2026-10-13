//! Skills, custom agents, MCP servers, custom prompts, command rules and hooks.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
use toml_edit::{DocumentMut, Item as TomlItem, TableLike};

use super::scan::Ctx;
use super::util::{
    field, file_name, file_stem, files_with_ext, first_heading, get, get_bool, get_str, norm,
    read_text, read_toml, skill_files, table_list,
};
use crate::tools::frontmatter;
use crate::tools::{Item, Kind, Scope, State};

/// Codex rejects longer skill names.
const MAX_SKILL_NAME: usize = 64;

// ---------------------------------------------------------------------------
// Skills

enum Selector {
    Path(PathBuf),
    Name(String),
}

pub fn skills(ctx: &Ctx, items: &mut Vec<Item>, notes: &mut Vec<String>) {
    let env = ctx.env;
    let mut roots: Vec<(Scope, PathBuf, Option<String>)> = Vec::new();
    // Repository skills, nearest folder first.
    for dir in ctx.project_dirs().into_iter().rev() {
        roots.push((Scope::Project, dir.join(".agents").join("skills"), None));
    }
    // Skills in project `.codex` layers load only once the project is trusted.
    let layer_roots: HashSet<PathBuf> = ctx
        .layers
        .iter()
        .map(|layer| norm(&layer.dir.join("skills")))
        .collect();
    for layer in &ctx.layers {
        roots.push((Scope::Project, layer.dir.join("skills"), None));
    }
    let gate = ctx.gate();
    roots.push((Scope::User, env.home.join(".agents").join("skills"), None));
    roots.push((
        Scope::User,
        env.codex_home.join("skills"),
        Some(crate::tr!(
            "예전 위치(~/.codex/skills) · 아직 읽힘",
            "Old location (~/.codex/skills) · still read"
        )),
    ));
    if let Some(system) = &env.system_dir {
        roots.push((Scope::Managed, system.join("skills"), None));
    }
    // A project at home would list `~/.agents/skills` twice: keep the user one.
    let user_roots: HashSet<PathBuf> = roots
        .iter()
        .filter(|(scope, ..)| *scope != Scope::Project)
        .map(|(_, root, _)| norm(root))
        .collect();
    let mut seen = HashSet::new();
    roots.retain(|(scope, root, _)| {
        let key = norm(root);
        (*scope != Scope::Project || !user_roots.contains(&key)) && seen.insert(key)
    });

    let toggles = skill_toggles(ctx.user_item());
    let source = format!("skills.config: enabled = false ({})", ctx.user_label());
    let start = items.len();
    for (scope, root, extra) in roots {
        let root_gate = gate
            .as_ref()
            .filter(|_| scope == Scope::Project && layer_roots.contains(&norm(&root)));
        for skill_md in skill_files(&root) {
            let text = read_text(&skill_md).unwrap_or_default();
            let fm = frontmatter::parse(&text);
            let dir_name = skill_md.parent().map(file_name).unwrap_or_default();
            let name = fm
                .as_ref()
                .and_then(|f| f.get("name"))
                .filter(|n| !n.trim().is_empty())
                .cloned()
                .unwrap_or(dir_name);
            let description = fm
                .as_ref()
                .and_then(|f| f.get("description"))
                .filter(|d| !d.trim().is_empty())
                .cloned();

            let (state, why) = if fm.is_none() {
                (
                    State::NotLoaded,
                    Some(crate::tr!(
                        "frontmatter가 없어 인식되지 않음",
                        "Not recognized: no frontmatter"
                    )),
                )
            } else if description.is_none() {
                (
                    State::NotLoaded,
                    Some(crate::tr!(
                        "description이 없어 인식되지 않음",
                        "Not recognized: no description"
                    )),
                )
            } else if name.chars().count() > MAX_SKILL_NAME {
                (
                    State::NotLoaded,
                    Some(crate::tr!(
                        "name이 {MAX_SKILL_NAME}자를 넘어 인식되지 않음",
                        "Not recognized: the name is longer than {MAX_SKILL_NAME} characters"
                    )),
                )
            } else if let Some((state, why)) = root_gate {
                (*state, Some(why.clone()))
            } else if skill_enabled(&toggles, &skill_md, &name) == Some(false) {
                (State::Off, Some(source.clone()))
            } else {
                (State::Active, None)
            };
            let detail = [extra.clone(), why]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            items.push(Item {
                kind: Kind::Skill,
                scope,
                name,
                path: skill_md.display().to_string(),
                description,
                state,
                detail: (!detail.is_empty()).then(|| detail.join(" · ")),
            });
        }
    }

    // Codex keeps same-named skills apart instead of picking one.
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in &items[start..] {
        if item.state != State::NotLoaded {
            *counts.entry(item.name.clone()).or_default() += 1;
        }
    }
    for item in &mut items[start..] {
        if item.state != State::NotLoaded && counts.get(&item.name).is_some_and(|n| *n > 1) {
            let dup = crate::tr!(
                "같은 이름의 skill이 또 있음 (둘 다 보임)",
                "Another skill has the same name (both are shown)"
            );
            item.detail = Some(match item.detail.take() {
                Some(d) => format!("{d} · {dup}"),
                None => dup,
            });
        }
    }

    let project_toggles = ctx.layers.iter().any(|layer| {
        layer
            .config
            .as_ref()
            .and_then(|c| get(c.as_item(), &["skills", "config"]))
            .is_some()
    });
    if project_toggles {
        notes.push(crate::tr!(
            "프로젝트 .codex/config.toml의 [[skills.config]]는 Codex가 무시합니다. skill을 끄려면 ~/.codex/config.toml에 적어야 합니다.",
            "Codex ignores [[skills.config]] in a project's .codex/config.toml. To turn a skill off, put it in ~/.codex/config.toml."
        ));
    }
}

/// `[[skills.config]]` entries from the user config, in file order.
fn skill_toggles(user: Option<&TomlItem>) -> Vec<(Selector, bool)> {
    let Some(list) = user.and_then(|u| get(u, &["skills", "config"])) else {
        return Vec::new();
    };
    table_list(list)
        .into_iter()
        .filter_map(|t| {
            let enabled = t.get("enabled").and_then(TomlItem::as_bool)?;
            let selector = if let Some(path) = field(t, "path") {
                Selector::Path(norm(Path::new(path)))
            } else {
                Selector::Name(field(t, "name")?.to_string())
            };
            Some((selector, enabled))
        })
        .collect()
}

/// The last matching entry wins.
fn skill_enabled(toggles: &[(Selector, bool)], skill_md: &Path, name: &str) -> Option<bool> {
    let path = norm(skill_md);
    toggles
        .iter()
        .rev()
        .find(|(selector, _)| match selector {
            Selector::Path(p) => *p == path,
            Selector::Name(n) => n == name,
        })
        .map(|(_, on)| *on)
}

// ---------------------------------------------------------------------------
// Agents

pub fn agents(ctx: &Ctx, items: &mut Vec<Item>) {
    let user = ctx.user_item();
    let all_off = user.and_then(|u| get_bool(u, &["agents", "enabled"])) == Some(false);
    let gate = ctx.gate();

    // Project layers (nearest first) beat the user's agents.
    let mut sources: Vec<(Scope, PathBuf, Option<&dyn TableLike>)> = Vec::new();
    for layer in ctx.layers.iter().rev() {
        for path in files_with_ext(&layer.dir.join("agents"), "toml") {
            sources.push((Scope::Project, path, None));
        }
    }
    for path in files_with_ext(&ctx.env.codex_home.join("agents"), "toml") {
        sources.push((Scope::User, path, None));
    }
    let mut listed: HashSet<PathBuf> = sources.iter().map(|(_, p, _)| norm(p)).collect();
    // `[agents.<name>] config_file = "..."` in the user config.
    let mut roles: Vec<(String, PathBuf, &dyn TableLike)> = Vec::new();
    if let Some(table) = user
        .and_then(|u| get(u, &["agents"]))
        .and_then(TomlItem::as_table_like)
    {
        for (name, value) in table.iter() {
            let Some(role) = value.as_table_like() else {
                continue;
            };
            let Some(file) = field(role, "config_file") else {
                continue;
            };
            let path = ctx.env.codex_home.join(file);
            if listed.insert(norm(&path)) {
                roles.push((name.to_string(), path, role));
            }
        }
    }

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    let all_off_detail = format!("agents.enabled = false ({})", ctx.user_label());
    let entries = sources
        .into_iter()
        .map(|(scope, path, _)| (scope, None, path, None))
        .chain(
            roles
                .into_iter()
                .map(|(name, path, role)| (Scope::User, Some(name), path, Some(role))),
        );
    for (scope, role_name, path, role) in entries {
        let agent = read_agent(&path, role_name.as_deref(), role);
        let name = agent.name.clone().unwrap_or_else(|| file_name(&path));
        let (state, why) = if let Some(why) = agent.invalid {
            (State::NotLoaded, Some(why))
        } else if let (Scope::Project, Some((state, why))) = (scope, &gate) {
            (*state, Some(why.clone()))
        } else if all_off {
            (State::Off, Some(all_off_detail.clone()))
        } else if let Some(winner) = taken.get(&name) {
            (
                State::Shadowed,
                Some(crate::tr!(
                    "같은 이름의 {} agent가 우선",
                    "Overridden by the {} agent with the same name",
                    scope_label(*winner)
                )),
            )
        } else {
            taken.insert(name.clone(), scope);
            (State::Active, None)
        };
        let detail = why.into_iter().chain(agent.facts).collect::<Vec<_>>();
        items.push(Item {
            kind: Kind::Agent,
            scope,
            name,
            path: path.display().to_string(),
            description: agent.description,
            state,
            detail: (!detail.is_empty()).then(|| detail.join(" · ")),
        });
    }
}

struct Agent {
    name: Option<String>,
    description: Option<String>,
    invalid: Option<String>,
    facts: Vec<String>,
}

/// An agent file, or a `[agents.<name>]` role whose `config_file` holds the
/// rest of its settings.
fn read_agent(path: &Path, role_name: Option<&str>, role: Option<&dyn TableLike>) -> Agent {
    let mut agent = Agent {
        name: role_name.map(str::to_string),
        description: role
            .and_then(|r| field(r, "description"))
            .map(str::to_string),
        invalid: None,
        facts: Vec::new(),
    };
    let doc: DocumentMut = match read_toml(path) {
        None => {
            agent.invalid = Some(crate::tr!(
                "config_file을 찾을 수 없음",
                "config_file not found"
            ));
            return agent;
        }
        Some(Err(e)) => {
            agent.invalid = Some(crate::tr!("TOML 오류: {e}", "TOML error: {e}"));
            return agent;
        }
        Some(Ok(doc)) => doc,
    };
    let root = doc.as_item();
    let text = |key: &str| {
        get_str(root, &[key])
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };
    if agent.name.is_none() {
        agent.name = text("name");
    }
    if agent.description.is_none() {
        agent.description = text("description");
    }
    let mut missing = Vec::new();
    if agent.name.is_none() {
        missing.push("name");
    }
    if agent.description.is_none() {
        missing.push("description");
    }
    // A role from the user config keeps its instructions wherever it likes.
    if role.is_none() && text("developer_instructions").is_none() {
        missing.push("developer_instructions");
    }
    if !missing.is_empty() {
        let missing = missing.join(", ");
        agent.invalid = Some(crate::tr!(
            "필수 항목이 없어 인식되지 않음: {missing}",
            "Not recognized: missing {missing}"
        ));
    }
    for key in ["model", "sandbox_mode"] {
        if let Some(value) = text(key) {
            agent.facts.push(format!("{key}: {value}"));
        }
    }
    agent
}

fn scope_label(scope: Scope) -> String {
    match scope {
        Scope::Managed => crate::tr!("관리 정책", "managed policy"),
        Scope::User => crate::tr!("개인(~/.codex)", "personal (~/.codex)"),
        Scope::Parent => crate::tr!("상위 폴더", "parent folder"),
        Scope::Project => crate::tr!("프로젝트", "project"),
        Scope::Local => crate::tr!("로컬", "local"),
    }
}

// ---------------------------------------------------------------------------
// MCP servers

pub fn mcp(ctx: &Ctx, items: &mut Vec<Item>) {
    let gate = ctx.gate();
    // Nearest project layer first, the user config last.
    let mut sources: Vec<(Scope, &Path, &TomlItem)> = Vec::new();
    for layer in ctx.layers.iter().rev() {
        if let Some(config) = &layer.config {
            sources.push((Scope::Project, &layer.config_path, config.as_item()));
        }
    }
    if let Some(user) = ctx.user_item() {
        sources.push((Scope::User, &ctx.user_path, user));
    }

    fn servers_of(config: &TomlItem) -> Option<&dyn TableLike> {
        get(config, &["mcp_servers"]).and_then(TomlItem::as_table_like)
    }
    let gated = |scope: Scope| scope == Scope::Project && gate.is_some();
    // Codex merges same-named servers field by field across the loaded layers,
    // so the nearest explicit `enabled` decides for all of them.
    let mut enabled_by: BTreeMap<String, (bool, Scope)> = BTreeMap::new();
    for (scope, _, config) in &sources {
        let Some(servers) = servers_of(config).filter(|_| !gated(*scope)) else {
            continue;
        };
        for (name, server) in servers.iter() {
            if let Some(on) = server.get("enabled").and_then(TomlItem::as_bool) {
                enabled_by.entry(name.to_string()).or_insert((on, *scope));
            }
        }
    }

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, path, config) in sources {
        let Some(servers) = servers_of(config) else {
            continue;
        };
        for (name, server) in servers.iter() {
            let Some(server) = server.as_table_like() else {
                continue;
            };
            let target = if let Some(command) = field(server, "command") {
                let args = server
                    .get("args")
                    .and_then(TomlItem::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                Some(format!("{command} {args}").trim_end().to_string())
            } else {
                field(server, "url").map(str::to_string)
            };
            let own_enabled = server.get("enabled").and_then(TomlItem::as_bool);
            let off_by = enabled_by
                .get(name)
                .filter(|(on, _)| !on)
                .map(|(_, by)| *by);
            let (state, why) = if let (Scope::Project, Some((state, why))) = (scope, &gate) {
                (*state, Some(why.clone()))
            } else if let Some(by) = off_by {
                taken.entry(name.to_string()).or_insert(scope);
                if own_enabled == Some(false) {
                    (State::Off, Some("enabled = false".to_string()))
                } else {
                    (
                        State::Off,
                        Some(crate::tr!(
                            "{} 설정의 enabled = false로 꺼짐",
                            "Turned off by enabled = false in the {} settings",
                            scope_label(by)
                        )),
                    )
                }
            } else if let Some(winner) = taken.get(name) {
                (
                    State::Shadowed,
                    Some(crate::tr!(
                        "같은 이름의 {} 서버 설정과 합쳐짐 · 둘 다 정한 값은 그쪽이 우선",
                        "Merged with the {} server of the same name · its values win where both set one",
                        scope_label(*winner)
                    )),
                )
            } else {
                taken.insert(name.to_string(), scope);
                (State::Active, None)
            };
            let detail = target.into_iter().chain(why).collect::<Vec<_>>();
            items.push(Item {
                kind: Kind::Mcp,
                scope,
                name: name.to_string(),
                path: path.display().to_string(),
                description: None,
                state,
                detail: (!detail.is_empty()).then(|| detail.join(" · ")),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Custom prompts, command rules, hooks

pub fn prompts(ctx: &Ctx, items: &mut Vec<Item>) {
    for path in files_with_ext(&ctx.env.codex_home.join("prompts"), "md") {
        let text = read_text(&path).unwrap_or_default();
        let description = frontmatter::parse(&text)
            .and_then(|f| f.get("description").cloned())
            .filter(|d| !d.trim().is_empty())
            .or_else(|| first_heading(&text));
        items.push(Item {
            kind: Kind::Command,
            scope: Scope::User,
            name: format!("/prompts:{}", file_stem(&path)),
            path: path.display().to_string(),
            description,
            state: State::Limited,
            detail: Some(crate::tr!(
                "폐지 예정(deprecated) · skill로 옮기는 걸 권장",
                "Deprecated · consider moving it to a skill"
            )),
        });
    }
}

pub fn policies(ctx: &Ctx, items: &mut Vec<Item>) {
    let gate = ctx.gate();
    let mut roots = vec![(Scope::User, ctx.env.codex_home.join("rules"))];
    roots.extend(
        ctx.layers
            .iter()
            .map(|l| (Scope::Project, l.dir.join("rules"))),
    );
    for (scope, root) in roots {
        for path in files_with_ext(&root, "rules") {
            let base = crate::tr!(
                "명령 실행 허용 규칙 (지침 아님)",
                "Rules for allowing commands (not instructions)"
            );
            let (state, detail) = match (scope, &gate) {
                (Scope::Project, Some((state, why))) => (*state, format!("{base} · {why}")),
                _ => (State::Active, base),
            };
            items.push(Item {
                kind: Kind::Policy,
                scope,
                name: file_name(&path),
                path: path.display().to_string(),
                description: None,
                state,
                detail: Some(detail),
            });
        }
    }
}

/// Hook events Codex runs. Other keys under `[hooks]` (such as the trust
/// bookkeeping table `state`) are not hooks.
const HOOK_EVENTS: &[&str] = &[
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

/// Event names of a `hooks.json`, or why Codex rejects the whole file: its
/// top level holds only `description` and a `hooks` object.
fn hooks_json_events(text: &str) -> Result<Vec<String>, String> {
    let root = match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(root)) => root,
        Ok(_) => return Err(crate::tr!("JSON 객체가 아닙니다", "Not a JSON object")),
        Err(e) => return Err(crate::tr!("JSON 오류: {e}", "JSON error: {e}")),
    };
    let unknown: Vec<&str> = root
        .keys()
        .map(String::as_str)
        .filter(|k| !matches!(*k, "description" | "hooks"))
        .collect();
    if !unknown.is_empty() {
        let keys = unknown.join(", ");
        return Err(crate::tr!(
            "Codex가 파일 전체를 거부함: 알 수 없는 최상위 키 ({keys}) · 이벤트는 \"hooks\" 객체 안에 적어야 함",
            "Codex rejects the whole file: unknown top-level keys ({keys}) · events go inside a \"hooks\" object"
        ));
    }
    match root.get("hooks") {
        Some(Value::Object(hooks)) => Ok(hooks.keys().cloned().collect()),
        _ => Err(crate::tr!(
            "Codex가 파일 전체를 거부함: \"hooks\" 객체가 없음",
            "Codex rejects the whole file: no \"hooks\" object"
        )),
    }
}

/// `[features] hooks = false` (`codex_hooks` before) turns every hook off; the
/// nearest loaded layer that sets it decides.
fn hooks_turned_off(ctx: &Ctx) -> Option<String> {
    let mut sources: Vec<(&TomlItem, String)> = Vec::new();
    if ctx.gate().is_none() {
        for layer in ctx.layers.iter().rev() {
            if let Some(config) = &layer.config {
                sources.push((config.as_item(), file_name(&layer.config_path)));
            }
        }
    }
    if let Some(user) = ctx.user_item() {
        sources.push((user, ctx.user_label()));
    }
    sources.iter().find_map(|(config, label)| {
        ["hooks", "codex_hooks"].iter().find_map(|key| {
            get_bool(config, &["features", key])
                .map(|on| (!on).then(|| format!("[features] {key} = false ({label})")))
        })
    })?
}

pub fn hooks(ctx: &Ctx, items: &mut Vec<Item>) {
    let gate = ctx.gate();
    let hooks_off = hooks_turned_off(ctx);
    let mut sources: Vec<(Scope, PathBuf, PathBuf, Option<&TomlItem>)> = vec![(
        Scope::User,
        ctx.env.codex_home.join("hooks.json"),
        ctx.user_path.clone(),
        ctx.user_item(),
    )];
    for layer in &ctx.layers {
        sources.push((
            Scope::Project,
            layer.dir.join("hooks.json"),
            layer.config_path.clone(),
            layer.config.as_ref().map(DocumentMut::as_item),
        ));
    }

    for (scope, json_path, config_path, config) in sources {
        let mut found: Vec<(String, PathBuf, Result<Vec<String>, String>)> = Vec::new();
        if let Some(text) = read_text(&json_path) {
            let events = hooks_json_events(&text);
            found.push((file_name(&json_path), json_path, events));
        }
        if let Some(table) = config
            .and_then(|c| get(c, &["hooks"]))
            .and_then(TomlItem::as_table_like)
        {
            let events: Vec<String> = table
                .iter()
                .map(|(k, _)| k.to_string())
                .filter(|k| HOOK_EVENTS.contains(&k.as_str()))
                .collect();
            if !events.is_empty() {
                found.push((
                    format!("{} [hooks]", file_name(&config_path)),
                    config_path,
                    Ok(events),
                ));
            }
        }

        for (name, path, events) in found {
            let (state, detail) = match events {
                Err(e) => (State::Off, e),
                Ok(events) => {
                    let events = if events.is_empty() {
                        crate::tr!("등록된 이벤트 없음", "No events")
                    } else {
                        events.join(", ")
                    };
                    match (scope, &gate, &hooks_off) {
                        (Scope::Project, Some((state, why)), _) => {
                            (*state, format!("{events} · {why}"))
                        }
                        (_, _, Some(off)) => (State::Off, format!("{events} · {off}")),
                        // Only managed hooks run without the user's trust.
                        _ => (
                            State::Limited,
                            format!(
                                "{events} · {}",
                                crate::tr!(
                                    "Codex /hooks에서 신뢰해야 실행 (내용이 바뀌면 다시 신뢰)",
                                    "Runs once trusted in Codex /hooks (again after each change)"
                                )
                            ),
                        ),
                    }
                }
            };
            items.push(Item {
                kind: Kind::Hook,
                scope,
                name,
                path: path.display().to_string(),
                description: None,
                state,
                detail: Some(detail),
            });
        }
    }
}
