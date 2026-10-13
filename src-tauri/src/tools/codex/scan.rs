use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item as TomlItem};

use super::Env;
use super::util::{get, get_str, get_strings, norm, read_toml};
use super::{extensions, instructions};
use crate::tools::{ConfigMap, Item, Kind, Scope, State, ToolId};

pub fn scan(project: &Path, env: &Env) -> ConfigMap {
    let mut items = Vec::new();
    let ctx = Ctx::load(project, env, &mut items);

    instructions::instructions(&ctx, &mut items);
    let mut notes = vec![ctx.trust.note()];
    extensions::skills(&ctx, &mut items, &mut notes);
    extensions::agents(&ctx, &mut items);
    extensions::mcp(&ctx, &mut items);
    extensions::prompts(&ctx, &mut items);
    extensions::policies(&ctx, &mut items);
    extensions::hooks(&ctx, &mut items);

    notes.extend([
        crate::tr!(
            "로그인 정보(auth.json)는 읽지 않습니다.",
            "Sign-in data (auth.json) isn't read."
        ),
        crate::tr!(
            "--profile로 고르는 프로필 설정과 클라우드에서 관리하는 설정은 반영하지 않습니다.",
            "Profiles chosen with --profile and cloud-managed settings aren't reflected."
        ),
        crate::tr!(
            "Codex에 내장된 skill은 표시하지 않습니다.",
            "Skills built into Codex aren't shown."
        ),
    ]);
    if cfg!(windows) {
        notes.push(crate::tr!(
            "MDM으로 배포된 관리 정책은 표시하지 않습니다.",
            "Managed policies deployed through MDM aren't shown."
        ));
    }

    ConfigMap {
        tool: ToolId::Codex,
        project: project.display().to_string(),
        repo_root: ctx.root.as_ref().map(|p| p.display().to_string()),
        items,
        notes,
    }
}

/// Whether the user trusted the project (`[projects."<path>"] trust_level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    Untrusted,
    Unknown,
}

impl Trust {
    fn note(self) -> String {
        match self {
            Trust::Trusted => crate::tr!(
                "이 프로젝트는 신뢰함(trusted)으로 표시되어 있어 프로젝트의 .codex 설정이 적용됩니다.",
                "This project is marked trusted, so its .codex settings apply."
            ),
            Trust::Untrusted => crate::tr!(
                "이 프로젝트는 신뢰하지 않음(untrusted)으로 표시되어 있어 프로젝트의 .codex 설정과 AGENTS.md를 읽지 않습니다.",
                "This project is marked untrusted, so its .codex settings and AGENTS.md files aren't read."
            ),
            Trust::Unknown => crate::tr!(
                "이 프로젝트의 신뢰 여부가 아직 없습니다. Codex가 처음 열 때 물어보고, 신뢰해야 프로젝트의 .codex 설정이 적용됩니다.",
                "This project hasn't been marked trusted or untrusted yet. Codex asks the first time it opens it; its .codex settings apply only once trusted."
            ),
        }
    }
}

/// A project config layer: a `<dir>/.codex` folder between the project root
/// and the project folder.
pub struct Layer {
    /// The `.codex` folder.
    pub dir: PathBuf,
    pub config_path: PathBuf,
    /// The parsed `config.toml`, when it exists and parses.
    pub config: Option<DocumentMut>,
}

pub struct Ctx<'a> {
    pub project: &'a Path,
    pub env: &'a Env,
    /// Nearest ancestor with a `project_root_markers` entry.
    pub root: Option<PathBuf>,
    /// `$CODEX_HOME/config.toml`, when it parses.
    pub user: Option<DocumentMut>,
    pub user_path: PathBuf,
    /// Root first, project folder last.
    pub layers: Vec<Layer>,
    pub trust: Trust,
}

impl<'a> Ctx<'a> {
    /// Reads the config files and lists them as settings items.
    fn load(project: &'a Path, env: &'a Env, items: &mut Vec<Item>) -> Self {
        // Admin defaults and managed requirements.
        let legacy_dir = if cfg!(windows) {
            Some(env.codex_home.clone())
        } else {
            env.system_dir.clone()
        };
        let managed = [
            (
                env.system_dir.as_ref().map(|d| d.join("config.toml")),
                crate::tr!(
                    "시스템 기본값 · 개인·프로젝트 설정이 덮어씀",
                    "System defaults · user and project settings override them"
                ),
            ),
            (
                Some(env.managed_dir.join("requirements.toml")),
                crate::tr!(
                    "관리 정책 · 사용자가 바꿀 수 없는 제한",
                    "Managed policy · limits users can't change"
                ),
            ),
            (
                legacy_dir.map(|d| d.join("managed_config.toml")),
                crate::tr!(
                    "예전 관리 설정 파일 · 다른 설정보다 우선",
                    "Legacy managed config · takes precedence over other settings"
                ),
            ),
        ];
        for (path, detail) in managed {
            if let Some(path) = path {
                settings_item(items, Scope::Managed, &path, read_toml(&path), detail, None);
            }
        }

        let user_path = env.codex_home.join("config.toml");
        let user_cfg = read_toml(&user_path);
        let user = user_cfg.as_ref().and_then(|r| r.as_ref().ok()).cloned();
        settings_item(
            items,
            Scope::User,
            &user_path,
            user_cfg,
            crate::tr!(
                "개인 설정 · 모든 프로젝트에 적용",
                "User settings · apply to every project"
            ),
            None,
        );

        let root_item = user.as_ref().map(DocumentMut::as_item);
        let markers = root_item
            .and_then(|u| get_strings(u, &["project_root_markers"]))
            .unwrap_or_else(|| vec![".git".to_string()]);
        let root = find_root(project, &markers);
        let trust = root_item.map_or(Trust::Unknown, |u| trust_of(u, project, root.as_deref()));

        let codex_home = norm(&env.codex_home);
        let mut layers = Vec::new();
        for dir in project_dirs(project, root.as_deref()) {
            let codex = dir.join(".codex");
            if !codex.is_dir() || norm(&codex) == codex_home {
                continue;
            }
            let scope = if dir == project {
                Scope::Project
            } else {
                Scope::Parent
            };
            let config_path = codex.join("config.toml");
            let parsed = read_toml(&config_path);
            let config = parsed.as_ref().and_then(|r| r.as_ref().ok()).cloned();
            settings_item(
                items,
                scope,
                &config_path,
                parsed,
                crate::tr!(
                    "프로젝트 설정 · 개인 설정을 덮어씀, 가까운 폴더 쪽이 우선",
                    "Project settings · override user settings; the nearest folder wins"
                ),
                trust_gate(trust),
            );
            layers.push(Layer {
                dir: codex,
                config_path,
                config,
            });
        }

        Ctx {
            project,
            env,
            root,
            user,
            user_path,
            layers,
            trust,
        }
    }

    /// The user config as a TOML item, for key lookups.
    pub fn user_item(&self) -> Option<&TomlItem> {
        self.user.as_ref().map(DocumentMut::as_item)
    }

    /// Folders from the project root down to the project folder.
    pub fn project_dirs(&self) -> Vec<PathBuf> {
        project_dirs(self.project, self.root.as_deref())
    }

    /// State for items from project config layers, unless trusted.
    pub fn gate(&self) -> Option<(State, String)> {
        trust_gate(self.trust)
    }

    /// `~/.codex/config.toml` as shown in details.
    pub fn user_label(&self) -> String {
        let home = norm(&self.env.home);
        match norm(&self.user_path).strip_prefix(&home) {
            Ok(rel) => format!("~/{}", rel.to_string_lossy().replace('\\', "/")),
            Err(_) => self.user_path.display().to_string(),
        }
    }
}

/// Why items from project config layers don't (yet) apply.
pub fn trust_gate(trust: Trust) -> Option<(State, String)> {
    match trust {
        Trust::Trusted => None,
        Trust::Untrusted => Some((State::NotLoaded, untrusted_detail())),
        Trust::Unknown => Some((
            State::Limited,
            crate::tr!(
                "Codex가 처음 열 때 신뢰 여부를 물어봄 · 신뢰해야 적용",
                "Codex asks whether to trust the project when it first opens it · applies once trusted"
            ),
        )),
    }
}

pub fn untrusted_detail() -> String {
    crate::tr!(
        "프로젝트가 신뢰하지 않음(untrusted)으로 표시되어 읽지 않음",
        "Not read: the project is marked untrusted"
    )
}

fn settings_item(
    items: &mut Vec<Item>,
    scope: Scope,
    path: &Path,
    parsed: Option<Result<DocumentMut, String>>,
    detail: String,
    gate: Option<(State, String)>,
) {
    let Some(parsed) = parsed else { return };
    let (state, detail) = match parsed {
        Err(e) => (State::Off, crate::tr!("TOML 오류: {e}", "TOML error: {e}")),
        Ok(_) => match gate {
            Some((state, why)) => (state, format!("{detail} · {why}")),
            None => (State::Active, detail),
        },
    };
    items.push(Item {
        kind: Kind::Settings,
        scope,
        name: super::util::file_name(path),
        path: path.display().to_string(),
        description: None,
        state,
        detail: Some(detail),
    });
}

/// Nearest ancestor of `project` (itself included) holding one of `markers`.
/// No markers means no search: the project folder stands alone.
fn find_root(project: &Path, markers: &[String]) -> Option<PathBuf> {
    if markers.is_empty() {
        return None;
    }
    project
        .ancestors()
        .find(|dir| markers.iter().any(|m| dir.join(m).exists()))
        .map(Path::to_path_buf)
}

/// From `root` down to `project`, or just `project` without a root.
fn project_dirs(project: &Path, root: Option<&Path>) -> Vec<PathBuf> {
    let Some(root) = root else {
        return vec![project.to_path_buf()];
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in project.ancestors() {
        dirs.push(dir.to_path_buf());
        if dir == root {
            break;
        }
    }
    dirs.reverse();
    dirs
}

/// `trust_level` for the project folder, else for the project root. Codex
/// writes the keys resolved (and lowercased on Windows).
fn trust_of(user: &TomlItem, project: &Path, root: Option<&Path>) -> Trust {
    let Some(projects) = get(user, &["projects"]).and_then(TomlItem::as_table_like) else {
        return Trust::Unknown;
    };
    let entries: Vec<(PathBuf, Option<&str>)> = projects
        .iter()
        .map(|(key, value)| (norm(Path::new(key)), get_str(value, &["trust_level"])))
        .collect();
    let lookup = |dir: &Path| {
        let dir = norm(dir);
        entries
            .iter()
            .find(|(key, _)| *key == dir)
            .and_then(|(_, level)| *level)
    };
    let level = lookup(project).or_else(|| root.and_then(lookup));
    match level {
        Some("trusted") => Trust::Trusted,
        Some("untrusted") => Trust::Untrusted,
        _ => Trust::Unknown,
    }
}
