//! Personal library of skills and subagents that can be applied to (copied
//! into) projects, plus per-project on/off toggles in the tool's settings.
//!
//! Library layout: `<dir>/skills/<id>/SKILL.md` and `<dir>/agents/<id>.md`.
//! Every operation works for one tool (`ToolId`): where copies go and how a
//! toggle is written depend on it. Codex subagents are converted to TOML
//! (`codex.rs`). Copies the app made are recorded with their hash in
//! `applied.json` in the app data folder, so a copy the user edited is never
//! removed or overwritten without asking.

mod codex;
pub(crate) mod ops;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::tools::{ToolId, frontmatter};
use ops::Change;

/// Error prefix the UI uses to ask before discarding the user's edits.
const MODIFIED_ERROR: &str = "MODIFIED:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Skill,
    Agent,
}

/// Who an applied copy or a toggle is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Kept out of git (`.git/info/exclude`, `settings.local.json`).
    Personal,
    /// Meant to be committed (`settings.json`).
    Team,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplyStatus {
    /// Applied, unchanged on both sides.
    Current,
    /// The library version changed since it was applied.
    Outdated,
    /// The project copy was edited.
    Modified,
    /// Recorded as applied but the project copy is gone.
    Missing,
    /// Not applied by us, but the project already has something with this name.
    Conflict,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    project: String,
    /// Records from before tools existed are Claude Code's.
    #[serde(default)]
    tool: ToolId,
    kind: ItemKind,
    id: String,
    mode: Mode,
    /// Hash of the project copy as the app wrote it.
    hash: String,
    /// Hash of the library source it was made from. Differs from `hash` when
    /// the copy is converted (Codex subagents); older records lack it, and
    /// their copy equals the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    src_hash: Option<String>,
}

impl Record {
    fn is(&self, project: &str, tool: ToolId, kind: ItemKind, id: &str) -> bool {
        self.project == project && self.tool == tool && self.kind == kind && self.id == id
    }
}

/// A skill or subagent this app turned off, so turning it back on only undoes
/// what the app wrote.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Toggle {
    project: String,
    /// Toggles from before tools existed are Claude Code's.
    #[serde(default)]
    tool: ToolId,
    kind: ItemKind,
    /// Name as the tool sees it (frontmatter `name`).
    name: String,
    mode: Mode,
    /// `skillOverrides` value before we set "off", restored on enable.
    previous: Option<String>,
    /// Codex: the SKILL.md path written to `[[skills.config]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl Toggle {
    /// Codex toggles are per SKILL.md path (two skills can share a name);
    /// Claude Code's are per name.
    fn is(&self, project: &str, target: &Target) -> bool {
        self.project == project
            && self.tool == target.tool
            && self.kind == target.kind
            && match (&self.path, target.path) {
                (Some(a), Some(b)) => a == b,
                _ => self.name == target.name,
            }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Applied {
    copies: Vec<Record>,
    toggles: Vec<Toggle>,
}

/// `applied.json` from before toggles were tracked was a bare list of copies.
#[derive(Deserialize)]
#[serde(untagged)]
enum AppliedFile {
    Current(Applied),
    Legacy(Vec<Record>),
}

impl AppliedFile {
    fn into_applied(self) -> Applied {
        match self {
            AppliedFile::Current(a) => a,
            AppliedFile::Legacy(copies) => Applied {
                copies,
                toggles: Vec::new(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggledOff {
    pub kind: ItemKind,
    pub name: String,
    pub mode: Mode,
    /// Codex: the SKILL.md path that was turned off.
    pub path: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    library_dir: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub kind: ItemKind,
    /// Folder (skill) or file stem (agent); what gets copied.
    pub id: String,
    /// Name from frontmatter, as the tool sees it.
    pub name: String,
    pub description: Option<String>,
    pub path: String,
    pub mode: Option<Mode>,
    pub status: Option<ApplyStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub dir: String,
    pub is_default: bool,
    pub items: Vec<LibraryItem>,
    /// Items this app turned off in the selected project.
    pub toggled_off: Vec<ToggledOff>,
}

pub struct Library {
    data_dir: PathBuf,
    state: Mutex<State>,
}

struct State {
    config: Config,
    records: Vec<Record>,
    toggles: Vec<Toggle>,
}

impl Library {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let data_dir = app.path().app_data_dir()?;
        let config = read_json(&data_dir.join("config.json")).unwrap_or_default();
        let applied = read_json::<AppliedFile>(&data_dir.join("applied.json"))
            .map(AppliedFile::into_applied)
            .unwrap_or_default();
        app.manage(Library {
            data_dir,
            state: Mutex::new(State {
                config,
                records: applied.copies,
                toggles: applied.toggles,
            }),
        });
        Ok(())
    }

    /// The current library folder's items, for the composer's block catalog.
    pub(crate) fn items(&self) -> Vec<LibraryItem> {
        let dir = self.dir(&self.lock());
        list(&dir)
    }

    /// The library folder in use (what sync reads and writes).
    pub(crate) fn library_dir(&self) -> PathBuf {
        self.dir(&self.lock())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn default_dir(&self) -> PathBuf {
        self.data_dir.join("library")
    }

    fn dir(&self, state: &State) -> PathBuf {
        state
            .config
            .library_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_dir())
    }

    fn save(&self, state: &State) -> ops::Result<()> {
        write_json(&self.data_dir.join("config.json"), &state.config)?;
        let applied = Applied {
            copies: state.records.clone(),
            toggles: state.toggles.clone(),
        };
        write_json(&self.data_dir.join("applied.json"), &applied)
    }
}

/// The library's items read straight from `data_dir`, for the MCP server,
/// which runs without the app. Reads only: a broken config means the default
/// folder.
pub(crate) fn items_at(data_dir: &Path) -> Vec<LibraryItem> {
    let config: Config = fs::read(data_dir.join("config.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let dir = config
        .library_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("library"));
    list(&dir)
}

// ---------------------------------------------------------------------------
// Pure helpers (testable without the app)

fn source_path(library: &Path, kind: ItemKind, id: &str) -> PathBuf {
    match kind {
        ItemKind::Skill => library.join("skills").join(id),
        ItemKind::Agent => library.join("agents").join(format!("{id}.md")),
    }
}

/// Where `tool` reads the item from in `project`.
fn project_path(project: &Path, tool: ToolId, kind: ItemKind, id: &str) -> PathBuf {
    match kind {
        ItemKind::Skill => project.join(tool.skills_dir()).join(id),
        ItemKind::Agent => project.join(tool.agent_file(id)),
    }
}

/// Whether the project copy is a conversion rather than a plain copy.
fn converted(tool: ToolId, kind: ItemKind) -> bool {
    kind == ItemKind::Agent && tool.agents_are_toml()
}

/// Writes the project copy of `src` to `dest`: a copy, or for Codex subagents
/// a TOML conversion. `replace` swaps an existing copy; otherwise `dest` must
/// not exist.
fn write_copy(
    src: &Path,
    dest: &Path,
    tool: ToolId,
    kind: ItemKind,
    replace: bool,
) -> ops::Result<()> {
    if converted(tool, kind) {
        let toml = codex::agent_to_toml(&ops::read_text(src)?)?;
        if !replace && fs::symlink_metadata(dest).is_ok() {
            return Err(crate::tr!(
                "이미 있습니다: {}",
                "Already exists: {}",
                dest.display()
            ));
        }
        ops::write_atomic(dest, toml)
    } else if replace {
        ops::copy_over(src, dest)
    } else {
        ops::copy_new(src, dest)
    }
}

/// Rejects ids that could escape the target folder, need escaping in gitignore
/// patterns, or are invalid file names on Windows.
pub(crate) fn check_id(id: &str) -> ops::Result<()> {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = id.split('.').next().unwrap_or(id).to_ascii_lowercase();
    let bad = id.is_empty()
        || id.starts_with(['.', '!', '#'])
        || id.ends_with(['.', ' '])
        || id.contains(['/', '\\', ':', '*', '?', '[', ']', '"', '<', '>', '|'])
        || id.chars().any(char::is_control)
        || id.contains("..")
        || RESERVED.contains(&stem.as_str());
    if bad {
        Err(crate::tr!(
            "사용할 수 없는 이름입니다: {id}",
            "This name can't be used: {id}"
        ))
    } else {
        Ok(())
    }
}

fn list(library: &Path) -> Vec<LibraryItem> {
    let mut items = Vec::new();
    for dir in sorted_entries(&library.join("skills")) {
        let skill = dir.join("SKILL.md");
        let Some(id) = file_name(&dir).filter(|n| !n.starts_with('.')) else {
            continue;
        };
        if !skill.is_file() {
            continue;
        }
        let fm = read_frontmatter(&skill);
        items.push(LibraryItem {
            kind: ItemKind::Skill,
            name: fm.get("name").cloned().unwrap_or_else(|| id.clone()),
            description: fm.get("description").cloned(),
            path: skill.display().to_string(),
            id,
            mode: None,
            status: None,
        });
    }
    for file in sorted_entries(&library.join("agents")) {
        if file.extension().is_none_or(|x| x != "md") || !file.is_file() {
            continue;
        }
        let Some(id) = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
        else {
            continue;
        };
        let fm = read_frontmatter(&file);
        items.push(LibraryItem {
            kind: ItemKind::Agent,
            name: fm.get("name").cloned().unwrap_or_else(|| id.clone()),
            description: fm.get("description").cloned(),
            path: file.display().to_string(),
            id,
            mode: None,
            status: None,
        });
    }
    items
}

fn status_of(
    library: &Path,
    project: &Path,
    tool: ToolId,
    item: &LibraryItem,
    record: Option<&Record>,
) -> Option<ApplyStatus> {
    let dest = project_path(project, tool, item.kind, &item.id);
    let Some(record) = record else {
        return dest.exists().then_some(ApplyStatus::Conflict);
    };
    match ops::hash_path(&dest) {
        Ok(None) => return Some(ApplyStatus::Missing),
        Ok(Some(h)) if h == record.hash => {}
        // Edited, or unreadable: either way not safe to treat as ours.
        _ => return Some(ApplyStatus::Modified),
    }
    let applied_from = record.src_hash.as_deref().unwrap_or(&record.hash);
    let src_hash = ops::hash_path(&source_path(library, item.kind, &item.id));
    Some(
        if src_hash.ok().flatten().as_deref() == Some(applied_from) {
            ApplyStatus::Current
        } else {
            ApplyStatus::Outdated
        },
    )
}

fn project_key(project: &Path) -> String {
    fs::canonicalize(project)
        .unwrap_or_else(|_| project.to_path_buf())
        .display()
        .to_string()
}

fn not_applied_by_us() -> String {
    crate::tr!(
        "이 앱이 적용한 항목이 아닙니다",
        "This item wasn't applied by this app"
    )
}

fn not_in_library(id: &str) -> String {
    crate::tr!("라이브러리에 없습니다: {id}", "Not in the library: {id}")
}

fn modified_in_project() -> String {
    crate::tr!(
        "{MODIFIED_ERROR}프로젝트에서 수정된 내용이 있습니다",
        "{MODIFIED_ERROR}It was changed in the project"
    )
}

// ---------------------------------------------------------------------------
// Operations

fn overview(lib: &Library, project: Option<&Path>, tool: ToolId) -> Overview {
    let state = lib.lock();
    let dir = lib.dir(&state);
    let mut items = list(&dir);
    let mut toggled_off = Vec::new();
    if let Some(project) = project {
        let key = project_key(project);
        for item in &mut items {
            let record = state
                .records
                .iter()
                .find(|r| r.is(&key, tool, item.kind, &item.id));
            item.mode = record.map(|r| r.mode);
            item.status = status_of(&dir, project, tool, item, record);
        }
        toggled_off = state
            .toggles
            .iter()
            .filter(|t| t.project == key && t.tool == tool)
            .map(|t| ToggledOff {
                kind: t.kind,
                name: t.name.clone(),
                mode: t.mode,
                path: t.path.clone(),
            })
            .collect();
    }
    Overview {
        dir: dir.display().to_string(),
        is_default: state.config.library_dir.is_none(),
        items,
        toggled_off,
    }
}

fn apply(
    lib: &Library,
    project: &Path,
    tool: ToolId,
    kind: ItemKind,
    id: &str,
    mode: Mode,
) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let src = source_path(&lib.dir(&state), kind, id);
    let Some(src_hash) = ops::hash_path(&src)? else {
        return Err(not_in_library(id));
    };
    let dest = project_path(project, tool, kind, id);
    let key = project_key(project);
    // Linked worktrees share one exclude file, so a personal exclude would also
    // hide a teammate-visible copy applied to a sibling worktree.
    if mode == Mode::Personal
        && let Some(file) = ops::exclude_file_for(&dest)
        && state.records.iter().any(|r| {
            r.project != key
                && r.tool == tool
                && r.kind == kind
                && r.id == id
                && r.mode == Mode::Team
                && ops::exclude_file_for(&project_path(Path::new(&r.project), tool, kind, id))
                    .as_ref()
                    == Some(&file)
        })
    {
        return Err(crate::tr!(
            "같은 저장소의 다른 worktree에 '팀'으로 적용되어 있어 '나만'으로 적용할 수 없습니다",
            "Can't apply as 'Just me' because another worktree of this repository has it applied as 'Team'"
        ));
    }
    if std::fs::symlink_metadata(&dest).is_ok() {
        return Err(crate::tr!(
            "프로젝트에 같은 이름이 이미 있습니다: {}",
            "The project already has something with this name: {}",
            dest.display()
        ));
    }
    write_copy(&src, &dest, tool, kind, false)?;
    // Record what is actually in the project, so status compares like with like.
    let hash = match ops::hash_path(&dest) {
        Ok(Some(hash)) => hash,
        other => {
            let _ = ops::remove_path(&dest);
            return Err(other.err().unwrap_or_else(|| {
                crate::tr!("복사본을 확인할 수 없습니다", "Couldn't verify the copy")
            }));
        }
    };
    if mode == Mode::Personal
        && let Err(e) = ops::exclude_add(&dest, kind == ItemKind::Skill)
    {
        // Roll back: a copy git would see is not what the user asked for.
        let _ = ops::remove_path(&dest);
        return Err(e);
    }
    state.records.retain(|r| !r.is(&key, tool, kind, id));
    state.records.push(Record {
        project: key,
        tool,
        kind,
        id: id.to_string(),
        mode,
        hash,
        src_hash: Some(src_hash),
    });
    lib.save(&state)
}

fn remove(
    lib: &Library,
    project: &Path,
    tool: ToolId,
    kind: ItemKind,
    id: &str,
    force: bool,
) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let record = state
        .records
        .iter()
        .find(|r| r.is(&key, tool, kind, id))
        .cloned()
        .ok_or_else(not_applied_by_us)?;
    let dest = project_path(project, tool, kind, id);
    if !force && !unchanged(&dest, &record.hash) {
        return Err(modified_in_project());
    }
    ops::remove_path(&dest)?;
    // Linked worktrees share one exclude file; keep the line while another
    // checkout still has its own personal copy.
    let shared_line = ops::exclude_file_for(&dest).is_some_and(|file| {
        state.records.iter().any(|r| {
            r.project != key
                && r.tool == tool
                && r.kind == kind
                && r.id == id
                && r.mode == Mode::Personal
                && ops::exclude_file_for(&project_path(Path::new(&r.project), tool, kind, id))
                    .as_ref()
                    == Some(&file)
        })
    });
    if record.mode == Mode::Personal && !shared_line {
        ops::exclude_remove(&dest, kind == ItemKind::Skill)?;
    }
    state.records.retain(|r| !r.is(&key, tool, kind, id));
    lib.save(&state)
}

/// `dest` holds exactly what we recorded, or nothing at all. Unreadable counts
/// as changed, so nothing is deleted on a guess.
fn unchanged(dest: &Path, hash: &str) -> bool {
    match ops::hash_path(dest) {
        Ok(None) => true,
        Ok(Some(h)) => h == hash,
        Err(_) => false,
    }
}

/// Drops the record of a copy that is gone from the project, without touching
/// any project files.
fn forget(
    lib: &Library,
    project: &Path,
    tool: ToolId,
    kind: ItemKind,
    id: &str,
) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let dest = project_path(project, tool, kind, id);
    if std::fs::symlink_metadata(&dest).is_ok() {
        return Err(crate::tr!(
            "프로젝트에 아직 파일이 있습니다. '빼기'를 사용해 주세요",
            "The files are still in the project. Use 'Remove' instead"
        ));
    }
    let before = state.records.len();
    state.records.retain(|r| !r.is(&key, tool, kind, id));
    if state.records.len() == before {
        return Err(not_applied_by_us());
    }
    lib.save(&state)
}

fn update(
    lib: &Library,
    project: &Path,
    tool: ToolId,
    kind: ItemKind,
    id: &str,
    force: bool,
) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let src = source_path(&lib.dir(&state), kind, id);
    let Some(src_hash) = ops::hash_path(&src)? else {
        return Err(not_in_library(id));
    };
    let dest = project_path(project, tool, kind, id);
    let record = state
        .records
        .iter_mut()
        .find(|r| r.is(&key, tool, kind, id))
        .ok_or_else(not_applied_by_us)?;
    if !force && !unchanged(&dest, &record.hash) {
        return Err(modified_in_project());
    }
    write_copy(&src, &dest, tool, kind, true)?;
    record.hash = ops::hash_path(&dest)?
        .ok_or_else(|| crate::tr!("복사본을 확인할 수 없습니다", "Couldn't verify the copy"))?;
    record.src_hash = Some(src_hash);
    lib.save(&state)
}

/// Copies an existing skill folder (given its SKILL.md) or subagent file into
/// the library. A Codex agent (`.toml`) is converted back to Markdown.
fn import(lib: &Library, path: &Path, tool: ToolId, kind: ItemKind) -> ops::Result<()> {
    let state = lib.lock();
    let dir = lib.dir(&state);
    let (src, id) = match kind {
        ItemKind::Skill => {
            let folder = path.parent().ok_or_else(|| {
                crate::tr!(
                    "skill 폴더를 찾을 수 없습니다",
                    "Couldn't find the skill folder"
                )
            })?;
            (folder.to_path_buf(), file_name(folder).unwrap_or_default())
        }
        ItemKind::Agent => (
            path.to_path_buf(),
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
    };
    check_id(&id)?;
    let dest = source_path(&dir, kind, &id);
    if dest.exists() {
        return Err(crate::tr!(
            "라이브러리에 같은 이름이 이미 있습니다: {id}",
            "The library already has something named {id}"
        ));
    }
    if converted(tool, kind) {
        // `config.toml` defines agents inline; it is not one agent's file.
        if path.extension().is_none_or(|x| x != "toml")
            || path.file_name().is_some_and(|n| n == "config.toml")
        {
            return Err(crate::tr!(
                "Codex agent 파일(.toml)이 아닙니다: {}",
                "Not a Codex agent file (.toml): {}",
                path.display()
            ));
        }
        let markdown = codex::agent_from_toml(&ops::read_text(path)?)?;
        return ops::write_atomic(&dest, markdown);
    }
    ops::copy_new(&src, &dest)
}

/// The item a toggle is about.
struct Target<'a> {
    tool: ToolId,
    kind: ItemKind,
    /// Name as the tool sees it.
    name: &'a str,
    /// The item's file (Codex: the SKILL.md path, which Codex matches on).
    path: Option<&'a str>,
}

fn already_off_by_app() -> String {
    crate::tr!(
        "이미 이 앱으로 꺼 둔 항목입니다",
        "This app has already turned it off"
    )
}

/// Turns a skill/subagent off for `mode`, remembering what was there so
/// turning it back on restores exactly that. `codex_config` is the user's
/// `$CODEX_HOME/config.toml`.
fn turn_off(
    lib: &Library,
    project: &Path,
    target: &Target,
    mode: Mode,
    codex_config: &Path,
) -> ops::Result<()> {
    match target.tool {
        ToolId::Claude => turn_off_claude(lib, project, target, mode),
        ToolId::Codex => turn_off_codex(lib, project, target, mode, codex_config),
    }
}

fn turn_off_claude(lib: &Library, project: &Path, target: &Target, mode: Mode) -> ops::Result<()> {
    let (kind, name) = (target.kind, target.name);
    let mut state = lib.lock();
    let key = project_key(project);
    if state.toggles.iter().any(|t| t.is(&key, target)) {
        return Err(already_off_by_app());
    }
    let file = settings_file(project, mode);
    if mode == Mode::Personal && file.exists() && ops::is_tracked(&file) {
        return Err(crate::tr!(
            "settings.local.json이 git에 커밋되어 있어 '나만' 설정을 쓸 수 없습니다. '팀'으로 꺼 주세요.",
            "settings.local.json is committed to git, so 'Just me' can't be used. Turn it off for 'Team' instead."
        ));
    }
    let created = !file.exists();
    let previous = match kind {
        ItemKind::Skill => ops::skill_override(&file, name)?,
        ItemKind::Agent => None,
    };
    let change = match kind {
        ItemKind::Skill => Change::SkillOverride {
            name,
            value: Some("off"),
        },
        ItemKind::Agent => Change::AgentDeny { name, deny: true },
    };
    if previous.as_deref() == Some("off") || !ops::apply_change(&file, change)? {
        // Already off by someone else's rule: nothing of ours to undo later.
        return Err(crate::tr!(
            "이미 이 설정 파일에서 꺼져 있습니다",
            "It's already off in this settings file"
        ));
    }
    // Record before anything else can fail, so the change can always be undone.
    state.toggles.push(Toggle {
        project: key,
        tool: ToolId::Claude,
        kind,
        name: name.to_string(),
        mode,
        previous,
        path: None,
    });
    lib.save(&state)?;
    // Claude Code only git-excludes settings.local.json when it creates the
    // file itself, so do it here for a file we created.
    if mode == Mode::Personal && created {
        ops::exclude_add(&file, false)?;
    }
    Ok(())
}

/// Codex honors `[[skills.config]]` (by absolute SKILL.md path) only in the
/// user's config.toml, and has no switch for a single subagent. Writing the
/// path of the project's own copy turns it off for this project only.
fn turn_off_codex(
    lib: &Library,
    project: &Path,
    target: &Target,
    mode: Mode,
    codex_config: &Path,
) -> ops::Result<()> {
    if target.kind == ItemKind::Agent && !target.tool.can_toggle_agents() {
        return Err(crate::tr!(
            "Codex는 subagent를 하나만 끌 수 없습니다 (agents.enabled로 전부만 끌 수 있습니다)",
            "Codex can't turn off a single subagent (agents.enabled turns off all of them)"
        ));
    }
    if mode == Mode::Team {
        return Err(crate::tr!(
            "Codex는 프로젝트 설정의 skill 끄기를 읽지 않아 '팀'으로 끌 수 없습니다. '나만'으로 꺼 주세요.",
            "Codex ignores turned-off skills in project settings, so it can't be turned off for 'Team'. Turn it off for 'Just me' instead."
        ));
    }
    let path = codex_skill_path(project, target.path)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let target = Target {
        path: Some(&path),
        ..*target
    };
    if state.toggles.iter().any(|t| t.is(&key, &target)) {
        return Err(already_off_by_app());
    }
    if codex::skill_disabled(codex_config, &path)? {
        return Err(crate::tr!(
            "이미 {}에서 꺼져 있습니다",
            "It's already off in {}",
            codex_config.display()
        ));
    }
    // Record first, so an entry in the user's config.toml always has a record
    // the app can undo; drop the record again if the write fails.
    state.toggles.push(Toggle {
        project: key,
        tool: ToolId::Codex,
        kind: ItemKind::Skill,
        name: target.name.to_string(),
        mode: Mode::Personal,
        previous: None,
        path: Some(path.clone()),
    });
    if let Err(e) = lib.save(&state) {
        state.toggles.pop();
        return Err(e);
    }
    if let Err(e) = codex::skill_off(codex_config, &path) {
        state.toggles.pop();
        return match lib.save(&state) {
            Ok(()) => Err(e),
            Err(save) => Err(crate::tr!(
                "{e} (기록도 되돌리지 못했습니다: {save})",
                "{e} (and couldn't undo the record either: {save})"
            )),
        };
    }
    Ok(())
}

/// Checks that `path` is a SKILL.md inside `project`. A skill outside it (the
/// user's own, a parent folder's) has one path for every project, so turning
/// it off here would turn it off everywhere.
fn codex_skill_path(project: &Path, path: Option<&str>) -> ops::Result<String> {
    let outside = || {
        crate::tr!(
            "Codex에서는 이 프로젝트 안의 skill만 끌 수 있습니다. 개인 skill은 모든 프로젝트에서 함께 꺼지기 때문입니다.",
            "In Codex, only skills inside this project can be turned off; a user skill would be turned off in every project."
        )
    };
    let path = path.filter(|p| !p.is_empty()).ok_or_else(outside)?;
    let file = Path::new(path);
    if !file.is_absolute() || file.file_name().is_none_or(|n| n != "SKILL.md") {
        return Err(outside());
    }
    let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let skill = fs::canonicalize(file).map_err(|_| outside())?;
    if !skill.starts_with(canonical(project)) {
        return Err(outside());
    }
    Ok(path.to_string())
}

/// Undoes a `turn_off` of this app. Rules the user wrote are never touched.
fn turn_on(lib: &Library, project: &Path, target: &Target, codex_config: &Path) -> ops::Result<()> {
    let (kind, name) = (target.kind, target.name);
    let mut state = lib.lock();
    let key = project_key(project);
    let index = state
        .toggles
        .iter()
        .position(|t| t.is(&key, target))
        .ok_or_else(|| {
            crate::tr!(
                "이 앱으로 끈 항목이 아니라 되돌릴 수 없습니다",
                "Can't turn it back on because this app didn't turn it off"
            )
        })?;
    let toggle = state.toggles[index].clone();
    match toggle.tool {
        ToolId::Codex => {
            // Only our entry goes; one the user changed since stays.
            if let Some(path) = &toggle.path {
                codex::skill_on(codex_config, path)?;
            }
        }
        ToolId::Claude => {
            let file = settings_file(project, toggle.mode);
            if file.exists() {
                let change = match kind {
                    // Only undo our "off"; if the user changed the value since, keep it.
                    ItemKind::Skill => (ops::skill_override(&file, name)?.as_deref()
                        == Some("off"))
                    .then_some(Change::SkillOverride {
                        name,
                        value: toggle.previous.as_deref(),
                    }),
                    ItemKind::Agent => Some(Change::AgentDeny { name, deny: false }),
                };
                if let Some(change) = change {
                    ops::apply_change(&file, change)?;
                }
            }
        }
    }
    state.toggles.remove(index);
    lib.save(&state)
}

fn settings_file(project: &Path, mode: Mode) -> PathBuf {
    let claude = project.join(".claude");
    match mode {
        Mode::Personal => claude.join("settings.local.json"),
        Mode::Team => claude.join("settings.json"),
    }
}
// ---------------------------------------------------------------------------
// Small IO helpers

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn read_frontmatter(path: &Path) -> std::collections::BTreeMap<String, String> {
    let text = String::from_utf8_lossy(&fs::read(path).unwrap_or_default()).into_owned();
    frontmatter::parse(&text).unwrap_or_default()
}

/// Reads app state. A file that exists but does not parse is moved aside
/// (`*.corrupt`) instead of being silently overwritten with empty state.
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            let _ = fs::rename(path, path.with_extension("json.corrupt"));
            None
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> ops::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    ops::write_atomic(path, &text)
}

// ---------------------------------------------------------------------------
// Commands

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Library) -> ops::Result<T> + Send + 'static,
) -> ops::Result<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<Library>()))
        .await
        .map_err(|e| e.to_string())?
}

/// The user's Codex `config.toml` (`$CODEX_HOME/config.toml`).
fn codex_config(app: &AppHandle) -> ops::Result<PathBuf> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    Ok(crate::tools::codex::Env::detect(home)
        .codex_home
        .join("config.toml"))
}

#[tauri::command]
pub async fn library_overview(
    app: AppHandle,
    project: Option<String>,
    tool: ToolId,
) -> Result<Overview, String> {
    blocking(app, move |lib| {
        Ok(overview(lib, project.as_deref().map(Path::new), tool))
    })
    .await
}

#[tauri::command]
pub async fn library_set_dir(app: AppHandle, dir: Option<String>) -> Result<(), String> {
    blocking(app, move |lib| {
        let mut state = lib.lock();
        state.config.library_dir = dir;
        let dir = lib.dir(&state);
        for sub in ["skills", "agents"] {
            fs::create_dir_all(dir.join(sub)).map_err(|e| e.to_string())?;
        }
        lib.save(&state)
    })
    .await
}

#[tauri::command]
pub async fn library_apply(
    app: AppHandle,
    project: String,
    tool: ToolId,
    kind: ItemKind,
    id: String,
    mode: Mode,
) -> Result<(), String> {
    blocking(app, move |lib| {
        apply(lib, Path::new(&project), tool, kind, &id, mode)
    })
    .await
}

#[tauri::command]
pub async fn library_remove(
    app: AppHandle,
    project: String,
    tool: ToolId,
    kind: ItemKind,
    id: String,
    force: bool,
) -> Result<(), String> {
    blocking(app, move |lib| {
        remove(lib, Path::new(&project), tool, kind, &id, force)
    })
    .await
}

#[tauri::command]
pub async fn library_update(
    app: AppHandle,
    project: String,
    tool: ToolId,
    kind: ItemKind,
    id: String,
    force: bool,
) -> Result<(), String> {
    blocking(app, move |lib| {
        update(lib, Path::new(&project), tool, kind, &id, force)
    })
    .await
}

#[tauri::command]
pub async fn library_forget(
    app: AppHandle,
    project: String,
    tool: ToolId,
    kind: ItemKind,
    id: String,
) -> Result<(), String> {
    blocking(app, move |lib| {
        forget(lib, Path::new(&project), tool, kind, &id)
    })
    .await
}

#[tauri::command]
pub async fn library_import(
    app: AppHandle,
    path: String,
    tool: ToolId,
    kind: ItemKind,
) -> Result<(), String> {
    blocking(app, move |lib| import(lib, Path::new(&path), tool, kind)).await
}

/// `path` is the item's file from the config map (Codex needs the SKILL.md
/// path; Claude Code matches by `name`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn config_set_enabled(
    app: AppHandle,
    project: String,
    tool: ToolId,
    kind: ItemKind,
    name: String,
    enabled: bool,
    mode: Mode,
    path: Option<String>,
) -> Result<(), String> {
    let codex_config = codex_config(&app)?;
    blocking(app, move |lib| {
        let project = Path::new(&project);
        let target = Target {
            tool,
            kind,
            name: &name,
            path: path.as_deref(),
        };
        if enabled {
            turn_on(lib, project, &target, &codex_config)
        } else {
            turn_off(lib, project, &target, mode, &codex_config)
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: ToolId = ToolId::Claude;
    const X: ToolId = ToolId::Codex;
    /// Claude Code toggles never touch the Codex config.
    const NO_CODEX: &str = "/nonexistent/codex/config.toml";

    fn claude(kind: ItemKind, name: &str) -> Target<'_> {
        Target {
            tool: C,
            kind,
            name,
            path: None,
        }
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        lib: Library,
        project: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let project = tmp.path().join("repo");
        fs::create_dir_all(project.join(".git").join("info")).unwrap();
        let lib = Library {
            data_dir,
            state: Mutex::new(State {
                config: Config::default(),
                records: Vec::new(),
                toggles: Vec::new(),
            }),
        };
        let dir = lib.default_dir();
        let skill = dir.join("skills").join("deploy");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: deploy\ndescription: Ship\n---\nv1",
        )
        .unwrap();
        fs::create_dir_all(dir.join("agents")).unwrap();
        fs::write(
            dir.join("agents").join("reviewer.md"),
            "---\nname: reviewer\n---\n",
        )
        .unwrap();
        Fixture {
            _tmp: tmp,
            lib,
            project,
        }
    }

    fn status(f: &Fixture, kind: ItemKind, id: &str) -> Option<ApplyStatus> {
        overview(&f.lib, Some(&f.project), C)
            .items
            .into_iter()
            .find(|i| i.kind == kind && i.id == id)
            .unwrap()
            .status
    }

    fn exclude(f: &Fixture) -> String {
        fs::read_to_string(f.project.join(".git").join("info").join("exclude")).unwrap_or_default()
    }

    #[test]
    fn lists_library_items() {
        let f = fixture();
        let o = overview(&f.lib, None, C);
        assert!(o.is_default);
        let names: Vec<_> = o.items.iter().map(|i| (i.kind, i.name.as_str())).collect();
        assert_eq!(
            names,
            [(ItemKind::Skill, "deploy"), (ItemKind::Agent, "reviewer")]
        );
        assert_eq!(o.items[0].description.as_deref(), Some("Ship"));
    }

    #[test]
    fn apply_update_and_remove_personal_skill() {
        let f = fixture();
        apply(
            &f.lib,
            &f.project,
            C,
            ItemKind::Skill,
            "deploy",
            Mode::Personal,
        )
        .unwrap();
        assert!(f.project.join(".claude/skills/deploy/SKILL.md").is_file());
        assert!(exclude(&f).contains("/.claude/skills/deploy/"));
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Current)
        );
        assert!(
            apply(
                &f.lib,
                &f.project,
                C,
                ItemKind::Skill,
                "deploy",
                Mode::Personal
            )
            .is_err()
        );

        // Library changes -> outdated -> update.
        let src = f.lib.default_dir().join("skills/deploy/SKILL.md");
        fs::write(&src, "---\nname: deploy\n---\nv2").unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Outdated)
        );
        update(&f.lib, &f.project, C, ItemKind::Skill, "deploy", false).unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Current)
        );

        // User edits the copy -> remove needs force.
        fs::write(f.project.join(".claude/skills/deploy/SKILL.md"), "mine").unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Modified)
        );
        let err = remove(&f.lib, &f.project, C, ItemKind::Skill, "deploy", false).unwrap_err();
        assert!(err.starts_with(MODIFIED_ERROR));
        remove(&f.lib, &f.project, C, ItemKind::Skill, "deploy", true).unwrap();
        assert!(!f.project.join(".claude/skills/deploy").exists());
        assert!(!exclude(&f).contains("deploy"));
        assert_eq!(status(&f, ItemKind::Skill, "deploy"), None);
    }

    #[test]
    fn team_agent_is_not_excluded_and_conflicts_are_reported() {
        let f = fixture();
        apply(
            &f.lib,
            &f.project,
            C,
            ItemKind::Agent,
            "reviewer",
            Mode::Team,
        )
        .unwrap();
        assert!(f.project.join(".claude/agents/reviewer.md").is_file());
        assert!(!exclude(&f).contains("reviewer"));

        // Someone else's skill with the same name.
        fs::create_dir_all(f.project.join(".claude/skills/deploy")).unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Conflict)
        );
        assert!(apply(&f.lib, &f.project, C, ItemKind::Skill, "deploy", Mode::Team).is_err());
        assert!(remove(&f.lib, &f.project, C, ItemKind::Skill, "deploy", true).is_err());
    }

    #[test]
    fn toggles_write_the_right_file_and_restore_previous_values() {
        let f = fixture();
        let local = f.project.join(".claude/settings.local.json");
        let shared = f.project.join(".claude/settings.json");
        // The user had chosen "name-only" for deploy and wrote their own deny rule.
        ops::write_atomic(
            &local,
            r#"{"skillOverrides": {"deploy": "name-only"}, "permissions": {"deny": ["Agent(other)"]}}"#,
        )
        .unwrap();

        turn_off(
            &f.lib,
            &f.project,
            &claude(ItemKind::Skill, "deploy"),
            Mode::Personal,
            Path::new(NO_CODEX),
        )
        .unwrap();
        turn_off(
            &f.lib,
            &f.project,
            &claude(ItemKind::Agent, "reviewer"),
            Mode::Team,
            Path::new(NO_CODEX),
        )
        .unwrap();
        assert!(
            turn_off(
                &f.lib,
                &f.project,
                &claude(ItemKind::Agent, "reviewer"),
                Mode::Team,
                Path::new(NO_CODEX)
            )
            .is_err()
        );
        assert!(
            fs::read_to_string(&local)
                .unwrap()
                .contains("\"deploy\": \"off\"")
        );
        assert!(
            fs::read_to_string(&shared)
                .unwrap()
                .contains("Agent(reviewer)")
        );
        let toggled = overview(&f.lib, Some(&f.project), C).toggled_off;
        assert_eq!(toggled.len(), 2);

        turn_on(
            &f.lib,
            &f.project,
            &claude(ItemKind::Skill, "deploy"),
            Path::new(NO_CODEX),
        )
        .unwrap();
        turn_on(
            &f.lib,
            &f.project,
            &claude(ItemKind::Agent, "reviewer"),
            Path::new(NO_CODEX),
        )
        .unwrap();
        let local_text = fs::read_to_string(&local).unwrap();
        assert!(local_text.contains("\"deploy\": \"name-only\""));
        assert!(local_text.contains("Agent(other)"));
        assert!(!fs::read_to_string(&shared).unwrap().contains("reviewer"));
        // Something the app did not turn off cannot be "turned on".
        assert!(
            turn_on(
                &f.lib,
                &f.project,
                &claude(ItemKind::Agent, "other"),
                Path::new(NO_CODEX)
            )
            .is_err()
        );
    }

    #[test]
    fn worktrees_keep_a_shared_exclude_line_until_the_last_copy_goes() {
        let f = fixture();
        let main_git = f.project.join(".git");
        let wt_git = main_git.join("worktrees").join("feature");
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(wt_git.join("commondir"), "../..").unwrap();
        let wt = f._tmp.path().join("feature");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();

        apply(
            &f.lib,
            &f.project,
            C,
            ItemKind::Skill,
            "deploy",
            Mode::Personal,
        )
        .unwrap();
        apply(&f.lib, &wt, C, ItemKind::Skill, "deploy", Mode::Personal).unwrap();
        remove(&f.lib, &f.project, C, ItemKind::Skill, "deploy", false).unwrap();
        assert!(exclude(&f).contains("/.claude/skills/deploy/"));
        remove(&f.lib, &wt, C, ItemKind::Skill, "deploy", false).unwrap();
        assert!(!exclude(&f).contains("/.claude/skills/deploy/"));
    }

    #[test]
    fn imports_and_rejects_unsafe_ids() {
        let f = fixture();
        let skill = f.project.join(".claude/skills/lint/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\nname: lint\n---\n").unwrap();
        import(&f.lib, &skill, C, ItemKind::Skill).unwrap();
        assert!(f.lib.default_dir().join("skills/lint/SKILL.md").is_file());
        assert!(import(&f.lib, &skill, C, ItemKind::Skill).is_err());

        for bad in [
            "../x", "a/b", ".hidden", "", "a[1]", "x*", "nul", "con.txt", "trail.",
        ] {
            assert!(apply(&f.lib, &f.project, C, ItemKind::Skill, bad, Mode::Team).is_err());
        }
    }

    #[test]
    fn turning_off_an_already_off_item_is_refused() {
        let f = fixture();
        ops::write_atomic(
            &f.project.join(".claude/settings.json"),
            r#"{"permissions": {"deny": ["Agent(reviewer)"]}, "skillOverrides": {"deploy": "off"}}"#,
        )
        .unwrap();
        assert!(
            turn_off(
                &f.lib,
                &f.project,
                &claude(ItemKind::Agent, "reviewer"),
                Mode::Team,
                Path::new(NO_CODEX)
            )
            .is_err()
        );
        assert!(
            turn_off(
                &f.lib,
                &f.project,
                &claude(ItemKind::Skill, "deploy"),
                Mode::Team,
                Path::new(NO_CODEX)
            )
            .is_err()
        );
        assert!(overview(&f.lib, Some(&f.project), C).toggled_off.is_empty());
    }

    #[test]
    fn forget_only_drops_records_of_missing_copies() {
        let f = fixture();
        apply(&f.lib, &f.project, C, ItemKind::Skill, "deploy", Mode::Team).unwrap();
        assert!(forget(&f.lib, &f.project, C, ItemKind::Skill, "deploy").is_err());
        fs::remove_dir_all(f.project.join(".claude/skills/deploy")).unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Missing)
        );
        forget(&f.lib, &f.project, C, ItemKind::Skill, "deploy").unwrap();
        assert_eq!(status(&f, ItemKind::Skill, "deploy"), None);
    }

    #[test]
    fn personal_apply_refused_when_a_sibling_worktree_has_a_team_copy() {
        let f = fixture();
        let wt_git = f.project.join(".git").join("worktrees").join("feature");
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(wt_git.join("commondir"), "../..").unwrap();
        let wt = f._tmp.path().join("feature");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        apply(&f.lib, &wt, C, ItemKind::Skill, "deploy", Mode::Team).unwrap();
        assert!(
            apply(
                &f.lib,
                &f.project,
                C,
                ItemKind::Skill,
                "deploy",
                Mode::Personal
            )
            .is_err()
        );
        assert!(!f.project.join(".claude/skills/deploy").exists());
    }

    fn status_for(f: &Fixture, tool: ToolId, kind: ItemKind, id: &str) -> Option<ApplyStatus> {
        overview(&f.lib, Some(&f.project), tool)
            .items
            .into_iter()
            .find(|i| i.kind == kind && i.id == id)
            .unwrap()
            .status
    }

    fn write_agent(f: &Fixture, id: &str, text: &str) {
        fs::write(
            f.lib.default_dir().join("agents").join(format!("{id}.md")),
            text,
        )
        .unwrap();
    }

    #[test]
    fn codex_apply_goes_to_codex_folders_and_converts_agents() {
        let f = fixture();
        apply(
            &f.lib,
            &f.project,
            X,
            ItemKind::Skill,
            "deploy",
            Mode::Personal,
        )
        .unwrap();
        assert!(f.project.join(".agents/skills/deploy/SKILL.md").is_file());
        assert!(!f.project.join(".claude").exists());
        assert!(exclude(&f).contains("/.agents/skills/deploy/"));
        assert_eq!(
            status_for(&f, X, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Current)
        );
        // The Claude view of the same project knows nothing about it.
        assert_eq!(status(&f, ItemKind::Skill, "deploy"), None);
        assert!(remove(&f.lib, &f.project, C, ItemKind::Skill, "deploy", true).is_err());

        write_agent(
            &f,
            "helper",
            "---\nname: helper\ndescription: Helps \"a lot\"\nmodel: opus\ntools: Read\n---\n\nDo it.\nCarefully.\n",
        );
        apply(&f.lib, &f.project, X, ItemKind::Agent, "helper", Mode::Team).unwrap();
        let dest = f.project.join(".codex/agents/helper.toml");
        let doc: toml_edit::DocumentMut = fs::read_to_string(&dest).unwrap().parse().unwrap();
        assert_eq!(doc["name"].as_str(), Some("helper"));
        assert_eq!(doc["description"].as_str(), Some("Helps \"a lot\""));
        assert_eq!(
            doc["developer_instructions"].as_str(),
            Some("Do it.\nCarefully.")
        );
        assert!(doc.get("model").is_none());
        assert!(!exclude(&f).contains("helper"));
        // The copy differs from the source, yet it is current.
        assert_eq!(
            status_for(&f, X, ItemKind::Agent, "helper"),
            Some(ApplyStatus::Current)
        );

        // Library edit -> outdated (via src_hash) -> update converts again.
        write_agent(
            &f,
            "helper",
            "---\nname: helper\ndescription: Helps\n---\nDo it twice.\n",
        );
        assert_eq!(
            status_for(&f, X, ItemKind::Agent, "helper"),
            Some(ApplyStatus::Outdated)
        );
        update(&f.lib, &f.project, X, ItemKind::Agent, "helper", false).unwrap();
        assert!(fs::read_to_string(&dest).unwrap().contains("Do it twice."));
        assert_eq!(
            status_for(&f, X, ItemKind::Agent, "helper"),
            Some(ApplyStatus::Current)
        );

        // Edited in the project -> modified, and update needs force.
        fs::write(&dest, "name = \"helper\"\n").unwrap();
        assert_eq!(
            status_for(&f, X, ItemKind::Agent, "helper"),
            Some(ApplyStatus::Modified)
        );
        let err = update(&f.lib, &f.project, X, ItemKind::Agent, "helper", false).unwrap_err();
        assert!(err.starts_with(MODIFIED_ERROR));
        remove(&f.lib, &f.project, X, ItemKind::Agent, "helper", true).unwrap();
        assert!(!dest.exists());
    }

    #[test]
    fn codex_agent_without_description_is_not_applied() {
        let f = fixture();
        // The fixture's reviewer has no description.
        let err = apply(
            &f.lib,
            &f.project,
            X,
            ItemKind::Agent,
            "reviewer",
            Mode::Team,
        )
        .unwrap_err();
        assert!(err.contains("description"), "{err}");
        assert!(!f.project.join(".codex/agents/reviewer.toml").exists());
        assert_eq!(status_for(&f, X, ItemKind::Agent, "reviewer"), None);
    }

    #[test]
    fn records_without_src_hash_compare_against_hash() {
        let f = fixture();
        apply(&f.lib, &f.project, C, ItemKind::Skill, "deploy", Mode::Team).unwrap();
        f.lib.lock().records[0].src_hash = None;
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Current)
        );
    }

    #[test]
    fn legacy_applied_json_means_claude() {
        let legacy =
            r#"[{"project": "/p", "kind": "skill", "id": "a", "mode": "team", "hash": "h"}]"#;
        let applied = serde_json::from_str::<AppliedFile>(legacy)
            .unwrap()
            .into_applied();
        assert_eq!(applied.copies[0].tool, ToolId::Claude);
        assert_eq!(applied.copies[0].src_hash, None);

        let current = r#"{"copies": [{"project": "/p", "kind": "agent", "id": "b", "mode": "personal", "hash": "h"}],
            "toggles": [{"project": "/p", "kind": "skill", "name": "x", "mode": "team", "previous": null}]}"#;
        let applied = serde_json::from_str::<AppliedFile>(current)
            .unwrap()
            .into_applied();
        assert_eq!(applied.copies[0].tool, ToolId::Claude);
        assert_eq!(applied.toggles[0].tool, ToolId::Claude);
        assert_eq!(applied.toggles[0].path, None);
        // Written back with the tool.
        let json = serde_json::to_string(&applied).unwrap();
        assert!(json.contains("\"tool\":\"claude\""));
    }

    /// A Codex skill under `root`; returns its SKILL.md path.
    fn codex_skill(root: &Path, name: &str) -> String {
        let dir = root.join(".agents/skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("SKILL.md");
        fs::write(&file, format!("---\nname: {name}\ndescription: d\n---\n")).unwrap();
        file.display().to_string()
    }

    fn codex<'a>(kind: ItemKind, name: &'a str, path: Option<&'a str>) -> Target<'a> {
        Target {
            tool: X,
            kind,
            name,
            path,
        }
    }

    #[test]
    fn codex_turns_off_project_skills_in_the_user_config() {
        let f = fixture();
        let config = f._tmp.path().join("codex-home").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let original = "# mine\nmodel = \"gpt-5\"\n";
        fs::write(&config, original).unwrap();
        let skill = codex_skill(&f.project, "lint");
        let target = codex(ItemKind::Skill, "lint", Some(&skill));

        turn_off(&f.lib, &f.project, &target, Mode::Personal, &config).unwrap();
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.starts_with(original));
        assert!(codex::skill_disabled(&config, &skill).unwrap());
        assert!(turn_off(&f.lib, &f.project, &target, Mode::Personal, &config).is_err());
        let toggled = overview(&f.lib, Some(&f.project), X).toggled_off;
        assert_eq!(toggled.len(), 1);
        assert_eq!(toggled[0].path.as_deref(), Some(skill.as_str()));
        assert!(overview(&f.lib, Some(&f.project), C).toggled_off.is_empty());
        // Nothing was written into the project.
        assert!(!f.project.join(".claude").exists());
        assert!(!f.project.join(".codex").exists());

        turn_on(&f.lib, &f.project, &target, &config).unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert!(overview(&f.lib, Some(&f.project), X).toggled_off.is_empty());
    }

    #[test]
    fn codex_turn_off_records_before_writing_and_rolls_back() {
        let f = fixture();
        let config = f._tmp.path().join("codex-home").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let original = "model = \"gpt-5\"\n";
        fs::write(&config, original).unwrap();
        let skill = codex_skill(&f.project, "lint");
        let target = codex(ItemKind::Skill, "lint", Some(&skill));

        // The record can't be saved: the user's config.toml stays untouched.
        let applied = f.lib.data_dir.join("applied.json");
        fs::create_dir_all(applied.join("blocker")).unwrap();
        assert!(turn_off(&f.lib, &f.project, &target, Mode::Personal, &config).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert!(f.lib.lock().toggles.is_empty());
        fs::remove_dir_all(&applied).unwrap();

        // config.toml can't be written: the record is dropped again.
        let blocker = config.with_file_name(".config.toml.ai-management-tmp");
        fs::create_dir_all(blocker.join("x")).unwrap();
        assert!(turn_off(&f.lib, &f.project, &target, Mode::Personal, &config).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert!(f.lib.lock().toggles.is_empty());
        let saved = fs::read_to_string(&applied).unwrap();
        assert!(!saved.contains("lint"), "{saved}");
        fs::remove_dir_all(&blocker).unwrap();

        turn_off(&f.lib, &f.project, &target, Mode::Personal, &config).unwrap();
        assert!(fs::read_to_string(&applied).unwrap().contains("lint"));
    }

    #[test]
    fn codex_refuses_team_agents_and_skills_outside_the_project() {
        let f = fixture();
        let config = f._tmp.path().join("codex-home").join("config.toml");
        let skill = codex_skill(&f.project, "lint");
        let user_skill = codex_skill(&f._tmp.path().join("home"), "mine");
        let escaping = format!(
            "{}/.agents/skills/lint/../../../../home/.agents/skills/mine/SKILL.md",
            f.project.display()
        );
        let refused = [
            (codex(ItemKind::Skill, "lint", Some(&skill)), Mode::Team),
            (
                codex(ItemKind::Agent, "reviewer", Some(&skill)),
                Mode::Personal,
            ),
            (
                codex(ItemKind::Skill, "mine", Some(&user_skill)),
                Mode::Personal,
            ),
            (codex(ItemKind::Skill, "lint", None), Mode::Personal),
            (
                codex(
                    ItemKind::Skill,
                    "lint",
                    Some(".agents/skills/lint/SKILL.md"),
                ),
                Mode::Personal,
            ),
            (
                codex(ItemKind::Skill, "mine", Some(&escaping)),
                Mode::Personal,
            ),
        ];
        for (target, mode) in &refused {
            assert!(
                turn_off(&f.lib, &f.project, target, *mode, &config).is_err(),
                "{:?} {:?}",
                target.path,
                mode
            );
        }
        assert!(!config.exists());
        assert!(overview(&f.lib, Some(&f.project), X).toggled_off.is_empty());
        // Something the app did not turn off cannot be turned on.
        let target = codex(ItemKind::Skill, "lint", Some(&skill));
        assert!(turn_on(&f.lib, &f.project, &target, &config).is_err());
    }

    #[test]
    fn codex_agents_import_as_markdown() {
        let f = fixture();
        let file = f.project.join(".codex/agents/rev.toml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            "name = \"rev\"\ndescription = \"Reviews\"\nmodel = \"gpt-5\"\ndeveloper_instructions = \"\"\"\nLook closely.\n\"\"\"\n",
        )
        .unwrap();
        import(&f.lib, &file, X, ItemKind::Agent).unwrap();
        let md = fs::read_to_string(f.lib.default_dir().join("agents/rev.md")).unwrap();
        assert_eq!(
            md,
            "---\nname: rev\ndescription: Reviews\n---\n\nLook closely.\n"
        );
        assert!(import(&f.lib, &file, X, ItemKind::Agent).is_err());

        // Skills import the same way for both tools.
        let skill = codex_skill(&f.project, "lint");
        import(&f.lib, Path::new(&skill), X, ItemKind::Skill).unwrap();
        assert!(f.lib.default_dir().join("skills/lint/SKILL.md").is_file());

        // A Markdown file is not a Codex agent.
        let md_agent = f.project.join(".codex/agents/other.md");
        fs::write(&md_agent, "---\nname: other\n---\n").unwrap();
        assert!(import(&f.lib, &md_agent, X, ItemKind::Agent).is_err());
    }
}
