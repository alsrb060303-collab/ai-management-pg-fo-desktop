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
    pub items: Vec<PlanItem>,
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

fn plan_items(
    app: &AppHandle,
    slug: &str,
    branch: &str,
    doc: Value,
) -> ops::Result<(Snapshot, Vec<(compile::OutFile, PlanItem)>)> {
    let compiled = compile::compile(&parse_doc(doc)?, &catalog_for(app));
    let wanted: Vec<String> = compiled.files.iter().map(|f| f.path.clone()).collect();
    let snapshot = github::snapshot(app, slug, branch, &wanted)?;
    let composer = app.state::<Composer>();
    let records = composer.written.lock().unwrap_or_else(|e| e.into_inner());
    let items = compiled
        .files
        .into_iter()
        .map(|f| {
            let item = write::plan(&Remote(&snapshot), &key(slug), &f, &records);
            (f, item)
        })
        .collect();
    Ok((snapshot, items))
}

#[tauri::command]
pub async fn composer_plan_remote(
    app: AppHandle,
    slug: String,
    branch: String,
    doc: Value,
) -> Result<RemotePlan, String> {
    blocking(app, move |app| {
        let (snapshot, items) = plan_items(app, &slug, &branch, doc)?;
        Ok(RemotePlan {
            head: snapshot.head,
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
    confirmed: Vec<Confirmation>,
    message: String,
    new_branch: Option<String>,
) -> Result<RemoteWriteResult, String> {
    blocking(app, move |app| {
        let (snapshot, items) = plan_items(app, &slug, &branch, doc)?;
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
                Target::ClaudeMd
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
        );
        assert_eq!(section.status, Status::Update);
        assert!(section.new.starts_with("# Mine\n"), "{}", section.new);
        assert!(section.new.contains("Rules"));

        let personal = write::plan(
            &remote,
            "github:o/r",
            &out(".claude/rules/a.md", "A", false, true),
            &[],
        );
        assert!(matches!(personal.status, Status::Blocked(_)));

        let new = write::plan(
            &remote,
            "github:o/r",
            &out(".claude/rules/b.md", "B", false, false),
            &[],
        );
        assert_eq!(new.status, Status::New);
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
        );
        assert_eq!(item.status, Status::Blocked("not text".into()));
    }
}
