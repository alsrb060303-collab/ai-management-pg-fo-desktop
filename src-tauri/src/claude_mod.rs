//! Installs the app's Claude Code mod (`claude-mod/branch-notes`) and the
//! recommended mods with the user's own `claude` command: the mod's files are
//! written to `<app data>/claude-mod` (a local marketplace), then
//! `claude plugin marketplace add` and `claude plugin install`. Claude Code's
//! own settings and sign-in are never read or written by the app.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// The mod's files, compiled into the app (its tests stay in the repo).
const FILES: &[(&str, &str)] = &[
    (
        ".claude-plugin/marketplace.json",
        include_str!("../../claude-mod/.claude-plugin/marketplace.json"),
    ),
    (
        "branch-notes/.claude-plugin/plugin.json",
        include_str!("../../claude-mod/branch-notes/.claude-plugin/plugin.json"),
    ),
    (
        "branch-notes/hooks/hooks.json",
        include_str!("../../claude-mod/branch-notes/hooks/hooks.json"),
    ),
    (
        "branch-notes/hooks/register.tsx",
        include_str!("../../claude-mod/branch-notes/hooks/register.tsx"),
    ),
    (
        "branch-notes/hooks/core.ts",
        include_str!("../../claude-mod/branch-notes/hooks/core.ts"),
    ),
    (
        "branch-notes/types/index.d.ts",
        include_str!("../../claude-mod/branch-notes/types/index.d.ts"),
    ),
];

pub const MARKETPLACE: &str = "ai-management";
pub const PLUGIN: &str = "branch-notes";
/// Mods need this Claude Code version or newer.
const MIN_VERSION: (u32, u32, u32) = (2, 1, 287);

type Result<T> = std::result::Result<T, String>;

/// The user's `claude`: on PATH, or where its installers put it.
fn find_claude() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".claude").join("local"));
    }
    for dir in dirs {
        for name in names {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Runs `claude` with `args`; stdout, or stderr (or stdout) as the error.
fn claude(args: &[&str]) -> Result<String> {
    let path = find_claude().ok_or_else(|| {
        crate::tr!(
            "Claude Code(claude 명령)를 찾지 못했어요. 설치한 뒤 다시 시도해 주세요",
            "Couldn't find Claude Code (the claude command). Install it and try again"
        )
    })?;
    // Rust quotes arguments for cmd.exe itself when the program is a .cmd.
    let mut cmd = Command::new(&path);
    cmd.args(args)
        .stdin(Stdio::null())
        .env("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| {
        crate::tr!(
            "claude를 실행하지 못했어요: {e}",
            "Couldn't run claude: {e}"
        )
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

/// Runs `claude` in `dir` with `args`, giving up after `timeout`: stdout and
/// whether it exited successfully, or why it couldn't run. MCP health checks
/// can wait on a server that never answers, hence the limit.
pub(crate) fn claude_in(
    dir: &std::path::Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<(bool, String, String)> {
    claude_with_input(dir, args, None, timeout)
}

/// [`claude_in`] with `input` written to its stdin (`claude -p` reads it
/// after the prompt), for text too long for a command line.
pub(crate) fn claude_with_input(
    dir: &std::path::Path,
    args: &[&str],
    input: Option<String>,
    timeout: std::time::Duration,
) -> Result<(bool, String, String)> {
    use std::io::{Read, Write};
    let path = find_claude().ok_or_else(|| {
        crate::tr!(
            "Claude Code(claude 명령)를 찾지 못했어요. 설치한 뒤 다시 시도해 주세요",
            "Couldn't find Claude Code (the claude command). Install it and try again"
        )
    })?;
    let mut cmd = Command::new(&path);
    cmd.args(args)
        .current_dir(dir)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1")
        .env("NO_COLOR", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| {
        crate::tr!(
            "claude를 실행하지 못했어요: {e}",
            "Couldn't run claude: {e}"
        )
    })?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // Written on a thread and closed after, so claude sees the end.
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    // Read both pipes on threads so a full pipe never blocks the child.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = stdout.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = stderr.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::tr!(
                    "claude가 {}초 안에 끝나지 않았어요",
                    "claude didn't finish within {} seconds",
                    timeout.as_secs()
                ));
            }
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok((status.success(), stdout, stderr))
}

/// `2.1.296 (Claude Code)` → (2, 1, 296).
fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModStatus {
    /// `None` when `claude` isn't found.
    pub claude_version: Option<String>,
    /// Mods need 2.1.287 or newer.
    pub supported: bool,
    /// `plugin@marketplace` ids installed for the user.
    pub installed: Vec<String>,
}

fn installed() -> Vec<String> {
    let Ok(json) = claude(&["plugin", "list", "--json"]) else {
        return Vec::new();
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    // A list of plugins, or an object holding one; each names its id.
    let list = value
        .as_array()
        .cloned()
        .or_else(|| value["plugins"].as_array().cloned())
        .or_else(|| value["installed"].as_array().cloned())
        .unwrap_or_default();
    list.iter()
        .filter_map(|p| {
            p["id"].as_str().map(str::to_string).or_else(|| {
                let name = p["name"].as_str()?;
                let market = p["marketplace"].as_str()?;
                Some(format!("{name}@{market}"))
            })
        })
        .collect()
}

pub fn status() -> ModStatus {
    let version = claude(&["--version"]).ok();
    let supported = version
        .as_deref()
        .and_then(parse_version)
        .is_some_and(|v| v >= MIN_VERSION);
    ModStatus {
        installed: if version.is_some() {
            installed()
        } else {
            Vec::new()
        },
        claude_version: version,
        supported,
    }
}

/// Writes the mod's files under `dir` (replacing older ones).
fn write_files(dir: &Path) -> Result<()> {
    for (rel, text) in FILES {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Adds a marketplace; one already added is updated instead.
fn add_marketplace(source: &str, name: &str) -> Result<()> {
    match claude(&["plugin", "marketplace", "add", source]) {
        Ok(_) => Ok(()),
        Err(e) if e.to_lowercase().contains("already") => {
            claude(&["plugin", "marketplace", "update", name]).map(drop)
        }
        Err(e) => Err(e),
    }
}

fn install_plugin(id: &str) -> Result<()> {
    match claude(&["plugin", "install", id, "--scope", "user"]) {
        Ok(_) => Ok(()),
        Err(e) if e.to_lowercase().contains("already installed") => Ok(()),
        Err(e) => Err(e),
    }
}

fn check_supported() -> Result<()> {
    let st = status();
    if st.claude_version.is_none() {
        return Err(crate::tr!(
            "Claude Code(claude 명령)를 찾지 못했어요. 설치한 뒤 다시 시도해 주세요",
            "Couldn't find Claude Code (the claude command). Install it and try again"
        ));
    }
    if !st.supported {
        return Err(crate::tr!(
            "mods는 Claude Code 2.1.287 이상에서 동작해요. `claude update`로 업데이트해 주세요",
            "Mods need Claude Code 2.1.287 or newer. Update with `claude update`"
        ));
    }
    Ok(())
}

pub fn install_own(dir: &Path) -> Result<()> {
    check_supported()?;
    write_files(dir)?;
    // `./` or an absolute path: claude reads it as a local folder.
    add_marketplace(&dir.to_string_lossy(), MARKETPLACE)?;
    install_plugin(&format!("{PLUGIN}@{MARKETPLACE}"))
}

/// Installs a recommended mod. A marketplace inside a subfolder of its
/// repository is cloned (or updated) under `sources` first and added from
/// there; one at a repository's root is added by `owner/repo`.
pub fn install_recommended(rec: &Recommended, sources: &Path) -> Result<()> {
    check_supported()?;
    let source = match rec.subdir {
        None => rec.repo.to_string(),
        Some(sub) => {
            let dir = sources.join(rec.marketplace);
            if dir.join(".git").is_dir() {
                crate::git::run(&dir, &["pull", "--ff-only", "-q"])?;
            } else {
                std::fs::create_dir_all(sources).map_err(|e| e.to_string())?;
                let url = format!("https://github.com/{}.git", rec.repo);
                let target = dir.to_string_lossy();
                crate::git::run(
                    sources,
                    &["clone", "-q", "--depth", "1", "--", &url, &target],
                )?;
            }
            dir.join(sub).to_string_lossy().into_owned()
        }
    };
    add_marketplace(&source, rec.marketplace)?;
    install_plugin(rec.id)
}

pub fn uninstall(id: &str) -> Result<()> {
    claude(&["plugin", "uninstall", id, "--scope", "user"]).map(drop)
}

fn mod_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("claude-mod"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn claude_mod_status() -> ModStatus {
    tauri::async_runtime::spawn_blocking(status)
        .await
        .unwrap_or(ModStatus {
            claude_version: None,
            supported: false,
            installed: Vec::new(),
        })
}

#[tauri::command]
pub async fn claude_mod_install(app: AppHandle) -> Result<()> {
    let dir = mod_dir(&app)?;
    blocking(move || install_own(&dir)).await
}

#[tauri::command]
pub async fn claude_mod_install_recommended(app: AppHandle, id: String) -> Result<()> {
    let rec = RECOMMENDED
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "unknown mod".to_string())?;
    let sources = mod_dir(&app)?.with_file_name("mod-sources");
    blocking(move || install_recommended(rec, &sources)).await
}

#[tauri::command]
pub async fn claude_mod_uninstall(id: String) -> Result<()> {
    // Only ids this app installs.
    let own = format!("{PLUGIN}@{MARKETPLACE}");
    if id != own && !RECOMMENDED.iter().any(|r| r.id == id) {
        return Err("unknown mod".into());
    }
    blocking(move || uninstall(&id)).await
}

/// Mods the app recommends, checked against their repositories (sources and
/// licenses in docs/research.md). Only these can be installed from the app;
/// the list the UI shows is `src/features/mods/recommended.ts`.
pub struct Recommended {
    /// `plugin@marketplace`.
    pub id: &'static str,
    /// GitHub `owner/repo`.
    pub repo: &'static str,
    /// The marketplace's folder inside the repository, when not its root.
    pub subdir: Option<&'static str>,
    pub marketplace: &'static str,
}

const PLAYGROUND: &str = "anthropics/claude-code-playground";
const PLAYGROUND_MODS: &str = "claude-code-playground-mods";

pub const RECOMMENDED: &[Recommended] = &[
    Recommended {
        id: "token-weather@claude-code-playground-mods",
        repo: PLAYGROUND,
        subdir: Some("claude-code/mods"),
        marketplace: PLAYGROUND_MODS,
    },
    Recommended {
        id: "blast-radius@claude-code-playground-mods",
        repo: PLAYGROUND,
        subdir: Some("claude-code/mods"),
        marketplace: PLAYGROUND_MODS,
    },
    Recommended {
        id: "replay-theater@claude-code-playground-mods",
        repo: PLAYGROUND,
        subdir: Some("claude-code/mods"),
        marketplace: PLAYGROUND_MODS,
    },
    Recommended {
        id: "claude-hud@claude-hud",
        repo: "jarrodwatts/claude-hud",
        subdir: None,
        marketplace: "claude-hud",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(parse_version("2.1.296 (Claude Code)"), Some((2, 1, 296)));
        assert!(parse_version("2.1.296 (Claude Code)").unwrap() >= MIN_VERSION);
        assert!(parse_version("2.1.200").unwrap() < MIN_VERSION);
        assert_eq!(parse_version("claude"), None);
    }

    #[test]
    fn the_mod_files_are_written_as_a_marketplace() {
        let dir = std::env::temp_dir().join(format!("aim-mod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_files(&dir).unwrap();
        let market: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude-plugin/marketplace.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(market["name"], MARKETPLACE);
        assert_eq!(market["plugins"][0]["name"], PLUGIN);
        assert!(dir.join("branch-notes/hooks/register.tsx").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
