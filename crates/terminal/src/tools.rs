//! Agent-facing terminal tools: bounded reads and visible command execution
//! (task 3.4, spec decision D7).

use std::sync::Arc;

use fergents_agents::{AgentError, Tool, ToolFuture};
use serde_json::{json, Value};

use crate::service::TerminalService;

/// `term.read`: a bounded, redacted tail of a session's output.
pub struct TermReadTool {
    terminal: Arc<TerminalService>,
}

impl TermReadTool {
    /// Create the tool over `terminal`.
    pub fn new(terminal: Arc<TerminalService>) -> Self {
        Self { terminal }
    }
}

impl Tool for TermReadTool {
    fn name(&self) -> &str {
        "term.read"
    }

    fn description(&self) -> &str {
        "Read a bounded, redacted tail of a terminal session's output."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "max_lines": { "type": ["integer", "null"] },
                "max_bytes": { "type": ["integer", "null"] }
            },
            "required": ["session_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let session_id = string_arg(&arguments, "session_id")?;
            let max_lines = u64_arg(&arguments, "max_lines").map(|value| value as usize);
            let max_bytes = u64_arg(&arguments, "max_bytes").map(|value| value as usize);
            let output = self
                .terminal
                .read(&session_id, max_lines, max_bytes)
                .map_err(|err| AgentError::Tool(err.to_string()))?;
            Ok(json!({ "session_id": session_id, "output": output }))
        })
    }
}

/// `term.exec`: run a command in a visible terminal tab.
pub struct TermExecTool {
    terminal: Arc<TerminalService>,
}

impl TermExecTool {
    /// Create the tool over `terminal`.
    pub fn new(terminal: Arc<TerminalService>) -> Self {
        Self { terminal }
    }
}

impl Tool for TermExecTool {
    fn name(&self) -> &str {
        "term.exec"
    }

    fn description(&self) -> &str {
        "Run a shell command in a visible terminal tab so the user can see it. \
         The command is audited; read its output with `term.read`."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string" },
                "command": { "type": "string" },
                "actor": { "type": ["string", "null"] }
            },
            "required": ["project_id", "command"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let project_id = string_arg(&arguments, "project_id")?;
            let command = string_arg(&arguments, "command")?;
            let actor = arguments
                .get("actor")
                .and_then(Value::as_str)
                .unwrap_or("agent")
                .to_string();
            let session = self
                .terminal
                .exec(&project_id, &command, &actor)
                .map_err(|err| AgentError::Tool(err.to_string()))?;
            serde_json::to_value(session).map_err(AgentError::from)
        })
    }
}

/// Every terminal tool.
pub fn terminal_tools(terminal: Arc<TerminalService>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(TermReadTool::new(terminal.clone())),
        Arc::new(TermExecTool::new(terminal)),
    ]
}

fn string_arg(arguments: &Value, key: &str) -> Result<String, AgentError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AgentError::Tool(format!("missing {key}")))
}

fn u64_arg(arguments: &Value, key: &str) -> Option<u64> {
    arguments.get(key).and_then(Value::as_u64)
}
