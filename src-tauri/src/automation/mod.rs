//! Automations: a prompt the app runs with the user's own `claude -p` on a
//! schedule (daily at a time, on chosen weekdays, or every N minutes), with a
//! model, extra instructions, subagents and the tools it may use. Results can
//! be mailed through an email MCP server the user connected to Claude Code.
//!
//! The app is the only writer of `automations.json` and
//! `automation-runs.json`; the MCP server (`mcp_server`) reads them and leaves
//! changes in the bridge inbox (`bridge.rs`). Runs happen while the app (or
//! its tray icon) is running; one missed while it was closed runs once on the
//! next start if it was due within a day.

mod run;
pub mod schedule;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

pub use run::Outcome;
use schedule::{Due, Schedule};

type Result<T> = std::result::Result<T, String>;

pub const EVENT: &str = "automations-changed";
/// How often the scheduler looks for due automations.
const TICK: Duration = Duration::from_secs(20);
/// Runs kept in `automation-runs.json`, newest last.
const MAX_RUNS: usize = 300;
/// Result text kept per run.
const MAX_RESULT_CHARS: usize = 20_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDef {
    pub description: String,
    pub prompt: String,
    /// `haiku`, `sonnet`, `opus`, `fable` or a model id; empty inherits.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub schedule: Schedule,
    /// Folder it runs in; empty: its own folder under the app's data.
    #[serde(default)]
    pub project: String,
    /// Main model; empty uses Claude Code's default.
    #[serde(default)]
    pub model: String,
    pub prompt: String,
    /// Added to the system prompt (instructions, rules and skills from the composer).
    #[serde(default)]
    pub instructions: String,
    /// Subagents by name (`--agents`).
    #[serde(default)]
    pub agents: BTreeMap<String, AgentDef>,
    /// Tools it may use without asking (`--allowedTools`); others are refused.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Mail the result here; empty: don't.
    #[serde(default)]
    pub email: String,
    /// The email MCP server's tools (`mcp__<server>`), allowed when `email` is set.
    #[serde(default)]
    pub mail_tool: String,
    /// The composer document it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    /// When the scheduler last ran (or skipped) it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<i64>,
}

pub const DEFAULT_MAIL_TOOL: &str = "mcp__claude_ai_Gmail";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Running,
    Success,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    Schedule,
    Manual,
    Mcp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub automation_id: String,
    pub name: String,
    pub trigger: Trigger,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    pub status: RunStatus,
    #[serde(default)]
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

pub fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn automations_path(data_dir: &Path) -> PathBuf {
    data_dir.join("automations.json")
}

fn runs_path(data_dir: &Path) -> PathBuf {
    data_dir.join("automation-runs.json")
}

/// Where an automation without a project folder runs (and keeps its agents file).
pub fn work_dir(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("automations").join(id)
}

fn read_list<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        // Kept aside, so the next save does not silently drop what it held.
        let _ = fs::rename(path, path.with_extension("json.corrupt"));
        Vec::new()
    })
}

/// Automations as saved (also read by the MCP server).
pub fn read_automations(data_dir: &Path) -> Vec<Automation> {
    read_list(&automations_path(data_dir))
}

/// Runs, oldest first (also read by the MCP server).
pub fn read_runs(data_dir: &Path) -> Vec<Run> {
    read_list(&runs_path(data_dir))
}

fn write_list<T: Serialize>(path: &Path, list: &[T]) -> Result<()> {
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    crate::library::ops::write_atomic(path, text)
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// Checks and fills in an automation before it is saved.
pub fn normalize(mut a: Automation, old: Option<&Automation>, now: i64) -> Result<Automation> {
    a.name = a.name.trim().to_string();
    if a.name.is_empty() {
        return Err(crate::tr!("이름을 적어 주세요", "Give it a name"));
    }
    if a.prompt.trim().is_empty() {
        return Err(crate::tr!(
            "할 일(프롬프트)을 적어 주세요",
            "Write what it should do (the prompt)"
        ));
    }
    a.schedule.check()?;
    a.project = a.project.trim().to_string();
    if !a.project.is_empty() && !Path::new(&a.project).is_dir() {
        return Err(crate::tr!(
            "폴더를 찾지 못했어요: {}",
            "Folder not found: {}",
            a.project
        ));
    }
    a.model = a.model.trim().to_string();
    let model_ok = |m: &str| {
        m.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '[' | ']'))
    };
    if !model_ok(&a.model) || a.agents.values().any(|g| !model_ok(g.model.trim())) {
        return Err(crate::tr!(
            "모델 이름이 올바르지 않아요",
            "Invalid model name"
        ));
    }
    for (name, agent) in &a.agents {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid || agent.prompt.trim().is_empty() {
            return Err(crate::tr!(
                "서브에이전트 이름은 영어 소문자·숫자·-로, 내용은 비우지 말아 주세요: {name}",
                "Subagent names use a-z, 0-9 and -, and need a prompt: {name}"
            ));
        }
    }
    a.email = a.email.trim().to_string();
    if !a.email.is_empty() && (!a.email.contains('@') || a.email.contains(char::is_whitespace)) {
        return Err(crate::tr!(
            "메일 주소가 올바르지 않아요",
            "Invalid email address"
        ));
    }
    a.mail_tool = a.mail_tool.trim().to_string();
    if !a.email.is_empty() && a.mail_tool.is_empty() {
        a.mail_tool = DEFAULT_MAIL_TOOL.into();
    }
    a.allowed_tools = a
        .allowed_tools
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    a.allowed_tools.dedup();
    if a.id.trim().is_empty() {
        let id = uuid::Uuid::new_v4().simple().to_string();
        a.id = format!("auto-{}", &id[..12]);
    }
    a.created_at = old.map_or(now, |o| o.created_at);
    a.last_fired_at = old.and_then(|o| o.last_fired_at);
    a.updated_at = now;
    Ok(a)
}

/// The base the next run is counted from: the last run, or the last edit.
fn base(a: &Automation) -> i64 {
    a.last_fired_at.unwrap_or(0).max(a.updated_at)
}

pub fn next_run_at(a: &Automation) -> Option<i64> {
    if !a.enabled {
        return None;
    }
    a.schedule
        .next_after(&schedule::from_ms(base(a)))
        .map(|t| t.timestamp_millis())
}

pub struct Automations {
    data_dir: PathBuf,
    /// Held while reading-and-writing the two files.
    lock: Mutex<()>,
    running: Mutex<HashSet<String>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(flatten)]
    pub automation: Automation,
    pub next_run_at: Option<i64>,
    pub running: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub claude_found: bool,
    pub automations: Vec<Item>,
    /// Newest first.
    pub runs: Vec<Run>,
}

impl Automations {
    pub fn manage(app: &AppHandle) -> tauri::Result<()> {
        let data_dir = app.path().app_data_dir()?;
        let this = Automations {
            data_dir,
            lock: Mutex::new(()),
            running: Mutex::new(HashSet::new()),
        };
        // Runs cut off when the app last quit.
        let _ = this.edit_runs(|runs| {
            for r in runs.iter_mut().filter(|r| r.status == RunStatus::Running) {
                r.status = RunStatus::Failed;
                r.error = Some(crate::tr!(
                    "앱이 종료되어 중단됐어요",
                    "Stopped because the app quit"
                ));
            }
        });
        app.manage(this);
        Ok(())
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn edit<T>(&self, f: impl FnOnce(&mut Vec<Automation>) -> Result<T>) -> Result<T> {
        let _g = self.guard();
        let path = automations_path(&self.data_dir);
        let mut list = read_list(&path);
        let out = f(&mut list)?;
        write_list(&path, &list)?;
        Ok(out)
    }

    fn edit_runs(&self, f: impl FnOnce(&mut Vec<Run>)) -> Result<()> {
        let _g = self.guard();
        let path = runs_path(&self.data_dir);
        let mut runs: Vec<Run> = read_list(&path);
        f(&mut runs);
        let extra = runs.len().saturating_sub(MAX_RUNS);
        runs.drain(..extra);
        write_list(&path, &runs)
    }

    pub fn list(&self) -> Vec<Automation> {
        let _g = self.guard();
        read_automations(&self.data_dir)
    }

    fn is_running(&self, id: &str) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }

    pub fn overview(&self) -> Overview {
        let automations = self
            .list()
            .into_iter()
            .map(|a| Item {
                next_run_at: next_run_at(&a),
                running: self.is_running(&a.id),
                automation: a,
            })
            .collect();
        let mut runs = {
            let _g = self.guard();
            read_runs(&self.data_dir)
        };
        runs.reverse();
        runs.truncate(100);
        Overview {
            claude_found: crate::claude_mod::find_claude().is_some(),
            automations,
            runs,
        }
    }

    pub fn save(&self, automation: Automation) -> Result<Automation> {
        let now = now_ms();
        self.edit(|list| {
            let pos = list.iter().position(|a| a.id == automation.id);
            let saved = normalize(automation, pos.map(|i| &list[i]), now)?;
            match pos {
                Some(i) => list[i] = saved.clone(),
                None => list.push(saved.clone()),
            }
            Ok(saved)
        })
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.edit(|list| {
            list.retain(|a| a.id != id);
            Ok(())
        })?;
        let _ = fs::remove_dir_all(work_dir(&self.data_dir, id));
        self.edit_runs(|runs| runs.retain(|r| r.automation_id != id))
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let now = now_ms();
        self.edit(|list| {
            let a = list
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| not_found(id))?;
            if a.enabled != enabled {
                a.enabled = enabled;
                // Turned back on: count from now, not from when it was turned off.
                a.updated_at = now;
            }
            Ok(())
        })
    }

    fn mark_fired(&self, id: &str, at: i64) -> Result<()> {
        self.edit(|list| {
            if let Some(a) = list.iter_mut().find(|a| a.id == id) {
                a.last_fired_at = Some(at);
            }
            Ok(())
        })
    }
}

fn not_found(id: &str) -> String {
    crate::tr!("자동화를 찾지 못했어요: {id}", "Automation not found: {id}")
}

pub fn changed(app: &AppHandle) {
    let _ = app.emit(EVENT, ());
}

/// Starts a run now; the run's id. One run per automation at a time.
pub fn start(app: &AppHandle, id: &str, trigger: Trigger) -> Result<String> {
    let state = app.state::<Automations>();
    let automation = state
        .list()
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| not_found(id))?;
    {
        let mut running = state.running.lock().unwrap_or_else(|e| e.into_inner());
        if !running.insert(automation.id.clone()) {
            return Err(crate::tr!("이미 실행 중이에요", "It is already running"));
        }
    }
    let run_id = format!("run-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let record = Run {
        id: run_id.clone(),
        automation_id: automation.id.clone(),
        name: automation.name.clone(),
        trigger,
        started_at: now_ms(),
        ended_at: None,
        status: RunStatus::Running,
        result: String::new(),
        error: None,
        cost_usd: None,
    };
    if let Err(e) = state.edit_runs(|runs| runs.push(record)) {
        state
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&automation.id);
        return Err(e);
    }
    changed(app);
    let app = app.clone();
    let data_dir = state.data_dir.clone();
    let id = run_id.clone();
    std::thread::spawn(move || {
        let outcome = run::execute(&data_dir, &automation);
        let state = app.state::<Automations>();
        let _ = state.edit_runs(|runs| {
            if let Some(r) = runs.iter_mut().find(|r| r.id == id) {
                r.ended_at = Some(now_ms());
                r.status = if outcome.ok {
                    RunStatus::Success
                } else {
                    RunStatus::Failed
                };
                r.result = truncate(&outcome.result, MAX_RESULT_CHARS);
                r.error = outcome.error.clone();
                r.cost_usd = outcome.cost_usd;
            }
        });
        state
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&automation.id);
        changed(&app);
        notify(&app, &automation.name, &outcome);
    });
    Ok(run_id)
}

fn notify(app: &AppHandle, name: &str, outcome: &Outcome) {
    use tauri_plugin_notification::NotificationExt;
    let title = if outcome.ok {
        crate::tr!("자동화 완료: {name}", "Automation finished: {name}")
    } else {
        crate::tr!("자동화 실패: {name}", "Automation failed: {name}")
    };
    let body = outcome
        .error
        .clone()
        .unwrap_or_else(|| truncate(outcome.result.trim(), 160));
    let _ = app.notification().builder().title(title).body(body).show();
}

/// The scheduler: runs due automations while the app is running.
pub fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            tick(&app);
            std::thread::sleep(TICK);
        }
    });
}

fn tick(app: &AppHandle) {
    let state = app.state::<Automations>();
    let now = chrono::Local::now();
    let mut any = false;
    for a in state.list() {
        if !a.enabled || state.is_running(&a.id) {
            continue;
        }
        match schedule::due(&a.schedule, &schedule::from_ms(base(&a)), &now) {
            Due::No => {}
            Due::Skip => {
                let _ = state.mark_fired(&a.id, now.timestamp_millis());
                any = true;
            }
            Due::Run => {
                // Marked first, so a run that fails to start isn't retried every tick.
                let _ = state.mark_fired(&a.id, now.timestamp_millis());
                let _ = start(app, &a.id, Trigger::Schedule);
                any = true;
            }
        }
    }
    if any {
        changed(app);
    }
}

/// A change the MCP server left in the bridge inbox.
pub fn apply_request(app: &AppHandle, request: crate::bridge::Request) {
    use crate::bridge::Request;
    let state = app.state::<Automations>();
    let _ = match request {
        Request::SaveAutomation { automation } => serde_json::from_value(automation)
            .map_err(|e| e.to_string())
            .and_then(|a| state.save(a))
            .map(drop),
        Request::SetAutomationEnabled { id, enabled } => state.set_enabled(&id, enabled),
        Request::RunAutomation { id } => start(app, &id, Trigger::Mcp).map(drop),
        _ => Ok(()),
    };
    changed(app);
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn automations_overview(app: AppHandle) -> Result<Overview> {
    blocking(move || Ok(app.state::<Automations>().overview())).await
}

#[tauri::command]
pub async fn automation_save(app: AppHandle, automation: Automation) -> Result<Automation> {
    blocking(move || {
        let saved = app.state::<Automations>().save(automation)?;
        changed(&app);
        Ok(saved)
    })
    .await
}

#[tauri::command]
pub async fn automation_delete(app: AppHandle, id: String) -> Result<()> {
    blocking(move || {
        app.state::<Automations>().delete(&id)?;
        changed(&app);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn automation_set_enabled(app: AppHandle, id: String, enabled: bool) -> Result<()> {
    blocking(move || {
        app.state::<Automations>().set_enabled(&id, enabled)?;
        changed(&app);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn automation_run(app: AppHandle, id: String) -> Result<String> {
    blocking(move || start(&app, &id, Trigger::Manual)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Automation {
        serde_json::from_value(serde_json::json!({
            "name": " Morning news ",
            "schedule": { "kind": "daily", "time": "10:00", "days": [] },
            "model": "sonnet",
            "prompt": "Collect today's AI news.",
            "agents": {
                "scraper": { "description": "Scrapes pages", "prompt": "Fetch and summarize.", "model": "haiku" }
            },
            "allowedTools": ["WebSearch", " WebFetch ", ""],
            "email": "me@example.com"
        }))
        .unwrap()
    }

    #[test]
    fn saving_fills_in_ids_times_and_the_mail_tool() {
        let a = normalize(sample(), None, 1000).unwrap();
        assert!(a.id.starts_with("auto-"));
        assert_eq!(a.name, "Morning news");
        assert!(a.enabled);
        assert_eq!((a.created_at, a.updated_at), (1000, 1000));
        assert_eq!(a.allowed_tools, ["WebSearch", "WebFetch"]);
        assert_eq!(a.mail_tool, DEFAULT_MAIL_TOOL);

        let mut edited = a.clone();
        edited.last_fired_at = None;
        let mut old = a.clone();
        old.last_fired_at = Some(1500);
        let again = normalize(edited, Some(&old), 2000).unwrap();
        assert_eq!((again.created_at, again.updated_at), (1000, 2000));
        assert_eq!(again.last_fired_at, Some(1500));
    }

    #[test]
    fn bad_input_is_refused() {
        let mut a = sample();
        a.prompt = " ".into();
        assert!(normalize(a, None, 0).is_err());
        let mut a = sample();
        a.email = "not an email".into();
        assert!(normalize(a, None, 0).is_err());
        let mut a = sample();
        a.model = "sonnet; rm".into();
        assert!(normalize(a, None, 0).is_err());
        let mut a = sample();
        a.agents.insert("Bad Name".into(), AgentDef::default());
        assert!(normalize(a, None, 0).is_err());
        let mut a = sample();
        a.project = "/surely/not/a/folder".into();
        assert!(normalize(a, None, 0).is_err());
    }

    #[test]
    fn the_store_round_trips_and_trims_runs() {
        let dir = std::env::temp_dir().join(format!("encela-auto-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let store = Automations {
            data_dir: dir.clone(),
            lock: Mutex::new(()),
            running: Mutex::new(HashSet::new()),
        };
        let saved = store.save(sample()).unwrap();
        assert_eq!(read_automations(&dir), std::slice::from_ref(&saved));
        store.set_enabled(&saved.id, false).unwrap();
        assert!(!store.list()[0].enabled);
        assert_eq!(next_run_at(&store.list()[0]), None);

        for i in 0..MAX_RUNS + 5 {
            store
                .edit_runs(|runs| {
                    runs.push(Run {
                        id: format!("r{i}"),
                        automation_id: saved.id.clone(),
                        name: "n".into(),
                        trigger: Trigger::Manual,
                        started_at: i as i64,
                        ended_at: None,
                        status: RunStatus::Success,
                        result: String::new(),
                        error: None,
                        cost_usd: None,
                    })
                })
                .unwrap();
        }
        let runs = read_runs(&dir);
        assert_eq!(runs.len(), MAX_RUNS);
        assert_eq!(runs[0].id, "r5");
        assert_eq!(store.overview().runs[0].id, format!("r{}", MAX_RUNS + 4));

        store.delete(&saved.id).unwrap();
        assert!(read_automations(&dir).is_empty() && read_runs(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn long_results_are_cut_on_a_char_boundary() {
        assert_eq!(truncate("가나다라", 2), "가나…");
        assert_eq!(truncate("ab", 5), "ab");
    }
}
