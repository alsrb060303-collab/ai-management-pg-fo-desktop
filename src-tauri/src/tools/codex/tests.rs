use std::fs;
use std::path::{Path, PathBuf};

use super::{Env, scan};
use crate::tools::{ConfigMap, Item, Kind, Scope, State};

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
    let home = root.join("home");
    let env = Env {
        codex_home: home.join(".codex"),
        home,
        system_dir: Some(root.join("etc")),
        managed_dir: root.join("managed"),
    };
    fs::create_dir_all(&env.codex_home).unwrap();
    Fixture {
        _tmp: tmp,
        project,
        env,
    }
}

impl Fixture {
    /// Writes the user config; `trust` adds a `[projects]` entry for the project.
    fn user_config(&self, trust: Option<&str>, extra: &str) {
        let mut text = extra.to_string();
        if let Some(level) = trust {
            text.push_str(&format!(
                "\n[projects.'{}']\ntrust_level = \"{level}\"\n",
                self.project.display()
            ));
        }
        write(&self.env.codex_home.join("config.toml"), &text);
    }

    fn codex(&self) -> PathBuf {
        self.project.join(".codex")
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
        .unwrap_or_else(|| panic!("missing {kind:?} {name} {scope:?}: {:#?}", map.items))
}

fn find_path<'a>(map: &'a ConfigMap, kind: Kind, path: &Path) -> &'a Item {
    let path = path.display().to_string();
    map.items
        .iter()
        .find(|i| i.kind == kind && i.path == path)
        .unwrap_or_else(|| panic!("missing {kind:?} {path}: {:#?}", map.items))
}

fn detail(item: &Item) -> &str {
    item.detail.as_deref().unwrap_or_default()
}

fn skill(dir: &Path, folder: &str, text: &str) -> PathBuf {
    let path = dir.join(folder).join("SKILL.md");
    write(&path, text);
    path
}

const AGENT: &str = "name = \"reviewer\"\ndescription = \"Reviews\"\ndeveloper_instructions = \"Review it\"\nmodel = \"gpt-5\"\n";

#[test]
fn global_override_wins_unless_empty() {
    let f = fixture();
    write(&f.env.codex_home.join("AGENTS.md"), "# Global\nhi\n");
    write(&f.env.codex_home.join("AGENTS.override.md"), "# Override\n");
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Instructions, "AGENTS.override.md", Scope::User).state,
        State::Active
    );
    let plain = find(&map, Kind::Instructions, "AGENTS.md", Scope::User);
    assert_eq!(plain.state, State::NotLoaded);
    assert!(detail(plain).contains("AGENTS.override.md가 우선"));

    write(&f.env.codex_home.join("AGENTS.override.md"), "  \n");
    let map = scan(&f.project, &f.env);
    let over = find(&map, Kind::Instructions, "AGENTS.override.md", Scope::User);
    assert_eq!(over.state, State::NotLoaded);
    assert!(detail(over).contains("빈 파일"));
    let plain = find(&map, Kind::Instructions, "AGENTS.md", Scope::User);
    assert_eq!(plain.state, State::Active);
    assert_eq!(plain.description.as_deref(), Some("Global"));
    assert!(detail(plain).starts_with("2줄 · "));
}

#[test]
fn one_doc_per_folder_from_root_down_with_fallbacks() {
    let f = fixture();
    let web = f.project.join("packages").join("web");
    let pkgs = f.project.join("packages");
    fs::create_dir_all(&web).unwrap();
    write(&f.project.join("AGENTS.md"), "# Repo\n");
    write(&pkgs.join("TEAM.md"), "# Team\n");
    write(&pkgs.join("NOTES.md"), "# Notes\n");
    write(&web.join("AGENTS.override.md"), "# Mine\n");
    write(&web.join("AGENTS.md"), "# Web\n");
    // Above the repository root: not read.
    write(
        &f.project.parent().unwrap().join("AGENTS.md"),
        "# Outside\n",
    );
    f.user_config(
        None,
        "project_doc_fallback_filenames = [\"a/b.md\", \"..\", \"\", \"TEAM.md\", \"NOTES.md\"]\n",
    );

    let map = scan(&web, &f.env);
    assert_eq!(map.repo_root.as_deref(), Some(f.project.to_str().unwrap()));
    assert_eq!(
        find_path(&map, Kind::Instructions, &f.project.join("AGENTS.md")).scope,
        Scope::Parent
    );
    assert_eq!(
        find(&map, Kind::Instructions, "TEAM.md", Scope::Parent).state,
        State::Active
    );
    let notes = find(&map, Kind::Instructions, "NOTES.md", Scope::Parent);
    assert_eq!(notes.state, State::NotLoaded);
    assert!(detail(notes).contains("같은 폴더의 TEAM.md가 우선"));
    assert_eq!(
        find(&map, Kind::Instructions, "AGENTS.override.md", Scope::Local).state,
        State::Active
    );
    assert_eq!(
        find_path(&map, Kind::Instructions, &web.join("AGENTS.md")).state,
        State::NotLoaded
    );
    assert!(
        !map.items
            .iter()
            .any(|i| i.description.as_deref() == Some("Outside"))
    );
}

#[test]
fn byte_budget_cuts_and_drops_later_docs() {
    let f = fixture();
    let a = f.project.join("a");
    let b = a.join("b");
    let c = b.join("c");
    fs::create_dir_all(&c).unwrap();
    write(&f.project.join("AGENTS.md"), "123456"); // 6 bytes
    write(&a.join("AGENTS.md"), " \n\n"); // whitespace only: skipped, not counted
    write(&b.join("AGENTS.md"), "abcdefgh"); // crosses 10
    write(&c.join("AGENTS.md"), "late");
    f.user_config(None, "project_doc_max_bytes = 10\n");

    let map = scan(&c, &f.env);
    let doc = |dir: &Path| find_path(&map, Kind::Instructions, &dir.join("AGENTS.md"));
    assert_eq!(doc(&f.project).state, State::Active);
    assert_eq!(doc(&a).state, State::NotLoaded);
    let cut = doc(&b);
    assert_eq!(cut.state, State::Limited);
    assert!(detail(cut).contains("10바이트 초과분은 잘림"));
    assert!(detail(cut).contains("4바이트만"));
    assert_eq!(doc(&c).state, State::NotLoaded);
    assert_eq!(doc(&c).scope, Scope::Project);
}

#[test]
fn empty_root_markers_read_only_the_project_folder() {
    let f = fixture();
    let sub = f.project.join("sub");
    fs::create_dir_all(&sub).unwrap();
    write(&f.project.join("AGENTS.md"), "# Repo\n");
    write(&sub.join("AGENTS.md"), "# Sub\n");
    f.user_config(None, "project_root_markers = []\n");
    let map = scan(&sub, &f.env);
    assert_eq!(map.repo_root, None);
    let docs: Vec<_> = map
        .items
        .iter()
        .filter(|i| i.kind == Kind::Instructions)
        .collect();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].description.as_deref(), Some("Sub"));
}

#[test]
fn trust_gates_project_layers() {
    let f = fixture();
    write(&f.project.join("AGENTS.md"), "# Repo\n");
    write(&f.codex().join("config.toml"), "model = \"gpt-5\"\n");
    write(&f.codex().join("agents").join("reviewer.toml"), AGENT);
    write(
        &f.codex().join("rules").join("git.rules"),
        "prefix_rule()\n",
    );
    write(&f.codex().join("hooks.json"), r#"{"hooks": {"Stop": []}}"#);

    // Unknown: AGENTS.md loads, the rest waits for trust.
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Instructions, "AGENTS.md", Scope::Project).state,
        State::Active
    );
    let config = find(&map, Kind::Settings, "config.toml", Scope::Project);
    assert_eq!(config.state, State::Limited);
    assert!(detail(config).contains("신뢰 여부를 물어봄"));
    assert_eq!(
        find(&map, Kind::Agent, "reviewer", Scope::Project).state,
        State::Limited
    );
    assert_eq!(
        find(&map, Kind::Policy, "git.rules", Scope::Project).state,
        State::Limited
    );
    assert!(map.notes[0].contains("신뢰 여부가 아직 없습니다"));

    f.user_config(Some("untrusted"), "");
    let map = scan(&f.project, &f.env);
    let doc = find(&map, Kind::Instructions, "AGENTS.md", Scope::Project);
    assert_eq!(doc.state, State::NotLoaded);
    assert!(detail(doc).contains("untrusted"));
    for (kind, name) in [
        (Kind::Settings, "config.toml"),
        (Kind::Agent, "reviewer"),
        (Kind::Policy, "git.rules"),
        (Kind::Hook, "hooks.json"),
    ] {
        assert_eq!(
            find(&map, kind, name, Scope::Project).state,
            State::NotLoaded,
            "{name}"
        );
    }
    assert!(map.notes[0].contains("untrusted"));

    f.user_config(Some("trusted"), "");
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Settings, "config.toml", Scope::Project).state,
        State::Active
    );
    let agent = find(&map, Kind::Agent, "reviewer", Scope::Project);
    assert_eq!(agent.state, State::Active);
    assert_eq!(agent.detail.as_deref(), Some("model: gpt-5"));
    let hook = find(&map, Kind::Hook, "hooks.json", Scope::Project);
    assert_eq!(hook.state, State::Active);
    assert_eq!(hook.detail.as_deref(), Some("Stop"));
}

#[test]
fn trust_falls_back_to_the_repo_root() {
    let f = fixture();
    let sub = f.project.join("sub");
    write(&sub.join(".codex").join("config.toml"), "");
    write(
        &f.env.codex_home.join("config.toml"),
        &format!(
            "[projects.'{}']\ntrust_level = \"trusted\"\n",
            f.project.display()
        ),
    );
    let map = scan(&sub, &f.env);
    assert_eq!(
        find(&map, Kind::Settings, "config.toml", Scope::Project).state,
        State::Active
    );
}

#[test]
fn claude_md_is_not_read_unless_a_fallback() {
    let f = fixture();
    write(&f.project.join("CLAUDE.md"), "# Claude\n");
    write(&f.project.join("CLAUDE.local.md"), "# Mine\n");
    let map = scan(&f.project, &f.env);
    let claude = find(&map, Kind::Instructions, "CLAUDE.md", Scope::Project);
    assert_eq!(claude.state, State::NotLoaded);
    assert!(detail(claude).contains("project_doc_fallback_filenames"));
    assert_eq!(
        find(&map, Kind::Instructions, "CLAUDE.local.md", Scope::Local).state,
        State::NotLoaded
    );

    f.user_config(None, "project_doc_fallback_filenames = [\"CLAUDE.md\"]\n");
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Instructions, "CLAUDE.md", Scope::Project).state,
        State::Active
    );
}

#[test]
fn skills_roots_validation_and_toggles() {
    let f = fixture();
    let repo_skills = f.project.join(".agents").join("skills");
    let user_skills = f.env.home.join(".agents").join("skills");
    let old_skills = f.env.codex_home.join("skills");
    let ok = |name: &str| format!("---\nname: {name}\ndescription: {name} skill\n---\n");

    let deploy = skill(&repo_skills, "deploy", &ok("deploy"));
    skill(&repo_skills, "lint", &ok("lint"));
    skill(&repo_skills, "nodesc", "---\nname: nodesc\n---\n");
    skill(&repo_skills, "nofm", "# Just text\n");
    skill(&repo_skills, ".hidden", &ok("hidden"));
    skill(&repo_skills, "folder-name", "---\ndescription: d\n---\n");
    skill(&f.codex().join("skills"), "layer", &ok("layer"));
    skill(&user_skills, "deploy", &ok("deploy"));
    skill(&old_skills, "legacy", &ok("legacy"));
    skill(
        &f.env.system_dir.clone().unwrap().join("skills"),
        "org",
        &ok("org"),
    );
    f.user_config(
        None,
        &format!(
            "[[skills.config]]\npath = '{}'\nenabled = false\n\n\
             [[skills.config]]\nname = \"lint\"\nenabled = false\n\n\
             [[skills.config]]\nname = \"lint\"\nenabled = true\n",
            deploy.display()
        ),
    );
    write(
        &f.codex().join("config.toml"),
        "[[skills.config]]\nname = \"layer\"\nenabled = false\n",
    );

    let map = scan(&f.project, &f.env);
    let project_deploy = find(&map, Kind::Skill, "deploy", Scope::Project);
    assert_eq!(project_deploy.state, State::Off);
    assert!(
        detail(project_deploy).starts_with("skills.config: enabled = false (~/.codex/config.toml)")
    );
    // Same-named skills are both kept.
    let user_deploy = find(&map, Kind::Skill, "deploy", Scope::User);
    assert_eq!(user_deploy.state, State::Active);
    assert!(detail(user_deploy).contains("같은 이름의 skill이 또 있음"));
    assert_eq!(
        find(&map, Kind::Skill, "lint", Scope::Project).state,
        State::Active
    );
    assert_eq!(
        find(&map, Kind::Skill, "nodesc", Scope::Project).state,
        State::NotLoaded
    );
    assert_eq!(
        find(&map, Kind::Skill, "nofm", Scope::Project).state,
        State::NotLoaded
    );
    assert_eq!(
        find(&map, Kind::Skill, "folder-name", Scope::Project).state,
        State::Active
    );
    // Only the user config turns skills off.
    assert_eq!(
        find(&map, Kind::Skill, "layer", Scope::Project).state,
        State::Active
    );
    assert!(map.notes.iter().any(|n| n.contains("[[skills.config]]")));
    let legacy = find(&map, Kind::Skill, "legacy", Scope::User);
    assert!(detail(legacy).contains("예전 위치"));
    assert_eq!(
        find(&map, Kind::Skill, "org", Scope::Managed).state,
        State::Active
    );
    assert!(!map.items.iter().any(|i| i.name == "hidden"));
}

#[test]
fn project_at_home_lists_user_skills_once() {
    let f = fixture();
    fs::create_dir_all(f.env.home.join(".git")).unwrap();
    skill(
        &f.env.home.join(".agents").join("skills"),
        "mine",
        "---\nname: mine\ndescription: d\n---\n",
    );
    let map = scan(&f.env.home, &f.env);
    let mine: Vec<_> = map.items.iter().filter(|i| i.name == "mine").collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].scope, Scope::User);
    // ~/.codex is the user folder, not a project layer.
    assert!(
        !map.items
            .iter()
            .any(|i| i.kind == Kind::Settings && i.scope == Scope::Project)
    );
}

#[test]
fn agents_validation_precedence_and_master_switch() {
    let f = fixture();
    let user_agents = f.env.codex_home.join("agents");
    write(&user_agents.join("reviewer.toml"), AGENT);
    write(
        &user_agents.join("half.toml"),
        "name = \"half\"\ndescription = \" \"\n",
    );
    write(&user_agents.join("broken.toml"), "name = ");
    write(
        &f.env.codex_home.join("roles").join("planner.toml"),
        "model = \"o3\"\ndeveloper_instructions = \"Plan\"\n",
    );
    write(&f.codex().join("agents").join("reviewer.toml"), AGENT);
    f.user_config(
        Some("trusted"),
        "[agents.planner]\ndescription = \"Plans\"\nconfig_file = \"roles/planner.toml\"\n\n\
         [agents.ghost]\ndescription = \"x\"\nconfig_file = \"roles/missing.toml\"\n",
    );

    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Agent, "reviewer", Scope::Project).state,
        State::Active
    );
    assert_eq!(
        find(&map, Kind::Agent, "reviewer", Scope::User).state,
        State::Shadowed
    );
    let half = find(&map, Kind::Agent, "half", Scope::User);
    assert_eq!(half.state, State::NotLoaded);
    assert!(detail(half).contains("description, developer_instructions"));
    let broken = find(&map, Kind::Agent, "broken.toml", Scope::User);
    assert_eq!(broken.state, State::NotLoaded);
    assert!(detail(broken).starts_with("TOML 오류"));
    let planner = find(&map, Kind::Agent, "planner", Scope::User);
    assert_eq!(planner.state, State::Active);
    assert_eq!(planner.description.as_deref(), Some("Plans"));
    assert_eq!(planner.detail.as_deref(), Some("model: o3"));
    assert_eq!(
        find(&map, Kind::Agent, "ghost", Scope::User).state,
        State::NotLoaded
    );

    f.user_config(Some("trusted"), "[agents]\nenabled = false\n");
    let map = scan(&f.project, &f.env);
    let off = find(&map, Kind::Agent, "reviewer", Scope::Project);
    assert_eq!(off.state, State::Off);
    assert!(detail(off).starts_with("agents.enabled = false"));
}

#[test]
fn mcp_project_overrides_user_and_enabled_false() {
    let f = fixture();
    f.user_config(
        Some("trusted"),
        "[mcp_servers.db]\ncommand = \"npx\"\nargs = [\"-y\", \"db-mcp\"]\n\n\
         [mcp_servers.docs]\nurl = \"https://example.com/mcp\"\nenabled = false\n",
    );
    write(
        &f.codex().join("config.toml"),
        "[mcp_servers.db]\ncommand = \"./db\"\n",
    );
    let map = scan(&f.project, &f.env);
    let project_db = find(&map, Kind::Mcp, "db", Scope::Project);
    assert_eq!(project_db.state, State::Active);
    assert_eq!(project_db.detail.as_deref(), Some("./db"));
    let user_db = find(&map, Kind::Mcp, "db", Scope::User);
    assert_eq!(user_db.state, State::Shadowed);
    assert!(detail(user_db).starts_with("npx -y db-mcp"));
    let docs = find(&map, Kind::Mcp, "docs", Scope::User);
    assert_eq!(docs.state, State::Off);
    assert!(detail(docs).contains("https://example.com/mcp"));

    // Without trust the project server waits, and the user one stays active.
    f.user_config(None, "[mcp_servers.db]\ncommand = \"npx\"\n");
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Mcp, "db", Scope::Project).state,
        State::Limited
    );
    assert_eq!(
        find(&map, Kind::Mcp, "db", Scope::User).state,
        State::Active
    );
}

#[test]
fn settings_prompts_rules_hooks() {
    let f = fixture();
    let system = f.env.system_dir.clone().unwrap();
    write(&system.join("config.toml"), "model = \"gpt-5\"\n");
    let legacy = if cfg!(windows) {
        f.env.codex_home.clone()
    } else {
        system.clone()
    };
    write(&legacy.join("managed_config.toml"), "");
    write(
        &f.env.managed_dir.join("requirements.toml"),
        "allowed_approval_policies = []\n",
    );
    f.user_config(None, "model = \n");
    write(&f.env.codex_home.join("auth.json"), "{\"secret\": true}");
    write(
        &f.env.codex_home.join("prompts").join("fix.md"),
        "# Fix things\n",
    );
    write(&f.env.codex_home.join("rules").join("default.rules"), "");
    write(&f.env.codex_home.join("rules").join("notes.txt"), "");
    write(&f.env.codex_home.join("hooks.json"), "{ nope");

    let map = scan(&f.project, &f.env);
    let user = find(&map, Kind::Settings, "config.toml", Scope::User);
    assert_eq!(user.state, State::Off);
    assert!(detail(user).starts_with("TOML 오류"));
    for name in ["config.toml", "requirements.toml", "managed_config.toml"] {
        assert_eq!(
            find(&map, Kind::Settings, name, Scope::Managed).state,
            State::Active
        );
    }
    let prompt = find(&map, Kind::Command, "/prompts:fix", Scope::User);
    assert_eq!(prompt.state, State::Limited);
    assert_eq!(prompt.description.as_deref(), Some("Fix things"));
    let rule = find(&map, Kind::Policy, "default.rules", Scope::User);
    assert_eq!(rule.state, State::Active);
    assert!(detail(rule).contains("지침 아님"));
    assert!(!map.items.iter().any(|i| i.name == "notes.txt"));
    assert_eq!(
        find(&map, Kind::Hook, "hooks.json", Scope::User).state,
        State::Off
    );
    assert!(!map.items.iter().any(|i| i.path.ends_with("auth.json")));
    assert!(map.notes.iter().any(|n| n.contains("auth.json")));

    f.user_config(Some("trusted"), "[hooks]\nPreToolUse = []\n");
    write(&f.codex().join("config.toml"), "[hooks]\nStop = []\n");
    let map = scan(&f.project, &f.env);
    assert_eq!(
        find(&map, Kind::Hook, "config.toml [hooks]", Scope::User)
            .detail
            .as_deref(),
        Some("PreToolUse")
    );
    assert_eq!(
        find(&map, Kind::Hook, "config.toml [hooks]", Scope::Project).state,
        State::Active
    );
}
