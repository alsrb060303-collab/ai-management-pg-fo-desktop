use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde_json::Value;

use super::frontmatter;
use super::{ConfigMap, Env, Item, Kind, Scope, State};

/// CLAUDE.md files longer than this are flagged; the docs advise staying under it.
const LONG_INSTRUCTIONS_LINES: usize = 200;
/// Guards against deep or cyclic directory trees under `.claude/`.
const MAX_DEPTH: usize = 8;
/// Subagent names longer than this are rejected by Claude Code.
const MAX_AGENT_NAME: usize = 256;
/// Built-in plugin that decides whether AGENTS.md is read.
const AGENTS_MD_PLUGINS: [&str; 2] = ["cc-plugin-agents-md@builtin", "agents-md@builtin"];

pub fn scan(project: &Path, env: &Env) -> ConfigMap {
    let repo_root = find_repo_root(project);
    let ctx = Ctx {
        project,
        repo_root: repo_root.as_deref(),
        env,
        user_dir: canonical(&env.config_dir),
    };
    let settings = Settings::load(&ctx);
    let mut items = Vec::new();

    items.extend(settings.items.iter().cloned());
    instructions(&ctx, &settings, &mut items);
    rules(&ctx, &settings, &mut items);
    skills_and_commands(&ctx, &settings, &mut items);
    agents(&ctx, &settings, &mut items);
    mcp(&ctx, &settings, &mut items);
    plugins(&settings, &mut items);

    let mut notes = vec![
        "개인 MCP 서버(~/.claude.json)는 로그인 정보가 함께 있어 읽지 않습니다.".to_string(),
        "설치된 플러그인 안의 skill·agent는 아직 표시하지 않습니다.".to_string(),
        "여러 설정 파일의 skillOverrides는 키 단위로 합쳐진다고 가정합니다(공식 문서에 명시 없음)."
            .to_string(),
    ];
    if cfg!(windows) {
        notes.push("레지스트리·MDM으로 배포된 관리 정책은 표시하지 않습니다.".to_string());
    }

    ConfigMap {
        project: project.display().to_string(),
        repo_root: repo_root.map(|p| p.display().to_string()),
        items,
        notes,
    }
}

struct Ctx<'a> {
    project: &'a Path,
    repo_root: Option<&'a Path>,
    env: &'a Env,
    /// Canonical `~/.claude`, so a project at `~` does not list it twice.
    user_dir: PathBuf,
}

impl Ctx<'_> {
    /// `<dir>/.claude/<sub>` unless that is the user config directory itself.
    fn project_claude(&self, dir: &Path, sub: &str) -> Option<PathBuf> {
        let claude = dir.join(".claude");
        (canonical(&claude) != self.user_dir).then(|| claude.join(sub))
    }

    /// Project-level `.claude/<sub>` directories: the project and each parent up
    /// to the repository root, nearest first.
    fn project_dirs(&self, sub: &str) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for dir in self.project.ancestors() {
            dirs.extend(self.project_claude(dir, sub));
            if self.repo_root.is_none_or(|root| dir == root) {
                break;
            }
        }
        dirs
    }
}

// ---------------------------------------------------------------------------
// Settings

/// Settings layers, lowest precedence first.
const LAYERS: [Scope; 4] = [Scope::User, Scope::Project, Scope::Local, Scope::Managed];
/// Surfaces `strictPluginOnlyCustomization` can lock to plugins only.
const PLUGIN_ONLY_SURFACES: [&str; 4] = ["skills", "agents", "hooks", "mcp"];

struct Settings {
    items: Vec<Item>,
    /// skill name -> (override value, layer that set it)
    skill_overrides: BTreeMap<String, (String, Scope)>,
    /// `permissions.deny` from every layer (lists merge across layers).
    deny: BTreeSet<String>,
    /// plugin id -> (enabled, layer that set it)
    enabled_plugins: BTreeMap<String, (bool, Scope)>,
    /// MCP server approvals; committed project settings only apply once the
    /// folder is trusted, so they are kept apart.
    mcp_approved: BTreeSet<String>,
    mcp_approved_by_project: BTreeSet<String>,
    mcp_disabled: BTreeSet<String>,
    mcp_approve_all: bool,
    mcp_approve_all_by_project: bool,
    claude_md_excludes: Vec<String>,
    /// From managed settings only.
    plugin_only: BTreeSet<&'static str>,
    /// AGENTS.md mode; honored from user and managed settings only.
    instruction_files: Option<String>,
}

impl Settings {
    fn load(ctx: &Ctx) -> Self {
        let mut s = Settings {
            items: Vec::new(),
            skill_overrides: BTreeMap::new(),
            deny: BTreeSet::new(),
            enabled_plugins: BTreeMap::new(),
            mcp_approved: BTreeSet::new(),
            mcp_approved_by_project: BTreeSet::new(),
            mcp_disabled: BTreeSet::new(),
            mcp_approve_all: false,
            mcp_approve_all_by_project: false,
            claude_md_excludes: Vec::new(),
            plugin_only: BTreeSet::new(),
            instruction_files: None,
        };
        for scope in LAYERS {
            for path in settings_files(ctx, scope) {
                s.read(scope, &path);
            }
        }
        s
    }

    fn read(&mut self, scope: Scope, path: &Path) {
        let Some(text) = read_text(path) else {
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

        let approved = strings(json.get("enabledMcpjsonServers"));
        let approve_all = json.get("enableAllProjectMcpServers") == Some(&Value::Bool(true));
        if scope == Scope::Project {
            self.mcp_approved_by_project.extend(approved);
            self.mcp_approve_all_by_project |= approve_all;
        } else {
            self.mcp_approved.extend(approved);
            self.mcp_approve_all |= approve_all;
        }
        self.mcp_disabled
            .extend(strings(json.get("disabledMcpjsonServers")));
        self.claude_md_excludes
            .extend(strings(json.get("claudeMdExcludes")));

        if scope == Scope::Managed {
            match json.get("strictPluginOnlyCustomization") {
                None | Some(Value::Bool(false)) => {}
                Some(Value::Array(list)) => {
                    for v in list {
                        if let Some(s) = PLUGIN_ONLY_SURFACES
                            .iter()
                            .find(|s| v.as_str() == Some(**s))
                        {
                            self.plugin_only.insert(s);
                        }
                    }
                }
                // `true` (or anything unexpected) locks every surface.
                Some(_) => self.plugin_only.extend(PLUGIN_ONLY_SURFACES),
            }
        }
        if matches!(scope, Scope::User | Scope::Managed)
            && let Some(Value::Object(configs)) = json.get("pluginConfigs")
        {
            let mode = AGENTS_MD_PLUGINS.iter().find_map(|id| {
                configs
                    .get(*id)?
                    .get("options")?
                    .get("instructionFiles")?
                    .as_str()
            });
            if let Some(mode) = mode {
                self.instruction_files = Some(mode.to_string());
            }
        }
    }

    /// Why a skill is off or limited, if it is.
    fn skill_state(&self, name: &str) -> Option<(State, String)> {
        if self.deny.iter().any(|rule| skill_rule_matches(rule, name)) {
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

    fn excluded(&self) -> Option<GlobSet> {
        if self.claude_md_excludes.is_empty() {
            return None;
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.claude_md_excludes {
            if let Ok(glob) = Glob::new(&pattern.replace('\\', "/")) {
                builder.add(glob);
            }
        }
        builder.build().ok()
    }

    fn agents_md_mode(&self) -> &str {
        self.instruction_files
            .as_deref()
            .unwrap_or("claude-md-or-agents-md")
    }
}

/// `Skill`, `Skill(name)`, `Skill(skill:name)` and prefix forms such as
/// `Skill(deploy *)` / `Skill(deploy:*)` / `Skill(*)`.
fn skill_rule_matches(rule: &str, name: &str) -> bool {
    if rule == "Skill" {
        return true;
    }
    let Some(spec) = rule
        .strip_prefix("Skill(")
        .and_then(|r| r.strip_suffix(')'))
    else {
        return false;
    };
    let spec = spec.strip_prefix("skill:").unwrap_or(spec).trim();
    if spec == "*" || spec == name {
        return true;
    }
    let prefix = spec.strip_suffix(" *").or_else(|| spec.strip_suffix(":*"));
    prefix.is_some_and(|p| name.starts_with(p.trim()))
}

fn settings_files(ctx: &Ctx, scope: Scope) -> Vec<PathBuf> {
    let env = ctx.env;
    match scope {
        Scope::User => vec![env.config_dir.join("settings.json")],
        Scope::Project => ctx
            .project_claude(ctx.project, "settings.json")
            .into_iter()
            .collect(),
        Scope::Local => ctx
            .project_claude(ctx.project, "settings.local.json")
            .into_iter()
            .collect(),
        Scope::Managed => {
            let mut files = vec![env.managed_dir.join("managed-settings.json")];
            if let Ok(entries) = fs::read_dir(env.managed_dir.join("managed-settings.d")) {
                let mut extra: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json") && !is_hidden(p))
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

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
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

fn instructions(ctx: &Ctx, settings: &Settings, items: &mut Vec<Item>) {
    let (project, env) = (ctx.project, ctx.env);
    let mut found: Vec<(Scope, PathBuf)> = Vec::new();
    let mut push = |scope: Scope, path: Option<PathBuf>| {
        if let Some(path) = path.filter(|p| p.is_file()) {
            found.push((scope, path));
        }
    };

    push(Scope::Managed, Some(env.managed_dir.join("CLAUDE.md")));
    push(Scope::User, Some(env.config_dir.join("CLAUDE.md")));
    // Ancestors load from the filesystem root down, so list them in that order.
    let mut ancestors: Vec<&Path> = project.ancestors().skip(1).collect();
    ancestors.reverse();
    for dir in &ancestors {
        push(Scope::Parent, Some(dir.join("CLAUDE.md")));
        push(Scope::Parent, ctx.project_claude(dir, "CLAUDE.md"));
        push(Scope::Parent, Some(dir.join("CLAUDE.local.md")));
    }
    push(Scope::Project, Some(project.join("CLAUDE.md")));
    push(Scope::Project, ctx.project_claude(project, "CLAUDE.md"));
    push(Scope::Local, Some(project.join("CLAUDE.local.md")));

    let excluded = settings.excluded();
    let mode = settings.agents_md_mode();
    let managed_only = mode == "managed-only";
    let is_excluded = |scope: Scope, path: &Path| {
        scope != Scope::Managed && excluded.as_ref().is_some_and(|g| g.is_match(slashed(path)))
    };

    // AGENTS.md is read only when no CLAUDE.md-family file exists here or above.
    let has_claude_md = found.iter().any(|(scope, path)| {
        matches!(scope, Scope::Parent | Scope::Project | Scope::Local) && !is_excluded(*scope, path)
    });

    for (scope, path) in &found {
        let (state, detail) = if managed_only && *scope != Scope::Managed {
            (
                State::NotLoaded,
                Some("관리 정책 CLAUDE.md만 읽도록 설정됨".into()),
            )
        } else if is_excluded(*scope, path) {
            (State::NotLoaded, Some("claudeMdExcludes로 제외".into()))
        } else {
            (State::Active, None)
        };
        items.push(instruction_item(*scope, path, state, detail));
    }

    let agents_md = project.join("AGENTS.md");
    if agents_md.is_file() {
        let (state, detail) = match mode {
            "managed-only" => (State::NotLoaded, "관리 정책 CLAUDE.md만 읽도록 설정됨"),
            "claude-md" => (State::NotLoaded, "설정(instructionFiles)으로 제외"),
            "claude-md-and-agents-md" => (State::Active, "설정에 따라 CLAUDE.md와 함께 읽힘"),
            _ if has_claude_md => (
                State::NotLoaded,
                "CLAUDE.md가 있어서 읽히지 않음 (CLAUDE.md에 @AGENTS.md로 가져오면 함께 읽힘)",
            ),
            _ => (State::Active, "CLAUDE.md가 없어서 대신 읽힘"),
        };
        items.push(instruction_item(
            Scope::Project,
            &agents_md,
            state,
            Some(detail.into()),
        ));
    }
}

fn instruction_item(scope: Scope, path: &Path, state: State, detail: Option<String>) -> Item {
    let text = read_text(path).unwrap_or_default();
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

fn rules(ctx: &Ctx, settings: &Settings, items: &mut Vec<Item>) {
    let excluded = settings.excluded();
    let managed_only = settings.agents_md_mode() == "managed-only";
    let roots = [
        (Scope::User, Some(ctx.env.config_dir.join("rules"))),
        (Scope::Project, ctx.project_claude(ctx.project, "rules")),
    ];
    for (scope, dir) in roots {
        let Some(dir) = dir else { continue };
        for path in markdown_files(&dir) {
            let text = read_text(&path).unwrap_or_default();
            let paths = frontmatter::parse(&text).and_then(|f| f.get("paths").cloned());
            let (state, detail) = if managed_only {
                (
                    State::NotLoaded,
                    Some("관리 정책 CLAUDE.md만 읽도록 설정됨".into()),
                )
            } else if excluded
                .as_ref()
                .is_some_and(|g| g.is_match(slashed(&path)))
            {
                (State::NotLoaded, Some("claudeMdExcludes로 제외".into()))
            } else {
                match paths {
                    Some(p) if !p.is_empty() => (State::Conditional, Some(format!("paths: {p}"))),
                    _ => (State::Active, Some("항상 읽힘".into())),
                }
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

const PLUGIN_ONLY_DETAIL: &str = "관리 정책: 플러그인으로 배포된 것만 허용";

fn skills_and_commands(ctx: &Ctx, settings: &Settings, items: &mut Vec<Item>) {
    let env = ctx.env;
    // Enterprise beats personal beats project on a name clash.
    let mut roots = vec![
        (
            Scope::Managed,
            env.managed_dir.join(".claude").join("skills"),
        ),
        (Scope::User, env.config_dir.join("skills")),
    ];
    roots.extend(
        ctx.project_dirs("skills")
            .into_iter()
            .map(|d| (Scope::Project, d)),
    );
    let plugin_only = settings.plugin_only.contains("skills");

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, root) in roots {
        for skill_md in skill_files(&root) {
            let text = read_text(&skill_md).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let dir_name = skill_md.parent().map(file_name).unwrap_or_default();
            let name = fm.get("name").cloned().unwrap_or(dir_name);

            let (state, detail) = if plugin_only && scope != Scope::Managed {
                (State::Off, Some(PLUGIN_ONLY_DETAIL.into()))
            } else if let Some(winner) = taken.get(&name) {
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
            if state != State::Off || !plugin_only {
                taken.entry(name.clone()).or_insert(scope);
            }
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

    // Skills synced from claude.ai live under ~/.claude/skills/synced and are
    // namespaced, so they never clash with local skills.
    for skill_md in skill_files(&env.config_dir.join("skills").join("synced")) {
        let text = read_text(&skill_md).unwrap_or_default();
        let fm = frontmatter::parse(&text).unwrap_or_default();
        let dir_name = skill_md.parent().map(file_name).unwrap_or_default();
        items.push(Item {
            kind: Kind::Skill,
            scope: Scope::User,
            name: format!("anthropic-skills:{dir_name}"),
            path: skill_md.display().to_string(),
            description: fm.get("description").cloned(),
            state: State::Limited,
            detail: Some("claude.ai에서 동기화됨 · 로그인한 세션에서만 사용".into()),
        });
    }

    // Legacy commands share the skill namespace; a skill wins a name clash.
    let mut command_roots = vec![(Scope::User, env.config_dir.join("commands"))];
    command_roots.extend(
        ctx.project_dirs("commands")
            .into_iter()
            .map(|d| (Scope::Project, d)),
    );
    for (scope, root) in command_roots {
        for path in markdown_files(&root) {
            let text = read_text(&path).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let name = relative_name(&root, &path).replace('/', ":");
            let (state, detail) = if plugin_only {
                (State::Off, Some(PLUGIN_ONLY_DETAIL.into()))
            } else if taken.contains_key(&name) {
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

fn agents(ctx: &Ctx, settings: &Settings, items: &mut Vec<Item>) {
    // Managed beats project (nearest first) beats personal for the same name.
    let mut roots = vec![(
        Scope::Managed,
        ctx.env.managed_dir.join(".claude").join("agents"),
    )];
    roots.extend(
        ctx.project_dirs("agents")
            .into_iter()
            .map(|d| (Scope::Project, d)),
    );
    roots.push((Scope::User, ctx.env.config_dir.join("agents")));
    let plugin_only = settings.plugin_only.contains("agents");

    let mut taken: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, root) in roots {
        for path in markdown_files(&root) {
            let text = read_text(&path).unwrap_or_default();
            let fm = frontmatter::parse(&text).unwrap_or_default();
            let name = fm.get("name").filter(|n| !n.is_empty()).cloned();
            let invalid = match &name {
                None => Some("frontmatter에 name이 없어 인식되지 않음"),
                Some(n) if n.contains(':') => Some("name에 콜론(:)이 있어 인식되지 않음"),
                Some(n) if n.chars().count() > MAX_AGENT_NAME => {
                    Some("name이 256자를 넘어 인식되지 않음")
                }
                Some(_) => None,
            };
            if let Some(why) = invalid {
                items.push(Item {
                    kind: Kind::Agent,
                    scope,
                    name: name.unwrap_or_else(|| file_name(&path)),
                    path: path.display().to_string(),
                    description: None,
                    state: State::NotLoaded,
                    detail: Some(why.into()),
                });
                continue;
            }
            let name = name.unwrap_or_default();

            let (state, detail) = if plugin_only && scope != Scope::Managed {
                (State::Off, Some(PLUGIN_ONLY_DETAIL.into()))
            } else if let Some(winner) = taken.get(&name) {
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
            if state != State::Off || !plugin_only {
                taken.entry(name.clone()).or_insert(scope);
            }
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

fn mcp(ctx: &Ctx, settings: &Settings, items: &mut Vec<Item>) {
    let managed = ctx.env.managed_dir.join("managed-mcp.json");
    // A managed-mcp.json takes exclusive control, even when it lists no servers.
    let exclusive = matches!(read_json(&managed), Some(Ok(Value::Object(_))));
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

    let project_mcp = ctx.project.join(".mcp.json");
    if let Some(Err(e)) = read_json(&project_mcp) {
        items.push(Item {
            kind: Kind::Mcp,
            scope: Scope::Project,
            name: ".mcp.json".into(),
            path: project_mcp.display().to_string(),
            description: None,
            state: State::Off,
            detail: Some(format!("JSON 오류: {e}")),
        });
    }
    for (name, path) in mcp_servers(&project_mcp) {
        let (state, detail) = if exclusive {
            (
                State::NotLoaded,
                "관리 정책 managed-mcp.json이 MCP를 독점 관리",
            )
        } else if settings.plugin_only.contains("mcp") {
            (State::Off, PLUGIN_ONLY_DETAIL)
        } else if settings.mcp_disabled.contains(&name) {
            (State::Off, "disabledMcpjsonServers에 있음")
        } else if settings.mcp_approve_all || settings.mcp_approved.contains(&name) {
            (State::Active, "승인됨")
        } else if settings.mcp_approve_all_by_project
            || settings.mcp_approved_by_project.contains(&name)
        {
            (
                State::Limited,
                "커밋된 설정에서 승인 · 폴더를 신뢰한 뒤에 적용",
            )
        } else {
            (State::Limited, "처음 실행할 때 승인 필요")
        };
        items.push(Item {
            kind: Kind::Mcp,
            scope: Scope::Project,
            name,
            path,
            description: None,
            state,
            detail: Some(detail.into()),
        });
    }
}

fn mcp_servers(path: &Path) -> Vec<(String, String)> {
    let Some(Ok(Value::Object(root))) = read_json(path) else {
        return Vec::new();
    };
    match root.get("mcpServers") {
        Some(Value::Object(servers)) => servers
            .keys()
            .map(|name| (name.clone(), path.display().to_string()))
            .collect(),
        _ => Vec::new(),
    }
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

/// Reads a text file as UTF-8 (lossy, like Claude Code) without a leading BOM.
fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

/// `None` when the file is missing, otherwise the parse result.
fn read_json(path: &Path) -> Option<Result<Value, serde_json::Error>> {
    read_text(path).map(|t| serde_json::from_str(&t))
}

fn canonical(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path
    }
}

/// Absolute path with forward slashes, for glob matching.
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// `<root>/<name>/SKILL.md`. Hidden folders (such as `.trash`) and the
/// claude.ai `synced` folder are not skill folders.
fn skill_files(root: &Path) -> Vec<PathBuf> {
    sorted_dirs(root)
        .into_iter()
        .filter(|dir| !is_hidden(dir) && dir.file_name().is_none_or(|n| n != "synced"))
        .map(|dir| dir.join("SKILL.md"))
        .filter(|skill| skill.is_file())
        .collect()
}

/// Subdirectories of `dir`, following symlinks, in a stable order.
fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// All `*.md` files under `dir`, recursively, in a stable order. Symlinked
/// directories are followed (Claude Code supports them); each real directory is
/// visited once, so link cycles end.
fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, seen: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH || !seen.insert(canonical(dir)) {
            return;
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if !is_hidden(&path) {
                    walk(&path, depth + 1, seen, out);
                }
            } else if path.extension().is_some_and(|x| x == "md") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut HashSet::new(), &mut out);
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

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_skill_and_rule_folders() {
        use std::os::unix::fs::symlink;
        let f = fixture();
        let shared = f._tmp.path().join("dotfiles");
        write(
            &shared.join("deploy").join("SKILL.md"),
            "---\nname: deploy\n---\n",
        );
        write(&shared.join("rules").join("style.md"), "# Style\n");
        fs::create_dir_all(f.env.config_dir.join("skills")).unwrap();
        symlink(
            shared.join("deploy"),
            f.env.config_dir.join("skills").join("deploy"),
        )
        .unwrap();
        fs::create_dir_all(f.project.join(".claude").join("rules")).unwrap();
        symlink(
            shared.join("rules"),
            f.project.join(".claude").join("rules").join("shared"),
        )
        .unwrap();
        // A cycle must not hang the walk.
        symlink(
            &f.project,
            f.project.join(".claude").join("rules").join("loop"),
        )
        .unwrap();

        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Skill, "deploy", Scope::User).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Rule, "shared/style", Scope::Project).state,
            State::Active
        );
    }

    #[test]
    fn synced_and_trashed_skills() {
        let f = fixture();
        let skills = f.env.config_dir.join("skills");
        write(
            &skills.join("synced").join("deploy").join("SKILL.md"),
            "---\nname: deploy\n---\n",
        );
        write(
            &skills.join(".trash").join("old").join("SKILL.md"),
            "---\nname: old\n---\n",
        );
        write(
            &f.project
                .join(".claude")
                .join("skills")
                .join("deploy")
                .join("SKILL.md"),
            "---\nname: deploy\n---\n",
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Skill, "deploy", Scope::Project).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Skill, "anthropic-skills:deploy", Scope::User).state,
            State::Limited
        );
        assert!(!map.items.iter().any(|i| i.name == "old"));
    }

    #[test]
    fn managed_agents_win_and_invalid_names_are_skipped() {
        let f = fixture();
        let agent = |path: PathBuf, name: &str| write(&path, &format!("---\nname: {name}\n---\n"));
        agent(
            f.env
                .managed_dir
                .join(".claude")
                .join("agents")
                .join("a.md"),
            "reviewer",
        );
        agent(
            f.project.join(".claude").join("agents").join("a.md"),
            "reviewer",
        );
        agent(
            f.project.join(".claude").join("agents").join("bad.md"),
            "x:y",
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Agent, "reviewer", Scope::Managed).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Agent, "reviewer", Scope::Project).state,
            State::Shadowed
        );
        assert_eq!(
            find(&map, Kind::Agent, "x:y", Scope::Project).state,
            State::NotLoaded
        );
    }

    #[test]
    fn claude_md_excludes_and_agents_md_mode() {
        let f = fixture();
        write(&f.project.join("CLAUDE.md"), "# Repo\n");
        write(&f.project.join("AGENTS.md"), "# Agents\n");
        write(
            &f.project.join(".claude").join("rules").join("x.md"),
            "# X\n",
        );
        write(
            &f.project.join(".claude").join("settings.json"),
            r#"{"claudeMdExcludes": ["**/repo/CLAUDE.md", "**/rules/x.md"]}"#,
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Instructions, "CLAUDE.md", Scope::Project).state,
            State::NotLoaded
        );
        assert_eq!(
            find(&map, Kind::Rule, "x", Scope::Project).state,
            State::NotLoaded
        );
        // With the only CLAUDE.md excluded, AGENTS.md is read instead.
        assert_eq!(
            find(&map, Kind::Instructions, "AGENTS.md", Scope::Project).state,
            State::Active
        );

        write(
            &f.env.config_dir.join("settings.json"),
            r#"{"pluginConfigs": {"cc-plugin-agents-md@builtin": {"options": {"instructionFiles": "claude-md"}}}}"#,
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Instructions, "AGENTS.md", Scope::Project).state,
            State::NotLoaded
        );
    }

    #[test]
    fn managed_policies_lock_mcp_and_customizations() {
        let f = fixture();
        write(
            &f.project.join(".mcp.json"),
            r#"{"mcpServers": {"db": {}}}"#,
        );
        write(
            &f.project
                .join(".claude")
                .join("skills")
                .join("s")
                .join("SKILL.md"),
            "---\nname: s\n---\n",
        );
        write(
            &f.env.managed_dir.join("managed-mcp.json"),
            r#"{"mcpServers": {}}"#,
        );
        write(
            &f.env.managed_dir.join("managed-settings.json"),
            r#"{"strictPluginOnlyCustomization": ["skills"]}"#,
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Mcp, "db", Scope::Project).state,
            State::NotLoaded
        );
        assert_eq!(
            find(&map, Kind::Skill, "s", Scope::Project).state,
            State::Off
        );
    }

    #[test]
    fn prefix_skill_deny_bom_settings_and_committed_mcp_approval() {
        let f = fixture();
        for name in ["deploy", "deploy-prod", "lint"] {
            write(
                &f.project
                    .join(".claude")
                    .join("skills")
                    .join(name)
                    .join("SKILL.md"),
                &format!("---\nname: {name}\n---\n"),
            );
        }
        write(
            &f.project.join(".mcp.json"),
            r#"{"mcpServers": {"db": {}}}"#,
        );
        write(
            &f.project.join(".claude").join("settings.json"),
            "\u{feff}{\"permissions\": {\"deny\": [\"Skill(deploy *)\"]}, \"enabledMcpjsonServers\": [\"db\"]}",
        );
        let map = scan(&f.project, &f.env);
        assert_eq!(
            find(&map, Kind::Settings, "settings.json", Scope::Project).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Skill, "deploy", Scope::Project).state,
            State::Off
        );
        assert_eq!(
            find(&map, Kind::Skill, "deploy-prod", Scope::Project).state,
            State::Off
        );
        assert_eq!(
            find(&map, Kind::Skill, "lint", Scope::Project).state,
            State::Active
        );
        assert_eq!(
            find(&map, Kind::Mcp, "db", Scope::Project).state,
            State::Limited
        );
    }

    #[test]
    fn project_at_home_does_not_list_user_config_twice() {
        let f = fixture();
        let home = f.env.config_dir.parent().unwrap().to_path_buf();
        write(
            &f.env
                .config_dir
                .join("skills")
                .join("mine")
                .join("SKILL.md"),
            "---\nname: mine\n---\n",
        );
        let map = scan(&home, &f.env);
        let mine: Vec<_> = map.items.iter().filter(|i| i.name == "mine").collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].state, State::Active);
    }
}
