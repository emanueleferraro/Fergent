//! The `git` CLI, used for remote operations and hook-honouring commands.

use std::path::Path;
use std::process::Command;

use crate::error::GitError;

/// Variables that must never reach a child git process.
const SECRET_MARKERS: [&str; 5] = ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY"];

/// Run `git -C <cwd> <args…>` and return trimmed stdout.
pub fn run(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd);
    command.args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    for (key, _) in std::env::vars() {
        if SECRET_MARKERS.iter().any(|marker| key.contains(marker)) {
            command.env_remove(&key);
        }
    }
    let output = command
        .output()
        .map_err(|error| GitError::Cli(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(GitError::Cli(format!(
            "git {} failed: {detail}",
            args.first().copied().unwrap_or_default()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
