//! Shell discovery and the environment handed to a PTY.

use std::path::Path;

use portable_pty::CommandBuilder;

use crate::error::TerminalError;

/// Env var names containing these fragments are removed from the shell.
const SECRET_MARKERS: [&str; 5] = ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY"];

/// The shells the terminal may launch (basenames, lower-case, no `.exe`).
const ALLOWED_SHELLS: [&str; 8] = [
    "pwsh",
    "powershell",
    "cmd",
    "bash",
    "zsh",
    "fish",
    "sh",
    "nu",
];

/// The platform's default shell.
pub fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// The shells this build can launch.
pub fn allowed_shells() -> &'static [&'static str] {
    &ALLOWED_SHELLS
}

/// Resolve a requested shell, rejecting anything outside the allowlist.
pub fn resolve_shell(requested: Option<&str>) -> Result<String, TerminalError> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default_shell());
    };
    let name = Path::new(requested)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(requested)
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
    if ALLOWED_SHELLS.contains(&name.as_str()) {
        Ok(requested.to_string())
    } else {
        Err(TerminalError::ShellNotAllowed(requested.to_string()))
    }
}

/// Build the command for a shell in `cwd`, with secrets scrubbed from the
/// inherited environment.
pub fn command(shell: &str, cwd: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(shell);
    command.cwd(cwd);
    command.env_clear();
    for (key, value) in std::env::vars() {
        if !is_secret_key(&key) {
            command.env(key, value);
        }
    }
    if std::env::var_os("TERM").is_none() {
        command.env("TERM", "xterm-256color");
    }
    command.env("COLORTERM", "truecolor");
    command
}

fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlisted_shells_resolve() {
        assert_eq!(resolve_shell(Some("bash")).unwrap(), "bash");
        assert_eq!(resolve_shell(Some("/bin/zsh")).unwrap(), "/bin/zsh");
        #[cfg(windows)]
        assert_eq!(resolve_shell(Some("cmd.exe")).unwrap(), "cmd.exe");
    }

    #[test]
    fn other_shells_are_rejected() {
        assert!(matches!(
            resolve_shell(Some("/usr/bin/curl")),
            Err(TerminalError::ShellNotAllowed(_))
        ));
    }

    #[test]
    fn a_default_shell_is_always_available() {
        assert!(!resolve_shell(None).unwrap().is_empty());
    }

    #[test]
    fn secret_keys_are_detected() {
        assert!(is_secret_key("GITHUB_TOKEN"));
        assert!(is_secret_key("db_password"));
        assert!(!is_secret_key("PATH"));
    }
}
