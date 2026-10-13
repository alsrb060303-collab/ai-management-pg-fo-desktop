//! Writing a document's files straight to a GitHub branch, without a clone:
//! the branch's current files are read through the API, planned with the same
//! rules as a local write, and everything that changes goes in one commit.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::write::{self, PlanItem, Project, Status};
use super::{Composer, Confirmation, blocking, catalog_for, compile, parse_doc};
use crate::github::{self, Snapshot};
use crate::library::ops;
use crate::tools::ToolId;

/// Records of files written to GitHub are kept per repository, so a change
/// merged from a pull request still counts as ours on the base branch.
fn key(slug: &str) -> String {
    format!("github:{slug}")
}

struct Remote<'a>(&'a Snapshot);

impl Project for Remote<'_> {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        match self.0.texts.get(path) {
            Some(text) => text.clone().map(Some),
            None => Ok(None),
        }
    }
    fn is_file(&self, path: &str) -> bool {
        self.0.modes.contains_key(path)
    }
    fn is_tracked(&self, path: &str) -> bool {
        self.0.modes.contains_key(path)
    }
    fn refuses_personal(&self) -> Option<String> {
        Some(crate::tr!(
            "'나만' 출력은 git에 올리지 않는 파일이라 GitHub에는 쓰지 않아요",
            "'Just me' outputs stay out of git, so they aren't written to GitHub"
        ))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePlan {
    /// The commit the plan was made on; writing requires the branch to still be there.
    pub head: String,
    /// The tool the files are laid out for: the one asked for, or the one
    /// detected from the branch's files.
    pub tool: ToolId,
    /// Whether `tool` was detected from the branch rather than picked.
    pub detected: bool,
    pub items: Vec<PlanItem>,
}

/// The tool a repository is set up for, from its file list: only one tool's
/// own files (CLAUDE.md, `.claude/` against AGENTS.md, AGENTS.override.md,
/// `.codex/`) decide it; otherwise `fallback` (the user's default tool).
pub fn detect_remote<'a>(paths: impl IntoIterator<Item = &'a str>, fallback: ToolId) -> ToolId {
    let (mut claude, mut codex) = (false, false);
    for path in paths {
        claude |= matches!(path, "CLAUDE.md" | "CLAUDE.local.md") || path.starts_with(".claude/");
        codex |= matches!(path, "AGENTS.md" | "AGENTS.override.md") || path.starts_with(".codex/");
    }
    match (claude, codex) {
        (true, false) => ToolId::Claude,
        (false, true) => ToolId::Codex,
        _ => fallback,
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWriteResult {
    /// The new commit, or `None` when nothing needed writing.
    pub commit: Option<String>,
    /// The branch the commit went to.
    pub branch: String,
    pub written: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

/// Which tool to plan for: the one picked, or one detected from the branch.
#[derive(Clone, Copy)]
enum ToolChoice {
    Picked(ToolId),
    /// Detect from the branch's files, else this (the user's default tool).
    Detect(ToolId),
}

type Planned = (Snapshot, ToolId, Vec<(compile::OutFile, PlanItem)>);

fn plan_items(
    app: &AppHandle,
    slug: &str,
    branch: &str,
    doc: Value,
    choice: ToolChoice,
) -> ops::Result<Planned> {
    let doc = parse_doc(doc)?;
    let catalog = catalog_for(app);
    let tools: Vec<ToolId> = match choice {
        ToolChoice::Picked(tool) => vec![tool],
        ToolChoice::Detect(_) => ToolId::ALL.to_vec(),
    };
    // Before the tool is known, read the files either tool would write.
    let mut compiled: Vec<compile::Compiled> = tools
        .iter()
        .map(|&tool| compile::compile(&doc, &catalog, tool))
        .collect();
    let mut wanted: Vec<String> = Vec::new();
    for path in compiled
        .iter()
        .flat_map(|c| c.files.iter().map(|f| &f.path))
    {
        if !wanted.contains(path) {
            wanted.push(path.clone());
        }
    }
    let snapshot = github::snapshot(app, slug, branch, &wanted)?;
    let tool = match choice {
        ToolChoice::Picked(tool) => tool,
        ToolChoice::Detect(fallback) => {
            detect_remote(snapshot.modes.keys().map(String::as_str), fallback)
        }
    };
    let index = tools.iter().position(|&t| t == tool).unwrap_or(0);
    let compiled = compiled.swap_remove(index);
    let composer = app.state::<Composer>();
    let records = composer.written.lock().unwrap_or_else(|e| e.into_inner());
    let items = compiled
        .files
        .into_iter()
        .map(|f| {
            let item = write::plan(&Remote(&snapshot), &key(slug), &f, &records, tool);
            (f, item)
        })
        .collect();
    Ok((snapshot, tool, items))
}

/// Plans a write to `branch` of `slug`. The files follow `tool`, or with
/// none, the tool detected from the branch's files (else `fallback`): the
/// repository's tool, not the open local project's.
#[tauri::command]
pub async fn composer_plan_remote(
    app: AppHandle,
    slug: String,
    branch: String,
    doc: Value,
    tool: Option<ToolId>,
    fallback: ToolId,
) -> Result<RemotePlan, String> {
    blocking(app, move |app| {
        let choice = match tool {
            Some(tool) => ToolChoice::Picked(tool),
            None => ToolChoice::Detect(fallback),
        };
        let (snapshot, tool, items) = plan_items(app, &slug, &branch, doc, choice)?;
        Ok(RemotePlan {
            head: snapshot.head,
            tool,
            detected: matches!(choice, ToolChoice::Detect(_)),
            items: items.into_iter().map(|(_, item)| item).collect(),
        })
    })
    .await
}

/// Re-plans on the branch as it is now and commits what the user approved.
/// If the branch moved since the preview, nothing is written.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn composer_write_remote(
    app: AppHandle,
    slug: String,
    branch: String,
    head: String,
    doc: Value,
    tool: ToolId,
    confirmed: Vec<Confirmation>,
    message: String,
    new_branch: Option<String>,
) -> Result<RemoteWriteResult, String> {
    blocking(app, move |app| {
        let (snapshot, _, items) =
            plan_items(app, &slug, &branch, doc, ToolChoice::Picked(tool))?;
        if snapshot.head != head {
            return Err(crate::tr!(
                "그사이 '{branch}' 브랜치에 새 커밋이 생겼어요. 미리보기를 다시 불러온 뒤 써 주세요",
                "Branch '{branch}' got new commits meanwhile. Reload the preview and write again"
            ));
        }
        let new_branch = new_branch
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty());
        let mut result = RemoteWriteResult {
            branch: new_branch.clone().unwrap_or_else(|| branch.clone()),
            ..RemoteWriteResult::default()
        };
        let mut files = Vec::new();
        let mut records = Vec::new();
        for (file, item) in &items {
            let approved = confirmed
                .iter()
                .any(|c| c.path == file.path && c.old_hash == item.old_hash);
            let skip = match &item.status {
                Status::Unchanged => continue,
                Status::Blocked(reason) => Some(reason.clone()),
                s if s.needs_confirm() && !approved => Some(crate::tr!(
                    "확인하지 않았거나 확인한 뒤에 파일이 바뀌어서 건너뛰었어요",
                    "Skipped: not confirmed, or the file changed after you confirmed"
                )),
                _ => None,
            };
            if let Some(reason) = skip {
                result.skipped.push((file.path.clone(), reason));
                continue;
            }
            files.push((
                file.path.clone(),
                item.new.clone(),
                snapshot.modes.get(&file.path).cloned(),
            ));
            records.push(write::record_of(&key(&slug), file, item));
            result.written.push(file.path.clone());
        }
        if files.is_empty() {
            return Ok(result);
        }
        result.commit = Some(github::commit_texts(
            app,
            &slug,
            &branch,
            &head,
            files,
            &message,
            new_branch.as_deref(),
        )?);

        let composer = app.state::<Composer>();
        let mut kept = composer.written.lock().unwrap_or_else(|e| e.into_inner());
        for record in records {
            kept.retain(|r| !(r.project == record.project && r.path == record.path));
            kept.push(record);
        }
        let text = serde_json::to_string_pretty(&*kept).map_err(|e| e.to_string())?;
        ops::write_atomic(&composer.data_dir.join("composed.json"), text)?;
        Ok(result)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::compile::{OutFile, Target};
    use std::collections::HashMap;

    fn snapshot(files: &[(&str, &str)]) -> Snapshot {
        Snapshot {
            head: "h".into(),
            modes: files
                .iter()
                .map(|(p, _)| (p.to_string(), "100644".into()))
                .collect(),
            texts: files
                .iter()
                .map(|(p, t)| (p.to_string(), Ok(t.to_string())))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn out(path: &str, content: &str, section: bool, personal: bool) -> OutFile {
        OutFile {
            path: path.into(),
            lines: 1,
            content: content.into(),
            section,
            personal,
            target: if section {
                Target::Instructions
            } else {
                Target::Rules
            },
            node: "o".into(),
            blocks: Vec::new(),
        }
    }

    #[test]
    fn remote_plan_merges_sections_and_refuses_personal_files() {
        let snap = snapshot(&[("CLAUDE.md", "# Mine\n"), ("AGENTS.md", "x")]);
        let remote = Remote(&snap);

        let section = write::plan(
            &remote,
            "github:o/r",
            &out("CLAUDE.md", "Rules", true, false),
            &[],
            ToolId::Claude,
        );
        assert_eq!(section.status, Status::Update);
        assert!(section.new.starts_with("# Mine\n"), "{}", section.new);
        assert!(section.new.contains("Rules"));

        let personal = write::plan(
            &remote,
            "github:o/r",
            &out(".claude/rules/a.md", "A", false, true),
            &[],
            ToolId::Claude,
        );
        assert!(matches!(personal.status, Status::Blocked(_)));

        let new = write::plan(
            &remote,
            "github:o/r",
            &out(".claude/rules/b.md", "B", false, false),
            &[],
            ToolId::Claude,
        );
        assert_eq!(new.status, Status::New);
    }

    #[test]
    fn the_repository_files_pick_the_tool() {
        let detect = |paths: &[&str], fallback| detect_remote(paths.iter().copied(), fallback);
        use ToolId::{Claude, Codex};
        assert_eq!(detect(&["AGENTS.md", "src/a.rs"], Claude), Codex);
        assert_eq!(detect(&[".codex/hooks.json"], Claude), Codex);
        assert_eq!(detect(&["CLAUDE.md", "README.md"], Codex), Claude);
        assert_eq!(detect(&[".claude/settings.json"], Codex), Claude);
        // Both, or neither: the default tool.
        assert_eq!(detect(&["CLAUDE.md", "AGENTS.md"], Codex), Codex);
        assert_eq!(detect(&["CLAUDE.md", "AGENTS.md"], Claude), Claude);
        assert_eq!(detect(&["README.md"], Codex), Codex);
        // Only the root files count; nested ones belong to subprojects.
        assert_eq!(detect(&["docs/CLAUDE.md", "AGENTS.md"], Claude), Codex);
    }

    #[test]
    fn unreadable_remote_files_are_left_alone() {
        let mut snap = snapshot(&[]);
        snap.modes.insert("CLAUDE.md".into(), "100644".into());
        snap.texts
            .insert("CLAUDE.md".into(), Err("not text".into()));
        let item = write::plan(
            &Remote(&snap),
            "github:o/r",
            &out("CLAUDE.md", "R", true, false),
            &[],
            ToolId::Claude,
        );
        assert_eq!(item.status, Status::Blocked("not text".into()));
    }
}
