//! Runs the user's own `git` without a console window, never prompting.

use std::path::Path;
use std::process::{Command, Stdio};

pub type Result<T> = std::result::Result<T, String>;

pub fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        // Branch and path names as UTF-8, not octal-escaped.
        .args(["-c", "core.quotepath=false"])
        .env("GIT_TERMINAL_PROMPT", "0")
        // Reads never take the index lock the user's own git may need.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Runs git and returns trimmed stdout, or stderr as the error.
pub fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = run_raw(dir, args)?;
    Ok(String::from_utf8_lossy(&out).trim_end().to_string())
}

/// Whether `dir` or a parent has a `.git` entry. Decides between "not a
/// repository" and a real git failure without parsing (localized) messages.
pub fn inside_git(dir: &Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

/// Runs git and returns raw stdout (for `-z` output).
pub fn run_raw(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = command(dir).args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "git을 찾을 수 없어요. Git을 설치한 뒤 다시 시도해 주세요".to_string()
        } else {
            format!("git을 실행하지 못했어요: {e}")
        }
    })?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
