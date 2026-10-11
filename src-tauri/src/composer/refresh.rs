//! "최신 받아오기": brings the bundled third-party blocks up to date with their
//! upstream repositories.
//!
//! Only the same file in the same repository is fetched, at that repository's
//! latest commit, and only if its license file still carries the same license.
//! Blocks are Markdown; nothing fetched is ever executed. Updated copies go to
//! the app's cache folder and replace the bundled text in the catalog.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use super::catalog::{self, Block};
use crate::library::ops;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    /// Titles of blocks whose text changed.
    pub updated: Vec<String>,
    pub unchanged: usize,
    /// `(title, reason)`.
    pub failed: Vec<(String, String)>,
}

/// What one bundled block points at upstream.
struct Upstream<'a> {
    source: &'a str,
    stem: &'a str,
    block: Block,
    license_path: String,
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        // The OS certificate store, so corporate proxies with their own CA work.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .user_agent(concat!("ai-management/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get(agent: &Agent, url: &str, accept: &str) -> Result<String, String> {
    agent
        .get(url)
        .header("Accept", accept)
        .call()
        .map_err(|e| crate::tr!("받아오지 못했어요 ({e})", "Couldn't download ({e})"))?
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| crate::tr!("읽지 못했어요 ({e})", "Couldn't read the response ({e})"))
}

/// The repository's HEAD commit from git's smart-HTTP ref advertisement (what
/// `git ls-remote` reads): no API token or API rate limit involved.
fn latest_commit(agent: &Agent, repo: &str) -> Result<String, String> {
    let url = format!("https://github.com/{repo}.git/info/refs?service=git-upload-pack");
    let refs = get(agent, &url, "*/*")?;
    head_commit(&refs).ok_or_else(|| {
        crate::tr!(
            "최신 커밋을 알 수 없어요",
            "Couldn't find the latest commit"
        )
    })
}

/// Finds `<sha> HEAD` in a pkt-line ref advertisement.
pub(super) fn head_commit(refs: &str) -> Option<String> {
    let at = refs.find(" HEAD")?;
    let sha = refs.get(at.checked_sub(40)?..at)?;
    sha.chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| sha.to_ascii_lowercase())
}

/// Whether a license file still grants the license the block was bundled under.
pub(super) fn license_matches(spdx: &str, text: &str) -> bool {
    match spdx {
        "MIT" => {
            text.contains("MIT License")
                || text.contains("Permission is hereby granted, free of charge")
        }
        "Apache-2.0" => text.contains("Apache License") && text.contains("Version 2.0"),
        _ => false,
    }
}

/// The cache copy of a block: the bundled frontmatter with the upstream
/// description, tools, commit and body.
pub(super) fn cached_text(
    block: &Block,
    license_path: &str,
    upstream: &str,
    commit: &str,
) -> Option<String> {
    let origin = block.origin.as_ref()?;
    let (fm, body) = catalog::split(upstream);
    let clean = |s: &str| s.replace('"', "'").replace('\n', " ");
    let description = fm
        .get("description")
        .map(|d| clean(d))
        .or_else(|| block.description.clone())?;
    let kind = match block.kind {
        catalog::BlockKind::Instruction => "instruction",
        catalog::BlockKind::Rule => "rule",
        catalog::BlockKind::Skill => "skill",
        catalog::BlockKind::Agent => "agent",
    };
    let mut out = format!(
        "---\nkind: {kind}\ntitle: {}\nsummary: {}\ncategory: {}\n",
        block.title, block.summary, block.category
    );
    // Our own English display text, like the Korean, is kept from the bundle.
    if let Some(en) = &block.en {
        out.push_str(&format!(
            "title_en: {}\nsummary_en: {}\ncategory_en: {}\n",
            en.title, en.summary, en.category
        ));
    }
    out.push_str(&format!(
        "name: {}\ndescription: \"{description}\"\n",
        block.name
    ));
    if let Some(tools) = fm.get("tools").or(block.tools.as_ref()) {
        out.push_str(&format!("tools: {tools}\n"));
    }
    out.push_str(&format!(
        "license: {}\ncopyright: \"{}\"\nsource_repo: {}\nsource_commit: {commit}\nsource_path: {}\nlicense_path: {license_path}\n---\n{}\n",
        origin.license,
        clean(&origin.copyright),
        origin.repo,
        origin.path,
        body.trim_end()
    ));
    Some(out)
}

pub fn refresh(cache: &Path) -> RefreshResult {
    let mut result = RefreshResult::default();
    let current: HashMap<String, Block> = catalog::external(Some(cache))
        .into_iter()
        .map(|b| (b.id.clone(), b))
        .collect();

    let items: Vec<Upstream> = catalog::bundled_external()
        .iter()
        .filter_map(|(source, stem, text)| {
            let block = catalog::parse_external(source, stem, text)?;
            let (fm, _) = catalog::split(text);
            Some(Upstream {
                source,
                stem,
                license_path: fm.get("license_path")?.clone(),
                block,
            })
        })
        .collect();

    let agent = agent();
    let mut commits: HashMap<String, Result<String, String>> = HashMap::new();
    let mut licenses: HashMap<(String, String), Result<(), String>> = HashMap::new();

    for item in items {
        let title = item.block.display_title().to_string();
        let Some(origin) = item.block.origin.clone() else {
            continue;
        };
        let commit = commits
            .entry(origin.repo.clone())
            .or_insert_with(|| latest_commit(&agent, &origin.repo))
            .clone();
        let commit = match commit {
            Ok(c) => c,
            Err(e) => {
                result.failed.push((title, e));
                continue;
            }
        };
        let now = current.get(&item.block.id);
        if now
            .and_then(|b| b.origin.as_ref())
            .is_some_and(|o| o.commit == commit)
        {
            result.unchanged += 1;
            continue;
        }

        let license = licenses
            .entry((origin.repo.clone(), item.license_path.clone()))
            .or_insert_with(|| {
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{commit}/{}",
                    origin.repo, item.license_path
                );
                get(&agent, &url, "text/plain").and_then(|text| {
                    if license_matches(&origin.license, &text) {
                        Ok(())
                    } else {
                        Err(crate::tr!(
                            "라이선스가 {}이(가) 아닌 것 같아서 건너뛰었어요",
                            "Skipped: the license no longer looks like {}",
                            origin.license
                        ))
                    }
                })
            })
            .clone();
        if let Err(e) = license {
            result.failed.push((title, e));
            continue;
        }

        let url = format!(
            "https://raw.githubusercontent.com/{}/{commit}/{}",
            origin.repo, origin.path
        );
        let upstream = match get(&agent, &url, "text/plain") {
            Ok(text) => text,
            Err(e) => {
                result.failed.push((
                    title,
                    crate::tr!(
                        "{e}. 원본 파일이 옮겨졌을 수 있어요",
                        "{e}. The original file may have moved"
                    ),
                ));
                continue;
            }
        };
        let Some(text) = cached_text(&item.block, &item.license_path, &upstream, &commit) else {
            result.failed.push((
                title,
                crate::tr!(
                    "원본 형식을 읽지 못했어요",
                    "Couldn't parse the original file"
                ),
            ));
            continue;
        };
        let Some(fresh) = catalog::parse_external(item.source, item.stem, &text) else {
            result.failed.push((
                title,
                crate::tr!(
                    "원본 형식을 읽지 못했어요",
                    "Couldn't parse the original file"
                ),
            ));
            continue;
        };
        let changed =
            now.is_none_or(|b| b.body != fresh.body || b.description != fresh.description);
        match ops::write_atomic(&catalog::cache_path(cache, item.source, item.stem), &text) {
            Ok(()) if changed => result.updated.push(title),
            Ok(()) => result.unchanged += 1,
            Err(e) => result.failed.push((title, e)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_head_from_a_ref_advertisement() {
        let sha = "bb92a77741419a4ab5f06e711a283343f1ada0c3";
        let refs = format!(
            "001e# service=git-upload-pack\n00000155{sha} HEAD\0multi_ack thin-pack symref=HEAD:refs/heads/main\n003f{sha} refs/heads/main\n0000"
        );
        assert_eq!(head_commit(&refs).as_deref(), Some(sha));
        assert_eq!(head_commit("0000"), None);
        assert_eq!(head_commit("0008zz HEAD"), None);
    }

    #[test]
    fn recognizes_the_bundled_licenses_only() {
        assert!(license_matches(
            "MIT",
            "MIT License\n\nCopyright (c) 2025 X\n\nPermission is hereby granted"
        ));
        assert!(license_matches(
            "Apache-2.0",
            "Apache License\n Version 2.0, January 2004"
        ));
        assert!(!license_matches("MIT", "All rights reserved."));
        assert!(!license_matches("Apache-2.0", "MIT License"));
        assert!(!license_matches("GPL-3.0", "GNU GENERAL PUBLIC LICENSE"));
    }

    #[test]
    fn cached_copy_keeps_provenance_and_takes_upstream_text() {
        let (source, stem, text) = catalog::bundled_external()[0];
        let block = catalog::parse_external(source, stem, text).unwrap();
        let upstream =
            "---\nname: whatever\ndescription: New \"quoted\" description\n---\n\n# New body\n";
        let commit = "b".repeat(40);
        let cached = cached_text(&block, "LICENSE", upstream, &commit).unwrap();
        let fresh = catalog::parse_external(source, stem, &cached).unwrap();
        assert_eq!(fresh.body, "# New body");
        assert_eq!(
            fresh.description.as_deref(),
            Some("New 'quoted' description")
        );
        assert_eq!(fresh.name, block.name);
        assert_eq!(fresh.title, block.title);
        assert!(block.en.is_some());
        assert_eq!(fresh.en, block.en);
        let (a, b) = (fresh.origin.unwrap(), block.origin.unwrap());
        assert_eq!(a.commit, commit);
        assert_eq!((a.repo, a.path, a.license), (b.repo, b.path, b.license));
    }

    #[test]
    fn catalog_uses_a_cached_copy_of_the_same_upstream_file_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (source, stem, text) = catalog::bundled_external()[0];
        let block = catalog::parse_external(source, stem, text).unwrap();
        let path = catalog::cache_path(tmp.path(), source, stem);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let fresh = cached_text(
            &block,
            "LICENSE",
            "---\ndescription: d\n---\nNew",
            &"c".repeat(40),
        )
        .unwrap();
        std::fs::write(&path, &fresh).unwrap();
        let found = catalog::external(Some(tmp.path()));
        assert_eq!(found.iter().find(|b| b.id == block.id).unwrap().body, "New");

        // A copy cached before the English fields existed still gets them.
        let mut old = String::new();
        for line in fresh.lines().filter(|l| !l.contains("_en: ")) {
            old.push_str(line);
            old.push('\n');
        }
        std::fs::write(&path, &old).unwrap();
        let found = catalog::external(Some(tmp.path()));
        let cached = found.iter().find(|b| b.id == block.id).unwrap();
        assert_eq!(cached.body, "New");
        assert_eq!(cached.en, block.en);

        // A cache file pointing elsewhere is ignored.
        let origin = block.origin.as_ref().unwrap();
        std::fs::write(&path, fresh.replace(&origin.repo, "evil/repo")).unwrap();
        let found = catalog::external(Some(tmp.path()));
        assert_eq!(
            found.iter().find(|b| b.id == block.id).unwrap().body,
            block.body
        );
    }

    /// Hits GitHub; run with `cargo test -- --ignored live_refresh`.
    #[test]
    #[ignore]
    fn live_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let result = refresh(tmp.path());
        println!("{result:#?}");
        assert!(result.failed.is_empty(), "{:?}", result.failed);
        assert_eq!(
            result.updated.len() + result.unchanged,
            catalog::bundled_external().len()
        );
    }
}
