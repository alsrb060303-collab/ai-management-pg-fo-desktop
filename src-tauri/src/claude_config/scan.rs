use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::frontmatter;
use super::{ConfigMap, Env, Item, Kind, Scope, State};

/// CLAUDE.md files longer than this are flagged; the docs advise staying under it.
const LONG_INSTRUCTIONS_LINES: usize = 200;
/// Guards against deep or cyclic directory trees under `.claude/`.
const MAX_DEPTH: usize = 8;

pub fn scan(project: &Path, env: &Env) -> ConfigMap {
    let repo_root = find_repo_root(project);
    let settings = Settings::load(project, env);
    let mut items = Vec::new();

    items.extend(settings.items.iter().cloned());
    instructions(project, env, &mut items);
    rules(project, env, &mut items);
    skills_and_commands(project, repo_root.as_deref(), env, &settings, &mut items);
    agents(project, repo_root.as_deref(), env, &settings, &mut items);
    mcp(project, env, &settings, &mut items);
    plugins(&settings, &mut items);

    ConfigMap {
        project: project.display().to_string(),
        repo_root: repo_root.map(|p| p.display().to_string()),
        items,
        notes: vec![
            "개인 MCP 서버(~/.claude.json)는 로그인 정보가 함께 있어 읽지 않습니다.".into(),
            "설치된 플러그인 안의 skill·agent는 아직 표시하지 않습니다.".into(),
            "여러 설정 파일의 skillOverrides는 키 단위로 합쳐진다고 가정합니다(공식 문서에 명시 없음)."
                .into(),
        ],
    }
}

// ---------------------------------------------------------------------------
// Settings

/// Settings layers, lowest precedence first.
const LAYERS: [Scope; 4] = [Scope::User, Scope::Project, Scope::Local, Scope::Managed];

struct Settings {
    items: Vec<Item>,
    /// skill name -> (override value, layer that set it)
    skill_overrides: BTreeMap<String, (String, Scope)>,
    /// `permissions.deny` from every layer (lists merge across layers).
    deny: BTreeSet<String>,
    /// plugin id -> (enabled, layer that set it)
    enabled_plugins: BTreeMap<String, (bool, Scope)>,
    enabled_mcpjson: BTreeSet<String>,
    disabled_mcpjson: BTreeSet<String>,
    enable_all_mcpjson: bool,
}

impl Settings {
    fn load(project: &Path, env: &Env) -> Self {
        let mut s = Settings {
            items: Vec::new(),
            skill_overrides: BTreeMap::new(),
            deny: BTreeSet::new(),
            enabled_plugins: BTreeMap::new(),
            enabled_mcpjson: BTreeSet::new(),
            disabled_mcpjson: BTreeSet::new(),
            enable_all_mcpjson: false,
        };
        for scope in LAYERS {
            for path in settings_files(project, env, scope) {
                s.read(scope, &path);
            }
        }
        s
    }

    fn read(&mut self, scope: Scope, path: &Path) {
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let (json, detail) = match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => (Some(map), None),
            Ok(_) => (None, Some("JSON 객체가 아닙니다".to_string())),
            Err(e) => (None, Some(format!("JSON 오류: {e}"))),
        };
        let state = if json.is_some() {
            State::Active
        } else {
            State::Off
        };
        self.items.push(Item {
            kind: Kind::Settings,
            scope,
            name: file_name(path),
            path: path.display().to_string(),
            description: None,
            state,
            detail,
        });
        let Some(json) = json else {
            return;
        };

        if let Some(Value::Object(overrides)) = json.get("skillOverrides") {
            for (name, value) in overrides {
                if let Some(v) = value.as_str() {
                    self.skill_overrides
                        .insert(name.clone(), (v.to_string(), scope));
                }
            }
        }
        if let Some(Value::Object(perms)) = json.get("permissions") {
            self.deny.extend(strings(perms.get("deny")));
        }
        if let Some(Value::Object(plugins)) = json.get("enabledPlugins") {
            for (id, value) in plugins {
                if let Some(on) = value.as_bool() {
                    self.enabled_plugins.insert(id.clone(), (on, scope));
                }
            }
        }
        self.enabled_mcpjson
            .extend(strings(json.get("enabledMcpjsonServers")));
        self.disabled_mcpjson
            .extend(strings(json.get("disabledMcpjsonServers")));
        if json.get("enableAllProjectMcpServers") == Some(&Value::Bool(true)) {
            self.enable_all_mcpjson = true;
        }
    }

    /// Why a skill is off or limited, if it is.
    fn skill_state(&self, name: &str) -> Option<(State, String)> {
        if self.deny.contains("Skill") || self.deny.contains(&format!("Skill({name})")) {
            return Some((State::Off, "permissions.deny 규칙으로 차단".into()));
        }
        let (value, scope) = self.skill_overrides.get(name)?;
        let source = layer_label(*scope);
        match value.as_str() {
            "off" => Some((State::Off, format!("skillOverrides: off ({source})"))),
            "name-only" => Some((
                State::Limited,
                format!("skillOverrides: 이름만 노출 ({source})"),
            )),
            "user-invocable-only" => Some((
                State::Limited,
                format!("skillOverrides: 사용자만 호출 ({source})"),
            )),
            _ => None,
        }
    }

    fn agent_denied(&self, name: &str) -> bool {
        ["Agent", "Task"]
            .iter()
            .any(|tool| self.deny.contains(*tool) || self.deny.contains(&format!("{tool}({name})")))
    }
}

fn settings_files(project: &Path, env: &Env, scope: Scope) -> Vec<PathBuf> {
    match scope {
        Scope::User => vec![env.config_dir.join("settings.json")],
        Scope::Project => vec![project.join(".claude").join("settings.json")],
        Scope::Local => vec![project.join(".claude").join("settings.local.json")],
        Scope::Managed => {
            let mut files = vec![env.managed_dir.join("managed-settings.json")];
            if let Ok(entries) = fs::read_dir(env.managed_dir.join("managed-settings.d")) {
                let mut extra: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .collect();
                extra.sort();
                files.extend(extra);
            }
            files
        }
        Scope::Parent => Vec::new(),
    }
    .into_iter()
    .filter(|p| p.is_file())
    .collect()
}

fn strings(value: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
}

fn layer_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Managed => "관리 정책",
        Scope::User => "~/.claude/settings.json",
        Scope::Project => ".claude/settings.json",
        Scope::Local => ".claude/settings.local.json",
        Scope::Parent => "상위 폴더",
    }
}

// ---------------------------------------------------------------------------
// Instructions and rules

fn instructions(project: &Path, env: &Env, items: &mut Vec<Item>) {
    let mut found = Vec::new();
    let mut push = |scope: Scope, path: PathBuf| {
        if path.is_file() {
            found.push((scope, path));
        }
    };

    push(Scope::Managed, env.managed_dir.join("CLAUDE.md"));
    push(Scope::User, env.config_dir.join("CLAUDE.md"));
    // Ancestors load from the filesystem root down, so list them in that order.
    let mut ancestors: Vec<&Path> = project.ancestors().skip(1).collect();
    ancestors.reverse();
    for dir in ancestors {
        push(Scope::Parent, dir.join("CLAUDE.md"));
        push(Scope::Parent, dir.join("CLAUDE.local.md"));
    }
    push(Scope::Project, project.join("CLAUDE.md"));
    push(Scope::Project, project.join(".claude").join("CLAUDE.md"));
    push(Scope::Local, project.join("CLAUDE.local.md"));

    // AGENTS.md is read only when no CLAUDE.md-family file exists here or above.
    let has_claude_md = found
        .iter()
        .any(|(scope, _)| matches!(scope, Scope::Parent | Scope::Project | Scope::Local));
    let agents_md = project.join("AGENTS.md");

    for (scope, path) in found {
        items.push(instruction_item(scope, &path, State::Active, None));
    }
    if agents_md.is_file() {
        let (state, detail) = if has_claude_md {
            (
                State::NotLoaded,
                Some(
                    "CLAUDE.md가 있어서 읽히지 않음 (CLAUDE.md에 @AGENTS.md로 가져오면 함께 읽힘)"
                        .into(),
                ),
            )
        } else {
            (State::Active, Some("CLAUDE.md가 없어서 대신 읽힘".into()))
        };
        items.push(instruction_item(Scope::Project, &agents_md, state, detail));
    }
}

fn instruction_item(scope: Scope, path: &Path, state: State, detail: Option<String>) -> Item {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines = text.lines().count();
    let imports = text
        .lines()
        .filter(|l| l.trim_start().starts_with('@'))
        .count();
    let mut facts = vec![format!("{lines}줄")];
    if imports > 0 {
        facts.push(format!("@가져오기 {imports}개"));
    }
    if lines > LONG_INSTRUCTIONS_LINES {
        facts.push(format!(
            "{LONG_INSTRUCTIONS_LINES}줄 초과, 나눠서 rules로 옮기는 걸 권장"
        ));
    }
    if let Some(d) = detail {
        facts.push(d);
    }
    Item {
        kind: Kind::Instructions,
        scope,
        name: file_name(path),
        path: path.display().to_string(),
        description: first_heading(&text),
        state,
        detail: Some(facts.join(" · ")),
    }
}

fn rules(project: &Path, env: &Env, items: &mut Vec<Item>) {
    for (scope, dir) in [
        (Scope::User, env.config_dir.join("rules")),
        (Scope::Project, project.join(".claude").join("rules")),
    ] {
        for path in markdown_files(&dir) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let paths = frontmatter::parse(&text).and_then(|f| f.get("paths").cloned());
            let (state, detail) = match paths {
                Some(p) if !p.is_empty() => (State::Conditional, Some(format!("paths: {p}"))),
                _ => (State::Active, Some("항상 읽힘".into())),
            };
            items.push(Item {
                kind: Kind::Rule,
                scope,
                name: relative_name(&dir, &path),
                path: path.display().to_string(),
                description: first_heading(&text),
                state,
                detail,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Skills, commands, agents

/// Project-level `.claude/<sub>` directories: the project and each parent up to
/// the repository root, nearest first.
fn project_dirs(project: &Path, repo_root: Option<&Path>, sub: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in project.ancestors() {
        dirs.push(dir.join(".claude").join(sub));
        if repo_root.is_none_or(|root| dir == root) {
            break;
        }
    }
    dirs
}

fn skills_and_commands(
    project: &Path,
    repo_root: Option<&Path>,
    env: &Env,
    settings: &Settings,
    items: &mut Vec<Item>,
) {
    // Enterprise beats personal beats project on a name clash.
    let mut roots = vec![
        (
            Scope::Managed,
            env.managed_dir.join(".claude").join("skills"),
        ),
        (Scope::User, env.config_dir.join("skills")),
    ];
    roots.extend(
        project_dirs(project, repo_root, "skills")
            .into_iter()
            .map(|d| (Scope::Project, d)),
    );

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, root) in roots {
        for skill_md in skill_files(&root) {
            let text = fs::read_to_string(&skill_md).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let dir_name = skill_md.parent().map(file_name).unwrap_or_default();
            let name = fm.get("name").cloned().unwrap_or(dir_name);

            let (state, detail) = if let Some(winner) = taken.get(&name) {
                (
                    State::Shadowed,
                    Some(format!("같은 이름의 {} skill이 우선", scope_label(*winner))),
                )
            } else if let Some((state, why)) = settings.skill_state(&name) {
                (state, Some(why))
            } else if is_true(fm.get("disable-model-invocation")) {
                (
                    State::Limited,
                    Some("Claude가 스스로 쓰지 않음 (/명령으로만 실행)".into()),
                )
            } else if fm.get("user-invocable").is_some_and(|v| is_false(v)) {
                (
                    State::Limited,
                    Some("/ 메뉴에 안 보임 (Claude만 사용)".into()),
                )
            } else {
                (State::Active, None)
            };
            taken.entry(name.clone()).or_insert(scope);
            items.push(Item {
                kind: Kind::Skill,
                scope,
                name,
                path: skill_md.display().to_string(),
                description: fm.get("description").cloned(),
                state,
                detail,
            });
        }
    }

    // Legacy commands share the skill namespace; a skill wins a name clash.
    let mut command_roots = vec![(Scope::User, env.config_dir.join("commands"))];
    command_roots.extend(
        project_dirs(project, repo_root, "commands")
            .into_iter()
            .map(|d| (Scope::Project, d)),
    );
    for (scope, root) in command_roots {
        for path in markdown_files(&root) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let name = relative_name(&root, &path).replace('/', ":");
            let (state, detail) = if taken.contains_key(&name) {
                (State::Shadowed, Some("같은 이름의 skill이 우선".into()))
            } else {
                (State::Active, None)
            };
            items.push(Item {
                kind: Kind::Command,
                scope,
                name: format!("/{name}"),
                path: path.display().to_string(),
                description: fm.get("description").cloned(),
                state,
                detail,
            });
        }
    }
}

fn agents(
    project: &Path,
    repo_root: Option<&Path>,
    env: &Env,
    settings: &Settings,
    items: &mut Vec<Item>,
) {
    // Project definitions (nearest first) beat personal ones with the same name.
    let mut roots: Vec<(Scope, PathBuf)> = project_dirs(project, repo_root, "agents")
        .into_iter()
        .map(|d| (Scope::Project, d))
        .collect();
    roots.push((Scope::User, env.config_dir.join("agents")));

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, root) in roots {
        for path in markdown_files(&root) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let Some(name) = fm.get("name").filter(|n| !n.is_empty()).cloned() else {
                items.push(Item {
                    kind: Kind::Agent,
                    scope,
                    name: file_name(&path),
                    path: path.display().to_string(),
                    description: None,
                    state: State::NotLoaded,
                    detail: Some("frontmatter에 name이 없어 인식되지 않음".into()),
                });
                continue;
            };
            let (state, detail) = if let Some(winner) = taken.get(&name) {
                (
                    State::Shadowed,
                    Some(format!("같은 이름의 {} agent가 우선", scope_label(*winner))),
                )
            } else if settings.agent_denied(&name) {
                (State::Off, Some(format!("permissions.deny: Agent({name})")))
            } else {
                let mut facts = Vec::new();
                if let Some(model) = fm.get("model") {
                    facts.push(format!("model: {model}"));
                }
                if let Some(tools) = fm.get("tools") {
                    facts.push(format!("tools: {tools}"));
                }
                (
                    State::Active,
                    (!facts.is_empty()).then(|| facts.join(" · ")),
                )
            };
            taken.entry(name.clone()).or_insert(scope);
            items.push(Item {
                kind: Kind::Agent,
                scope,
                name,
                path: path.display().to_string(),
                description: fm.get("description").cloned(),
                state,
                detail,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// MCP servers and plugins

fn mcp(project: &Path, env: &Env, settings: &Settings, items: &mut Vec<Item>) {
    let managed = env.managed_dir.join("managed-mcp.json");
    for (name, path) in mcp_servers(&managed) {
        items.push(Item {
            kind: Kind::Mcp,
            scope: Scope::Managed,
            name,
            path,
            description: None,
            state: State::Active,
            detail: None,
        });
    }

    for (name, path) in mcp_servers(&project.join(".mcp.json")) {
        let (state, detail) = if settings.disabled_mcpjson.contains(&name) {
            (State::Off, Some("disabledMcpjsonServers에 있음".into()))
        } else if settings.enable_all_mcpjson || settings.enabled_mcpjson.contains(&name) {
            (State::Active, Some("승인됨".into()))
        } else {
            (State::Limited, Some("처음 실행할 때 승인 필요".into()))
        };
        items.push(Item {
            kind: Kind::Mcp,
            scope: Scope::Project,
            name,
            path,
            description: None,
            state,
            detail,
        });
    }
}

fn mcp_servers(path: &Path) -> Vec<(String, String)> {
    let Some(Value::Object(root)) = fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
    else {
        return Vec::new();
    };
    let servers: Map<String, Value> = match root.get("mcpServers") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    servers
        .keys()
        .map(|name| (name.clone(), path.display().to_string()))
        .collect()
}

fn plugins(settings: &Settings, items: &mut Vec<Item>) {
    for (id, (on, scope)) in &settings.enabled_plugins {
        items.push(Item {
            kind: Kind::Plugin,
            scope: *scope,
            name: id.clone(),
            path: String::new(),
            description: None,
            state: if *on { State::Active } else { State::Off },
            detail: Some(format!("enabledPlugins ({})", layer_label(*scope))),
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// `<root>/<name>/SKILL.md`, plus one extra level for grouped folders such as
/// `skills/synced/<name>/SKILL.md`.
fn skill_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in sorted_dirs(root) {
        let skill = dir.join("SKILL.md");
        if skill.is_file() {
            found.push(skill);
        } else {
            for inner in sorted_dirs(&dir) {
                let skill = inner.join("SKILL.md");
                if skill.is_file() {
                    found.push(skill);
                }
            }
        }
    }
    found
}

fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

/// All `*.md` files under `dir`, recursively, in a stable order. Symlinked
/// directories are not followed.
fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH {
            return;
        }
        let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                walk(&path, depth + 1, out);
            } else if path.extension().is_some_and(|x| x == "md") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out
}

/// Path of `path` under `root` without the `.md` extension, using `/`.
fn relative_name(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path).with_extension("");
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn first_heading(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|h| h.trim().to_string())
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Managed => "관리 정책",
        Scope::User => "개인(~/.claude)",
        Scope::Parent => "상위 폴더",
        Scope::Project => "프로젝트",
        Scope::Local => "로컬",
    }
}

fn is_true(value: Option<&String>) -> bool {
    value.is_some_and(|v| matches!(v.trim(), "true" | "yes" | "on" | "1"))
}

fn is_false(value: &str) -> bool {
    matches!(value.trim(), "false" | "no" | "off" | "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        project: PathBuf,
        env: Env,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let project = root.join("work").join("repo");
        fs::create_dir_all(project.join(".git")).unwrap();
        let env = Env {
            config_dir: root.join("home").join(".claude"),
            managed_dir: root.join("managed"),
        };
        fs::create_dir_all(&env.config_dir).unwrap();
        Fixture {
            _tmp: tmp,
            project,
            env,
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn find<'a>(map: &'a ConfigMap, kind: Kind, name: &str, scope: Scope) -> &'a Item {
        map.items
            .iter()
            .find(|i| i.kind == kind && i.name == name && i.scope == scope)
            .unwrap_or_else(|| panic!("missing {kind:?} {name} {scope:?}"))
    }

    #[test]
    fn lists_instruction_files_and_skips_agents_md_when_claude_md_exists() {
        let f = fixture();
        write(&f.env.config_dir.join("CLAUDE.md"), "# Personal\n");
        write(&f.project.join("CLAUDE.md"), "# Repo rules\n@docs/a.md\n");
        write(&f.project.join("CLAUDE.local.md"), "mine\n");
        write(&f.project.join("AGENTS.md"), "# Agents\n");
        write(&f.project.parent().unwrap().join("CLAUDE.md"), "# Parent\n");

        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Instructions, "CLAUDE.md", Scope::User).state,
            State::Active
        );
        let project_md = find(&map, Kind::Instructions, "CLAUDE.md", Scope::Project);
        assert_eq!(project_md.description.as_deref(), Some("Repo rules"));
        assert!(
            project_md
                .detail
                .as_ref()
                .unwrap()
                .contains("@가져오기 1개")
        );
        assert_eq!(
            find(&map, Kind::Instructions, "CLAUDE.md", Scope::Parent).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Instructions, "CLAUDE.local.md", Scope::Local).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Instructions, "AGENTS.md", Scope::Project).state,
            State::NotLoaded
        );
        assert_eq!(map.repo_root.as_deref(), Some(f.project.to_str().unwrap()));
    }

    #[test]
    fn agents_md_is_read_without_claude_md() {
        let f = fixture();
        write(&f.project.join("AGENTS.md"), "# Agents\n");
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Instructions, "AGENTS.md", Scope::Project).state,
            State::Active
        );
    }

    #[test]
    fn applies_skill_overrides_and_precedence() {
        let f = fixture();
        let skill = |dir: &Path, name: &str, extra: &str| {
            write(
                &dir.join(name).join("SKILL.md"),
                &format!("---\nname: {name}\ndescription: {name} skill\n{extra}---\n"),
            )
        };
        let user_skills = f.env.config_dir.join("skills");
        let project_skills = f.project.join(".claude").join("skills");
        skill(&user_skills, "deploy", "");
        skill(&project_skills, "deploy", "");
        skill(&project_skills, "review", "");
        skill(&project_skills, "legacy", "");
        skill(
            &project_skills,
            "manual",
            "disable-model-invocation: true\n",
        );
        write(
            &f.project.join(".claude").join("settings.local.json"),
            r#"{"skillOverrides": {"review": "off", "legacy": "name-only"}}"#,
        );

        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Skill, "deploy", Scope::User).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Skill, "deploy", Scope::Project).state,
            State::Shadowed
        );
        let review = find(&map, Kind::Skill, "review", Scope::Project);
        assert_eq!(review.state, State::Off);
        assert!(
            review
                .detail
                .as_ref()
                .unwrap()
                .contains("settings.local.json")
        );
        assert_eq!(
            find(&map, Kind::Skill, "legacy", Scope::Project).state,
            State::Limited
        );
        assert_eq!(
            find(&map, Kind::Skill, "manual", Scope::Project).state,
            State::Limited
        );
    }

    #[test]
    fn later_settings_layers_override_earlier_ones() {
        let f = fixture();
        write(
            &f.project
                .join(".claude")
                .join("skills")
                .join("lint")
                .join("SKILL.md"),
            "---\nname: lint\n---\n",
        );
        write(
            &f.env.config_dir.join("settings.json"),
            r#"{"skillOverrides": {"lint": "off"}}"#,
        );
        write(
            &f.project.join(".claude").join("settings.local.json"),
            r#"{"skillOverrides": {"lint": "on"}}"#,
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Skill, "lint", Scope::Project).state,
            State::Active
        );
    }

    #[test]
    fn command_is_shadowed_by_same_named_skill() {
        let f = fixture();
        let claude = f.project.join(".claude");
        write(
            &claude.join("skills").join("deploy").join("SKILL.md"),
            "---\nname: deploy\n---\n",
        );
        write(&claude.join("commands").join("deploy.md"), "Deploy it\n");
        write(
            &claude
                .join("commands")
                .join("frontend")
                .join("component.md"),
            "x\n",
        );

        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Command, "/deploy", Scope::Project).state,
            State::Shadowed
        );
        assert_eq!(
            find(&map, Kind::Command, "/frontend:component", Scope::Project).state,
            State::Active
        );
    }

    #[test]
    fn agents_respect_deny_rules_and_project_precedence() {
        let f = fixture();
        let agent = |path: PathBuf, name: &str| {
            write(
                &path,
                &format!("---\nname: {name}\ndescription: d\nmodel: haiku\n---\n"),
            )
        };
        agent(
            f.env.config_dir.join("agents").join("reviewer.md"),
            "reviewer",
        );
        agent(
            f.project.join(".claude").join("agents").join("reviewer.md"),
            "reviewer",
        );
        agent(
            f.project
                .join(".claude")
                .join("agents")
                .join("ops")
                .join("deployer.md"),
            "deployer",
        );
        write(
            &f.project.join(".claude").join("agents").join("broken.md"),
            "no frontmatter\n",
        );
        write(
            &f.project.join(".claude").join("settings.json"),
            r#"{"permissions": {"deny": ["Agent(deployer)"]}}"#,
        );

        let map = scan(&f.project, &f.env);
        let project_reviewer = find(&map, Kind::Agent, "reviewer", Scope::Project);
        assert_eq!(project_reviewer.state, State::Active);
        assert_eq!(project_reviewer.detail.as_deref(), Some("model: haiku"));
        assert_eq!(
            find(&map, Kind::Agent, "reviewer", Scope::User).state,
            State::Shadowed
        );
        assert_eq!(
            find(&map, Kind::Agent, "deployer", Scope::Project).state,
            State::Off
        );
        assert_eq!(
            find(&map, Kind::Agent, "broken.md", Scope::Project).state,
            State::NotLoaded
        );
    }

    #[test]
    fn rules_mcp_plugins_and_broken_settings() {
        let f = fixture();
        let rules = f.project.join(".claude").join("rules");
        write(&rules.join("style.md"), "# Style\n");
        write(
            &rules.join("api").join("ts.md"),
            "---\npaths: [\"src/**/*.ts\"]\n---\n# TS\n",
        );
        write(
            &f.project.join(".mcp.json"),
            r#"{"mcpServers": {"db": {}, "search": {}, "old": {}}}"#,
        );
        write(
            &f.project.join(".claude").join("settings.local.json"),
            r#"{"enabledMcpjsonServers": ["db"], "disabledMcpjsonServers": ["old"],
               "enabledPlugins": {"fmt@market": false}}"#,
        );
        write(&f.env.config_dir.join("settings.json"), "{ not json");

        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Rule, "style", Scope::Project).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Rule, "api/ts", Scope::Project).state,
            State::Conditional
        );
        assert_eq!(
            find(&map, Kind::Mcp, "db", Scope::Project).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Mcp, "search", Scope::Project).state,
            State::Limited
        );
        assert_eq!(
            find(&map, Kind::Mcp, "old", Scope::Project).state,
            State::Off
        );
        assert_eq!(
            find(&map, Kind::Plugin, "fmt@market", Scope::Local).state,
            State::Off
        );
        let broken = find(&map, Kind::Settings, "settings.json", Scope::User);
        assert_eq!(broken.state, State::Off);
        assert!(broken.detail.as_ref().unwrap().starts_with("JSON 오류"));
    }

    #[test]
    fn finds_skills_in_parent_dirs_up_to_repo_root_only() {
        let f = fixture();
        let sub = f.project.join("packages").join("web");
        fs::create_dir_all(&sub).unwrap();
        write(
            &f.project
                .join(".claude")
                .join("skills")
                .join("root")
                .join("SKILL.md"),
            "---\nname: root\n---\n",
        );
        // Above the repository root: must not be picked up as a project skill.
        write(
            &f.project
                .parent()
                .unwrap()
                .join(".claude")
                .join("skills")
                .join("outside")
                .join("SKILL.md"),
            "---\nname: outside\n---\n",
        );
        let map = scan(&sub, &f.env);
        assert!(
            map.items
                .iter()
                .any(|i| i.kind == Kind::Skill && i.name == "root")
        );
        assert!(!map.items.iter().any(|i| i.name == "outside"));
    }
}
