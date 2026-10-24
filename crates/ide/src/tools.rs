//! Agent-facing file tools, scoped to a project and gated by the runtime's
//! permission checks (task 3.2, spec decision D7).

use std::sync::Arc;

use fergents_agents::{AgentError, Tool, ToolFuture};
use serde_json::{json, Value};

use crate::files::FileService;

/// `file.read`: read a file from a project.
pub struct FileReadTool {
    files: Arc<FileService>,
}

impl FileReadTool {
    /// Create the tool over `files`.
    pub fn new(files: Arc<FileService>) -> Self {
        Self { files }
    }
}

impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file.read"
    }

    fn description(&self) -> &str {
        "Read a text file from a project. Returns the content, its etag, the \
         detected language, and whether it was truncated or binary."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string" },
                "path": { "type": "string", "description": "Project-relative path." }
            },
            "required": ["project_id", "path"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let project_id = string_arg(&arguments, "project_id")?;
            let path = string_arg(&arguments, "path")?;
            let snapshot = self
                .files
                .read(&project_id, &path)
                .map_err(|err| AgentError::Tool(err.to_string()))?;
            serde_json::to_value(snapshot).map_err(AgentError::from)
        })
    }
}

/// `file.write`: write a file, refusing stale writes.
pub struct FileWriteTool {
    files: Arc<FileService>,
}

impl FileWriteTool {
    /// Create the tool over `files`.
    pub fn new(files: Arc<FileService>) -> Self {
        Self { files }
    }
}

impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file.write"
    }

    fn description(&self) -> &str {
        "Write a text file in a trusted project. Pass `base_etag` (the etag \
         returned by file.read) to avoid overwriting concurrent edits; a stale \
         etag is refused with file_conflict."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string" },
                "path": { "type": "string", "description": "Project-relative path." },
                "content": { "type": "string" },
                "base_etag": { "type": "string" }
            },
            "required": ["project_id", "path", "content"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let project_id = string_arg(&arguments, "project_id")?;
            let path = string_arg(&arguments, "path")?;
            let content = string_arg(&arguments, "content")?;
            let base_etag = arguments.get("base_etag").and_then(Value::as_str);
            let snapshot = self
                .files
                .write(&project_id, &path, &content, base_etag)
                .map_err(|err| AgentError::Tool(err.to_string()))?;
            serde_json::to_value(snapshot).map_err(AgentError::from)
        })
    }
}

/// The file tools, ready to register on the runtime.
pub fn file_tools(files: Arc<FileService>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(FileReadTool::new(files.clone())),
        Arc::new(FileWriteTool::new(files)),
    ]
}

fn string_arg(arguments: &Value, key: &str) -> Result<String, AgentError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AgentError::Tool(format!("missing `{key}`")))
}
