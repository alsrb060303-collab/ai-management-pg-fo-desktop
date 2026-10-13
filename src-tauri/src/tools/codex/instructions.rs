//! AGENTS.md instructions: one global file from `CODEX_HOME`, then one file
//! per folder from the project root down to the project folder, joined under
//! a byte budget.

use std::path::Path;

use super::scan::{Ctx, Trust, untrusted_detail};
use super::util::{file_name, first_heading, get, get_strings, read_text};
use crate::tools::{Item, Kind, Scope, State};

const OVERRIDE: &str = "AGENTS.override.md";
const AGENTS_MD: &str = "AGENTS.md";
/// `project_doc_max_bytes` default (32 KiB).
const DEFAULT_MAX_BYTES: u64 = 32 * 1024;

pub fn instructions(ctx: &Ctx, items: &mut Vec<Item>) {
    global(ctx, items);
    project_docs(ctx, items);
}

fn global(ctx: &Ctx, items: &mut Vec<Item>) {
    let home = &ctx.env.codex_home;
    let over = home.join(OVERRIDE);
    let plain = home.join(AGENTS_MD);
    let over_text = read_text(&over);
    let over_used = over_text.as_deref().is_some_and(|t| !t.trim().is_empty());

    if let Some(text) = &over_text {
        let (state, why) = if over_used {
            (
                State::Active,
                Some(crate::tr!(
                    "{AGENTS_MD} 대신 읽힘",
                    "Read instead of {AGENTS_MD}"
                )),
            )
        } else {
            (State::NotLoaded, Some(empty_detail()))
        };
        items.push(doc_item(Scope::User, &over, text, state, why));
    }
    if let Some(text) = read_text(&plain) {
        let (state, why) = if over_used {
            (
                State::NotLoaded,
                Some(crate::tr!(
                    "{OVERRIDE}가 우선",
                    "{OVERRIDE} takes precedence"
                )),
            )
        } else if text.trim().is_empty() {
            (State::NotLoaded, Some(empty_detail()))
        } else {
            (State::Active, None)
        };
        items.push(doc_item(Scope::User, &plain, &text, state, why));
    }
}

fn project_docs(ctx: &Ctx, items: &mut Vec<Item>) {
    let user = ctx.user_item();
    let names =
        candidate_names(user.and_then(|u| get_strings(u, &["project_doc_fallback_filenames"])));
    let max = user
        .and_then(|u| get(u, &["project_doc_max_bytes"]))
        .and_then(|v| v.as_integer())
        .map_or(DEFAULT_MAX_BYTES, |n| n.max(0) as u64);
    let untrusted = ctx.trust == Trust::Untrusted;
    let mut used: u64 = 0;

    for dir in ctx.project_dirs() {
        let dir_scope = if dir == ctx.project {
            Scope::Project
        } else {
            Scope::Parent
        };
        let existing: Vec<_> = names
            .iter()
            .map(|n| dir.join(n))
            .filter(|p| p.is_file())
            .collect();
        let Some((chosen, rest)) = existing.split_first() else {
            continue;
        };

        let text = read_text(chosen).unwrap_or_default();
        let bytes = text_bytes(chosen, &text);
        let (state, why) = if untrusted {
            (State::NotLoaded, Some(untrusted_detail()))
        } else if text.trim().is_empty() {
            (State::NotLoaded, Some(empty_detail()))
        } else if max == 0 {
            (
                State::NotLoaded,
                Some(crate::tr!(
                    "project_doc_max_bytes = 0이라 프로젝트 문서를 읽지 않음",
                    "Project docs are off (project_doc_max_bytes = 0)"
                )),
            )
        } else if used >= max {
            (
                State::NotLoaded,
                Some(crate::tr!(
                    "앞 파일들이 project_doc_max_bytes({max}바이트)를 다 써서 읽히지 않음",
                    "Not read: earlier files used up project_doc_max_bytes ({max} bytes)"
                )),
            )
        } else if used + bytes > max {
            let kept = max - used;
            used = max;
            (
                State::Limited,
                Some(crate::tr!(
                    "합계 {max}바이트 초과분은 잘림 ({kept}바이트만 읽힘)",
                    "Cut off past {max} bytes in total (only {kept} bytes read)"
                )),
            )
        } else {
            used += bytes;
            (State::Active, None)
        };
        items.push(doc_item(
            doc_scope(chosen, dir_scope),
            chosen,
            &text,
            state,
            why,
        ));

        let winner = file_name(chosen);
        for path in rest {
            let text = read_text(path).unwrap_or_default();
            items.push(doc_item(
                doc_scope(path, dir_scope),
                path,
                &text,
                State::NotLoaded,
                Some(crate::tr!(
                    "같은 폴더의 {winner}가 우선",
                    "{winner} in the same folder takes precedence"
                )),
            ));
        }
    }

    // Codex never reads Claude Code's files unless told to.
    for (name, scope) in [
        ("CLAUDE.md", Scope::Project),
        ("CLAUDE.local.md", Scope::Local),
    ] {
        let path = ctx.project.join(name);
        if names.iter().any(|n| n == name) || !path.is_file() {
            continue;
        }
        let text = read_text(&path).unwrap_or_default();
        items.push(doc_item(
            scope,
            &path,
            &text,
            State::NotLoaded,
            Some(crate::tr!(
                "Codex는 읽지 않음 (project_doc_fallback_filenames에 넣으면 읽음)",
                "Codex doesn't read it (add it to project_doc_fallback_filenames to read it)"
            )),
        ));
    }
}

/// `AGENTS.override.md`, `AGENTS.md`, then the valid fallback names.
fn candidate_names(fallbacks: Option<Vec<String>>) -> Vec<String> {
    let mut names = vec![OVERRIDE.to_string(), AGENTS_MD.to_string()];
    for name in fallbacks.unwrap_or_default() {
        let name = name.trim();
        let invalid = name.is_empty()
            || name == "."
            || name == ".."
            || name.contains(['/', '\0'])
            || (cfg!(windows) && name.contains(['\\', ':']));
        if !invalid && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

/// An override file is usually a personal, uncommitted one.
fn doc_scope(path: &Path, dir_scope: Scope) -> Scope {
    if file_name(path) == OVERRIDE {
        Scope::Local
    } else {
        dir_scope
    }
}

fn text_bytes(path: &Path, text: &str) -> u64 {
    std::fs::metadata(path).map_or(text.len() as u64, |m| m.len())
}

fn empty_detail() -> String {
    crate::tr!("빈 파일", "Empty file")
}

fn doc_item(scope: Scope, path: &Path, text: &str, state: State, why: Option<String>) -> Item {
    let lines = text.lines().count();
    let bytes = text_bytes(path, text);
    let mut facts = vec![crate::tr!(
        "{lines}줄 · {bytes}바이트",
        "{lines} lines · {bytes} bytes"
    )];
    facts.extend(why);
    Item {
        kind: Kind::Instructions,
        scope,
        name: file_name(path),
        path: path.display().to_string(),
        description: first_heading(text),
        state,
        detail: Some(facts.join(" · ")),
    }
}
