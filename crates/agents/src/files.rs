//! Per-agent files: `agents/<id>/brain.md` and `agents/<id>/session.md`
//! (`AGENTS.md` §8, spec: Compaction produces session.md).

use std::path::{Path, PathBuf};

use crate::error::AgentError;

/// The directory holding one agent's files: `<agents_dir>/<id>`.
pub fn agent_dir(agents_dir: &Path, agent_id: &str) -> PathBuf {
    agents_dir.join(agent_id)
}

/// The path of an agent's self-improving prompt file.
pub fn brain_path(agents_dir: &Path, agent_id: &str) -> PathBuf {
    agent_dir(agents_dir, agent_id).join("brain.md")
}

/// The path of an agent's rolling session summary file.
pub fn session_path(agents_dir: &Path, agent_id: &str) -> PathBuf {
    agent_dir(agents_dir, agent_id).join("session.md")
}

/// Read `brain.md`, or `None` when it does not exist yet.
pub fn read_brain(agents_dir: &Path, agent_id: &str) -> Result<Option<String>, AgentError> {
    let path = brain_path(agents_dir, agent_id);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AgentError::Io(err)),
    }
}

/// Write `brain.md`, creating the agent directory if needed.
pub fn write_brain(agents_dir: &Path, agent_id: &str, content: &str) -> Result<(), AgentError> {
    write_file(&brain_path(agents_dir, agent_id), content)
}

/// Write `session.md`, creating the agent directory if needed.
pub fn write_session(agents_dir: &Path, agent_id: &str, content: &str) -> Result<(), AgentError> {
    write_file(&session_path(agents_dir, agent_id), content)
}

fn write_file(path: &Path, content: &str) -> Result<(), AgentError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}
