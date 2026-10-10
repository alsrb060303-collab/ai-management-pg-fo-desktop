//! Personal library of skills and subagents that can be applied to (copied
//! into) projects, plus per-project on/off toggles in Claude Code settings.
//!
//! Library layout: `<dir>/skills/<id>/SKILL.md` and `<dir>/agents/<id>.md`.
//! Copies the app made are recorded with their hash in `applied.json` in the
//! app data folder, so a copy the user edited is never removed or overwritten
//! without asking.

mod ops;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::claude_config::frontmatter;
use ops::Target;

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
}

pub struct Library {
    data_dir: PathBuf,
    state: Mutex<State>,
}

struct State {
    config: Config,
    records: Vec<Record>,
}

impl Library {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let data_dir = app.path().app_data_dir()?;
        let config = read_json(&data_dir.join("config.json")).unwrap_or_default();
        let records = read_json(&data_dir.join("applied.json")).unwrap_or_default();
        app.manage(Library {
            data_dir,
            state: Mutex::new(State { config, records }),
        });
        Ok(())
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
        write_json(&self.data_dir.join("applied.json"), &state.records)
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

/// Rejects ids that could escape the target folder.
fn check_id(id: &str) -> ops::Result<()> {
    let bad =
        id.is_empty() || id.starts_with('.') || id.contains(['/', '\\', ':']) || id.contains("..");
    if bad {
        Err(format!("사용할 수 없는 이름입니다: {id}"))
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
    let Some(dest_hash) = ops::hash_path(&dest) else {
        return Some(ApplyStatus::Missing);
    };
    if dest_hash != record.hash {
        return Some(ApplyStatus::Modified);
    }
    let src_hash = ops::hash_path(&source_path(library, item.kind, &item.id));
    Some(if src_hash.as_deref() == Some(record.hash.as_str()) {
        ApplyStatus::Current
    } else {
        ApplyStatus::Outdated
    })
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
    Overview {
        dir: dir.display().to_string(),
        is_default: state.config.library_dir.is_none(),
        items,
    }
}

fn apply(lib: &Library, project: &Path, kind: ItemKind, id: &str, mode: Mode) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let src = source_path(&lib.dir(&state), kind, id);
    let hash = ops::hash_path(&src).ok_or_else(|| format!("라이브러리에 없습니다: {id}"))?;
    let dest = project_path(project, kind, id);
    if dest.exists() {
        return Err(format!(
            "프로젝트에 같은 이름이 이미 있습니다: {}",
            dest.display()
        ));
    }
    ops::copy_new(&src, &dest)?;
    if mode == Mode::Personal {
        ops::exclude_add(&dest, kind == ItemKind::Skill)?;
    }
    let key = project_key(project);
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
        .ok_or_else(|| "이 앱이 적용한 항목이 아닙니다".to_string())?;
    let dest = project_path(project, kind, id);
    if !force && ops::hash_path(&dest).is_some_and(|h| h != record.hash) {
        return Err(format!(
            "{MODIFIED_ERROR}프로젝트에서 수정된 내용이 있습니다"
        ));
    }
    ops::remove_path(&dest)?;
    if record.mode == Mode::Personal {
        ops::exclude_remove(&dest, kind == ItemKind::Skill)?;
    }
    state
        .records
        .retain(|r| !(r.project == key && r.kind == kind && r.id == id));
    lib.save(&state)
}

fn update(lib: &Library, project: &Path, kind: ItemKind, id: &str, force: bool) -> ops::Result<()> {
    check_id(id)?;
    let mut state = lib.lock();
    let key = project_key(project);
    let src = source_path(&lib.dir(&state), kind, id);
    let hash = ops::hash_path(&src).ok_or_else(|| format!("라이브러리에 없습니다: {id}"))?;
    let dest = project_path(project, kind, id);
    let record = state
        .records
        .iter_mut()
        .find(|r| r.project == key && r.kind == kind && r.id == id)
        .ok_or_else(|| "이 앱이 적용한 항목이 아닙니다".to_string())?;
    if !force && ops::hash_path(&dest).is_some_and(|h| h != record.hash) {
        return Err(format!(
            "{MODIFIED_ERROR}프로젝트에서 수정된 내용이 있습니다"
        ));
    }
    ops::copy_over(&src, &dest)?;
    record.hash = hash;
    lib.save(&state)
}

/// Copies an existing skill folder (given its SKILL.md) or subagent file into
/// the library.
fn import(lib: &Library, path: &Path, kind: ItemKind) -> ops::Result<()> {
    let state = lib.lock();
    let dir = lib.dir(&state);
    let (src, id) = match kind {
        ItemKind::Skill => {
            let folder = path.parent().ok_or("skill 폴더를 찾을 수 없습니다")?;
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
        return Err(format!("라이브러리에 같은 이름이 이미 있습니다: {id}"));
    }
    ops::copy_new(&src, &dest)
}

fn set_enabled(
    project: &Path,
    kind: ItemKind,
    name: &str,
    enabled: bool,
    mode: Mode,
) -> ops::Result<()> {
    let target = match kind {
        ItemKind::Skill => Target::Skill(name),
        ItemKind::Agent => Target::Agent(name),
    };
    let claude = project.join(".claude");
    let local = claude.join("settings.local.json");
    let shared = claude.join("settings.json");

    if enabled {
        // Turning back on clears the switch wherever this app could have put it.
        for file in [&local, &shared] {
            if file.is_file() {
                ops::set_enabled(file, target, true)?;
            }
        }
        return Ok(());
    }

    let file = match mode {
        Mode::Personal => &local,
        Mode::Team => &shared,
    };
    let created = !file.exists();
    ops::set_enabled(file, target, false)?;
    // Claude Code only git-excludes settings.local.json when it creates the
    // file itself, so do it here for a file we created.
    if mode == Mode::Personal && created && !ops::is_tracked(file) {
        ops::exclude_add(file, false)?;
    }
    Ok(())
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

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
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
    blocking(app, move |_| {
        set_enabled(Path::new(&project), kind, &name, enabled, mode)
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
    fn toggles_write_the_right_settings_file() {
        let f = fixture();
        set_enabled(&f.project, ItemKind::Skill, "deploy", false, Mode::Personal).unwrap();
        set_enabled(&f.project, ItemKind::Agent, "reviewer", false, Mode::Team).unwrap();
        let local = fs::read_to_string(f.project.join(".claude/settings.local.json")).unwrap();
        let shared = fs::read_to_string(f.project.join(".claude/settings.json")).unwrap();
        assert!(local.contains("\"deploy\": \"off\""));
        assert!(shared.contains("Agent(reviewer)"));
        assert!(exclude(&f).contains("/.claude/settings.local.json"));

        set_enabled(&f.project, ItemKind::Skill, "deploy", true, Mode::Team).unwrap();
        set_enabled(
            &f.project,
            ItemKind::Agent,
            "reviewer",
            true,
            Mode::Personal,
        )
        .unwrap();
        let local = fs::read_to_string(f.project.join(".claude/settings.local.json")).unwrap();
        let shared = fs::read_to_string(f.project.join(".claude/settings.json")).unwrap();
        assert!(!local.contains("deploy"));
        assert!(!shared.contains("reviewer"));
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

        for bad in ["../x", "a/b", ".hidden", ""] {
            assert!(apply(&f.lib, &f.project, ItemKind::Skill, bad, Mode::Team).is_err());
        }
    }
}
