//! Locating and running a tool's own CLI (`claude`, `codex`) with the user's
//! unmodified binary. Shared by the commit-message draft and Claude mod code.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::ToolId;

type Result<T> = std::result::Result<T, String>;

/// Executable stem on the command line.
pub fn command_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Claude => "claude",
        ToolId::Codex => "codex",
    }
}

/// Product name for messages.
fn product(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Claude => "Claude Code",
        ToolId::Codex => "Codex",
    }
}

/// File names a CLI is installed under (npm adds `.cmd` shims on Windows).
fn file_names(tool: ToolId) -> Vec<String> {
    let stem = command_name(tool);
    if cfg!(windows) {
        vec![format!("{stem}.exe"), format!("{stem}.cmd")]
    } else {
        vec![stem.to_string()]
    }
}

/// PATH plus install folders that may be missing from a GUI app's PATH.
fn search_dirs(tool: ToolId, home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
        if tool == ToolId::Claude {
            dirs.push(home.join(".claude").join("local"));
        }
    }
    dirs
}

fn find_in(tool: ToolId, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names = file_names(tool);
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// The user's CLI for `tool`: on PATH, or where its installers put it.
pub fn find(tool: ToolId) -> Option<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    find_in(tool, &search_dirs(tool, home))
}

/// Error for a CLI that isn't installed.
pub fn not_found(tool: ToolId) -> String {
    let (name, cmd) = (product(tool), command_name(tool));
    crate::tr!(
        "{name}({cmd} 명령)를 찾지 못했어요. 설치한 뒤 다시 시도해 주세요",
        "Couldn't find {name} (the {cmd} command). Install it and try again"
    )
}

/// Runs `tool`'s CLI in `dir` with `args`, `input` on stdin, giving up after
/// `timeout`: (exited successfully, stdout, stderr), or why it couldn't run.
/// Rust quotes arguments for cmd.exe itself when the program is a `.cmd`.
pub fn run_with_input(
    tool: ToolId,
    dir: &Path,
    args: &[&str],
    input: Option<String>,
    timeout: Duration,
) -> Result<(bool, String, String)> {
    use std::io::{Read, Write};
    let path = find(tool).ok_or_else(|| not_found(tool))?;
    let cmd_name = command_name(tool);
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
        .env("NO_COLOR", "1");
    if tool == ToolId::Claude {
        cmd.env("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| {
        crate::tr!(
            "{cmd_name}를 실행하지 못했어요: {e}",
            "Couldn't run {cmd_name}: {e}"
        )
    })?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // Written on a thread and closed after, so the CLI sees the end.
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
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::tr!(
                    "{cmd_name}가 {}초 안에 끝나지 않았어요",
                    "{cmd_name} didn't finish within {} seconds",
                    timeout.as_secs()
                ));
            }
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok((status.success(), stdout, stderr))
}

/// Which tool's CLI drafts text for a project of `preferred`: its own if
/// installed, else the other installed one, else `None`.
pub fn choose(preferred: ToolId, installed: &[ToolId]) -> Option<ToolId> {
    if installed.contains(&preferred) {
        return Some(preferred);
    }
    ToolId::ALL.into_iter().find(|t| installed.contains(t))
}

/// Tools whose CLI is installed.
pub fn installed() -> Vec<ToolId> {
    ToolId::ALL
        .into_iter()
        .filter(|t| find(*t).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_prefers_own_then_other() {
        use ToolId::*;
        assert_eq!(choose(Codex, &[Claude, Codex]), Some(Codex));
        assert_eq!(choose(Codex, &[Claude]), Some(Claude));
        assert_eq!(choose(Claude, &[Codex]), Some(Codex));
        assert_eq!(choose(Claude, &[]), None);
    }

    #[test]
    fn finds_in_given_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "codex.cmd" } else { "codex" };
        std::fs::write(dir.path().join(name), "").unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        assert!(find_in(ToolId::Codex, &dirs).is_some());
        assert!(find_in(ToolId::Claude, &dirs).is_none());
    }
}
