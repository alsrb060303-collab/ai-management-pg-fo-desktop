//! Personal library of skills and subagents that can be applied to (copied
//! into) projects, plus per-project on/off toggles in Claude Code settings.
//!
//! Library layout: `<dir>/skills/<id>/SKILL.md` and `<dir>/agents/<id>.md`.
//! Copies the app made are recorded with their hash in `applied.json` in the
//! app data folder, so a copy the user edited is never removed or overwritten
//! without asking.

pub(crate) mod ops;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::tools::frontmatter;
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
    kind: ItemKind,
    id: String,
    mode: Mode,
    hash: String,
}

/// A skill or subagent this app turned off, so turning it back on only undoes
/// what the app wrote.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Toggle {
    project: String,
    kind: ItemKind,
    /// Name as Claude Code sees it (frontmatter `name`).
    name: String,
    mode: Mode,
    /// `skillOverrides` value before we set "off", restored on enable.
    previous: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggledOff {
    pub kind: ItemKind,
    pub name: String,
    pub mode: Mode,
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
    /// Name from frontmatter, as Claude Code sees it.
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
        let applied = match read_json(&data_dir.join("applied.json")) {
            Some(AppliedFile::Current(a)) => a,
            Some(AppliedFile::Legacy(copies)) => Applied {
                copies,
                toggles: Vec::new(),
            },
            None => Applied::default(),
        };
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

// ---------------------------------------------------------------------------
// Pure helpers (testable without the app)

fn source_path(library: &Path, kind: ItemKind, id: &str) -> PathBuf {
    match kind {
        ItemKind::Skill => library.join("skills").join(id),
        ItemKind::Agent => library.join("agents").join(format!("{id}.md")),
    }
}

fn project_path(project: &Path, kind: ItemKind, id: &str) -> PathBuf {
    let claude = project.join(".claude");
    match kind {
        ItemKind::Skill => claude.join("skills").join(id),
        ItemKind::Agent => claude.join("agents").join(format!("{id}.md")),
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
    item: &LibraryItem,
    record: Option<&Record>,
) -> Option<ApplyStatus> {
    let dest = project_path(project, item.kind, &item.id);
    let Some(record) = record else {
        return dest.exists().then_some(ApplyStatus::Conflict);
    };
    match ops::hash_path(&dest) {
        Ok(None) => return Some(ApplyStatus::Missing),
        Ok(Some(h)) if h == record.hash => {}
        // Edited, or unreadable: either way not safe to treat as ours.
        _ => return Some(ApplyStatus::Modified),
    }
    let src_hash = ops::hash_path(&source_path(library, item.kind, &item.id));
    Some(
        if src_hash.ok().flatten().as_deref() == Some(record.hash.as_str()) {
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

fn find_record<'a>(
    records: &'a [Record],
    project: &str,
    kind: ItemKind,
    id: &str,
) -> Option<&'a Record> {
    records
        .iter()
        .find(|r| r.project == project && r.kind == kind && r.id == id)
}

// ---------------------------------------------------------------------------
// Operations

fn overview(lib: &Library, project: Option<&Path>) -> Overview {
    let state = lib.lock();
    let dir = lib.dir(&state);
    let mut items = list(&dir);
    if let Some(project) = project {
        let key = project_key(project);
        for item in &mut items {
            let record = find_record(&state.records, &key, item.kind, &item.id);
            item.mode = record.map(|r| r.mode);
            item.status = status_of(&dir, project, item, record);
        }
    }
    let toggled_off = project
        .map(|p| {
            let key = project_key(p);
            state
                .toggles
                .iter()
                .filter(|t| t.project == key)
                .map(|t| ToggledOff {
                    kind: t.kind,
                    name: t.name.clone(),
                    mode: t.mode,
                })
                .collect()
        })
        .unwrap_or_default();
    Overview {
        dir: dir.display().to_string(),
        is_default: state.config.library_dir.is_none(),
        items,
        toggled_off,
    }
}

fn apply(lib: &Library, project: &Path, kind: ItemKind, id: &str, mode: Mode) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let src = source_path(&lib.dir(&state), kind, id);
    if ops::hash_path(&src)?.is_none() {
        return Err(crate::tr!(
            "라이브러리에 없습니다: {id}",
            "Not in the library: {id}"
        ));
    }
    let dest = project_path(project, kind, id);
    let key = project_key(project);
    // Linked worktrees share one exclude file, so a personal exclude would also
    // hide a teammate-visible copy applied to a sibling worktree.
    if mode == Mode::Personal
        && let Some(file) = ops::exclude_file_for(&dest)
        && state.records.iter().any(|r| {
            r.project != key
                && r.kind == kind
                && r.id == id
                && r.mode == Mode::Team
                && ops::exclude_file_for(&project_path(Path::new(&r.project), kind, id)).as_ref()
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
    ops::copy_new(&src, &dest)?;
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
    state
        .records
        .retain(|r| !(r.project == key && r.kind == kind && r.id == id));
    state.records.push(Record {
        project: key,
        kind,
        id: id.to_string(),
        mode,
        hash,
    });
    lib.save(&state)
}

fn remove(lib: &Library, project: &Path, kind: ItemKind, id: &str, force: bool) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let record = find_record(&state.records, &key, kind, id)
        .cloned()
        .ok_or_else(|| {
            crate::tr!(
                "이 앱이 적용한 항목이 아닙니다",
                "This item wasn't applied by this app"
            )
        })?;
    let dest = project_path(project, kind, id);
    if !force && !unchanged(&dest, &record.hash) {
        return Err(crate::tr!(
            "{MODIFIED_ERROR}프로젝트에서 수정된 내용이 있습니다",
            "{MODIFIED_ERROR}It was changed in the project"
        ));
    }
    ops::remove_path(&dest)?;
    // Linked worktrees share one exclude file; keep the line while another
    // checkout still has its own personal copy.
    let shared_line = ops::exclude_file_for(&dest).is_some_and(|file| {
        state.records.iter().any(|r| {
            r.project != key
                && r.kind == kind
                && r.id == id
                && r.mode == Mode::Personal
                && ops::exclude_file_for(&project_path(Path::new(&r.project), kind, id)).as_ref()
                    == Some(&file)
        })
    });
    if record.mode == Mode::Personal && !shared_line {
        ops::exclude_remove(&dest, kind == ItemKind::Skill)?;
    }
    state
        .records
        .retain(|r| !(r.project == key && r.kind == kind && r.id == id));
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
fn forget(lib: &Library, project: &Path, kind: ItemKind, id: &str) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let dest = project_path(project, kind, id);
    if std::fs::symlink_metadata(&dest).is_ok() {
        return Err(crate::tr!(
            "프로젝트에 아직 파일이 있습니다. '빼기'를 사용해 주세요",
            "The files are still in the project. Use 'Remove' instead"
        ));
    }
    let before = state.records.len();
    state
        .records
        .retain(|r| !(r.project == key && r.kind == kind && r.id == id));
    if state.records.len() == before {
        return Err(crate::tr!(
            "이 앱이 적용한 항목이 아닙니다",
            "This item wasn't applied by this app"
        ));
    }
    lib.save(&state)
}

fn update(lib: &Library, project: &Path, kind: ItemKind, id: &str, force: bool) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let src = source_path(&lib.dir(&state), kind, id);
    if ops::hash_path(&src)?.is_none() {
        return Err(crate::tr!(
            "라이브러리에 없습니다: {id}",
            "Not in the library: {id}"
        ));
    }
    let dest = project_path(project, kind, id);
    let record = state
        .records
        .iter_mut()
        .find(|r| r.project == key && r.kind == kind && r.id == id)
        .ok_or_else(|| {
            crate::tr!(
                "이 앱이 적용한 항목이 아닙니다",
                "This item wasn't applied by this app"
            )
        })?;
    if !force && !unchanged(&dest, &record.hash) {
        return Err(crate::tr!(
            "{MODIFIED_ERROR}프로젝트에서 수정된 내용이 있습니다",
            "{MODIFIED_ERROR}It was changed in the project"
        ));
    }
    ops::copy_over(&src, &dest)?;
    record.hash = ops::hash_path(&dest)?
        .ok_or_else(|| crate::tr!("복사본을 확인할 수 없습니다", "Couldn't verify the copy"))?;
    lib.save(&state)
}

/// Copies an existing skill folder (given its SKILL.md) or subagent file into
/// the library.
fn import(lib: &Library, path: &Path, kind: ItemKind) -> ops::Result<()> {
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
    ops::copy_new(&src, &dest)
}

/// Turns a skill/subagent off in the settings file for `mode`, remembering
/// what was there so turning it back on restores exactly that.
fn turn_off(
    lib: &Library,
    project: &Path,
    kind: ItemKind,
    name: &str,
    mode: Mode,
) -> ops::Result<()> {
    let mut state = lib.lock();
    let key = project_key(project);
    if state
        .toggles
        .iter()
        .any(|t| t.project == key && t.kind == kind && t.name == name)
    {
        return Err(crate::tr!(
            "이미 이 앱으로 꺼 둔 항목입니다",
            "This app has already turned it off"
        ));
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
        kind,
        name: name.to_string(),
        mode,
        previous,
    });
    lib.save(&state)?;
    // Claude Code only git-excludes settings.local.json when it creates the
    // file itself, so do it here for a file we created.
    if mode == Mode::Personal && created {
        ops::exclude_add(&file, false)?;
    }
    Ok(())
}

/// Undoes a `turn_off` of this app. Rules the user wrote are never touched.
fn turn_on(lib: &Library, project: &Path, kind: ItemKind, name: &str) -> ops::Result<()> {
    let mut state = lib.lock();
    let key = project_key(project);
    let index = state
        .toggles
        .iter()
        .position(|t| t.project == key && t.kind == kind && t.name == name)
        .ok_or_else(|| {
            crate::tr!(
                "이 앱으로 끈 항목이 아니라 되돌릴 수 없습니다",
                "Can't turn it back on because this app didn't turn it off"
            )
        })?;
    let toggle = state.toggles[index].clone();
    let file = settings_file(project, toggle.mode);
    if file.exists() {
        let change = match kind {
            // Only undo our "off"; if the user changed the value since, keep it.
            ItemKind::Skill => (ops::skill_override(&file, name)?.as_deref() == Some("off"))
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

#[tauri::command]
pub async fn library_overview(app: AppHandle, project: Option<String>) -> Result<Overview, String> {
    blocking(app, move |lib| {
        Ok(overview(lib, project.as_deref().map(Path::new)))
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
    kind: ItemKind,
    id: String,
    mode: Mode,
) -> Result<(), String> {
    blocking(app, move |lib| {
        apply(lib, Path::new(&project), kind, &id, mode)
    })
    .await
}

#[tauri::command]
pub async fn library_remove(
    app: AppHandle,
    project: String,
    kind: ItemKind,
    id: String,
    force: bool,
) -> Result<(), String> {
    blocking(app, move |lib| {
        remove(lib, Path::new(&project), kind, &id, force)
    })
    .await
}

#[tauri::command]
pub async fn library_update(
    app: AppHandle,
    project: String,
    kind: ItemKind,
    id: String,
    force: bool,
) -> Result<(), String> {
    blocking(app, move |lib| {
        update(lib, Path::new(&project), kind, &id, force)
    })
    .await
}

#[tauri::command]
pub async fn library_forget(
    app: AppHandle,
    project: String,
    kind: ItemKind,
    id: String,
) -> Result<(), String> {
    blocking(app, move |lib| forget(lib, Path::new(&project), kind, &id)).await
}

#[tauri::command]
pub async fn library_import(app: AppHandle, path: String, kind: ItemKind) -> Result<(), String> {
    blocking(app, move |lib| import(lib, Path::new(&path), kind)).await
}

#[tauri::command]
pub async fn config_set_enabled(
    app: AppHandle,
    project: String,
    kind: ItemKind,
    name: String,
    enabled: bool,
    mode: Mode,
) -> Result<(), String> {
    blocking(app, move |lib| {
        let project = Path::new(&project);
        if enabled {
            turn_on(lib, project, kind, &name)
        } else {
            turn_off(lib, project, kind, &name, mode)
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
        overview(&f.lib, Some(&f.project))
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
        let o = overview(&f.lib, None);
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
        update(&f.lib, &f.project, ItemKind::Skill, "deploy", false).unwrap();
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
        let err = remove(&f.lib, &f.project, ItemKind::Skill, "deploy", false).unwrap_err();
        assert!(err.starts_with(MODIFIED_ERROR));
        remove(&f.lib, &f.project, ItemKind::Skill, "deploy", true).unwrap();
        assert!(!f.project.join(".claude/skills/deploy").exists());
        assert!(!exclude(&f).contains("deploy"));
        assert_eq!(status(&f, ItemKind::Skill, "deploy"), None);
    }

    #[test]
    fn team_agent_is_not_excluded_and_conflicts_are_reported() {
        let f = fixture();
        apply(&f.lib, &f.project, ItemKind::Agent, "reviewer", Mode::Team).unwrap();
        assert!(f.project.join(".claude/agents/reviewer.md").is_file());
        assert!(!exclude(&f).contains("reviewer"));

        // Someone else's skill with the same name.
        fs::create_dir_all(f.project.join(".claude/skills/deploy")).unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Conflict)
        );
        assert!(apply(&f.lib, &f.project, ItemKind::Skill, "deploy", Mode::Team).is_err());
        assert!(remove(&f.lib, &f.project, ItemKind::Skill, "deploy", true).is_err());
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
            ItemKind::Skill,
            "deploy",
            Mode::Personal,
        )
        .unwrap();
        turn_off(&f.lib, &f.project, ItemKind::Agent, "reviewer", Mode::Team).unwrap();
        assert!(turn_off(&f.lib, &f.project, ItemKind::Agent, "reviewer", Mode::Team).is_err());
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
        let toggled = overview(&f.lib, Some(&f.project)).toggled_off;
        assert_eq!(toggled.len(), 2);

        turn_on(&f.lib, &f.project, ItemKind::Skill, "deploy").unwrap();
        turn_on(&f.lib, &f.project, ItemKind::Agent, "reviewer").unwrap();
        let local_text = fs::read_to_string(&local).unwrap();
        assert!(local_text.contains("\"deploy\": \"name-only\""));
        assert!(local_text.contains("Agent(other)"));
        assert!(!fs::read_to_string(&shared).unwrap().contains("reviewer"));
        // Something the app did not turn off cannot be "turned on".
        assert!(turn_on(&f.lib, &f.project, ItemKind::Agent, "other").is_err());
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
            ItemKind::Skill,
            "deploy",
            Mode::Personal,
        )
        .unwrap();
        apply(&f.lib, &wt, ItemKind::Skill, "deploy", Mode::Personal).unwrap();
        remove(&f.lib, &f.project, ItemKind::Skill, "deploy", false).unwrap();
        assert!(exclude(&f).contains("/.claude/skills/deploy/"));
        remove(&f.lib, &wt, ItemKind::Skill, "deploy", false).unwrap();
        assert!(!exclude(&f).contains("/.claude/skills/deploy/"));
    }

    #[test]
    fn imports_and_rejects_unsafe_ids() {
        let f = fixture();
        let skill = f.project.join(".claude/skills/lint/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\nname: lint\n---\n").unwrap();
        import(&f.lib, &skill, ItemKind::Skill).unwrap();
        assert!(f.lib.default_dir().join("skills/lint/SKILL.md").is_file());
        assert!(import(&f.lib, &skill, ItemKind::Skill).is_err());

        for bad in [
            "../x", "a/b", ".hidden", "", "a[1]", "x*", "nul", "con.txt", "trail.",
        ] {
            assert!(apply(&f.lib, &f.project, ItemKind::Skill, bad, Mode::Team).is_err());
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
        assert!(turn_off(&f.lib, &f.project, ItemKind::Agent, "reviewer", Mode::Team).is_err());
        assert!(turn_off(&f.lib, &f.project, ItemKind::Skill, "deploy", Mode::Team).is_err());
        assert!(overview(&f.lib, Some(&f.project)).toggled_off.is_empty());
    }

    #[test]
    fn forget_only_drops_records_of_missing_copies() {
        let f = fixture();
        apply(&f.lib, &f.project, ItemKind::Skill, "deploy", Mode::Team).unwrap();
        assert!(forget(&f.lib, &f.project, ItemKind::Skill, "deploy").is_err());
        fs::remove_dir_all(f.project.join(".claude/skills/deploy")).unwrap();
        assert_eq!(
            status(&f, ItemKind::Skill, "deploy"),
            Some(ApplyStatus::Missing)
        );
        forget(&f.lib, &f.project, ItemKind::Skill, "deploy").unwrap();
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
        apply(&f.lib, &wt, ItemKind::Skill, "deploy", Mode::Team).unwrap();
        assert!(
            apply(
                &f.lib,
                &f.project,
                ItemKind::Skill,
                "deploy",
                Mode::Personal
            )
            .is_err()
        );
        assert!(!f.project.join(".claude/skills/deploy").exists());
    }
}
