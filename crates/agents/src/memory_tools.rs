//! Memory tools exposed to agents (`AGENTS.md` §8.4).

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::AgentError;
use crate::memory::{MemoryKind, MemoryScope, MemoryStore};
use crate::tool::{Tool, ToolFuture};

fn scope_arg(args: &Value) -> MemoryScope {
    args.get("scope")
        .and_then(Value::as_str)
        .map(MemoryScope::parse)
        .unwrap_or(MemoryScope::Global)
}

fn kind_arg(args: &Value) -> MemoryKind {
    args.get("kind")
        .and_then(Value::as_str)
        .map_or(MemoryKind::Fact, MemoryKind::parse)
}

/// `memory.remember(content, kind?, scope?, importance?)`.
pub struct RememberTool {
    store: Arc<MemoryStore>,
}

impl RememberTool {
    /// Create the tool over `store`.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

impl Tool for RememberTool {
    fn name(&self) -> &str {
        "memory.remember"
    }

    fn description(&self) -> &str {
        "Store a memory entry (a fact or an episodic summary)."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "kind": { "type": "string", "enum": ["episodic", "fact"] },
                "scope": { "type": "string" },
                "importance": { "type": "number" }
            },
            "required": ["content"]
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let content = arguments
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::Tool("missing content".to_string()))?;
            let importance = arguments
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32;
            let entry = self.store.remember(
                &scope_arg(&arguments),
                kind_arg(&arguments),
                content,
                importance,
            )?;
            serde_json::to_value(entry).map_err(AgentError::from)
        })
    }
}

/// `memory.recall(query, k?, scope?)`.
pub struct RecallTool {
    store: Arc<MemoryStore>,
}

impl RecallTool {
    /// Create the tool over `store`.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

impl Tool for RecallTool {
    fn name(&self) -> &str {
        "memory.recall"
    }

    fn description(&self) -> &str {
        "Recall the most relevant memories for a query."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "k": { "type": "integer" },
                "scope": { "type": "string" }
            },
            "required": ["query"]
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::Tool("missing query".to_string()))?;
            let k = arguments.get("k").and_then(Value::as_u64).unwrap_or(8) as usize;
            let entries = self.store.recall(&scope_arg(&arguments), query, k)?;
            serde_json::to_value(entries).map_err(AgentError::from)
        })
    }
}

/// `memory.forget(id)`.
pub struct ForgetTool {
    store: Arc<MemoryStore>,
}

impl ForgetTool {
    /// Create the tool over `store`.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

impl Tool for ForgetTool {
    fn name(&self) -> &str {
        "memory.forget"
    }

    fn description(&self) -> &str {
        "Delete a memory entry by id."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let id = arguments
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::Tool("missing id".to_string()))?;
            let removed = self.store.forget(id)?;
            Ok(json!({ "removed": removed }))
        })
    }
}

/// `memory.list(scope?, kind?)`.
pub struct ListTool {
    store: Arc<MemoryStore>,
}

impl ListTool {
    /// Create the tool over `store`.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

impl Tool for ListTool {
    fn name(&self) -> &str {
        "memory.list"
    }

    fn description(&self) -> &str {
        "List memory entries in a scope."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scope": { "type": "string" },
                "kind": { "type": "string", "enum": ["episodic", "fact"] }
            }
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move {
            let kind = arguments
                .get("kind")
                .and_then(Value::as_str)
                .map(MemoryKind::parse);
            let entries = self.store.list(&scope_arg(&arguments), kind)?;
            serde_json::to_value(entries).map_err(AgentError::from)
        })
    }
}

/// The four memory tools, ready to register.
pub fn memory_tools(store: Arc<MemoryStore>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(RememberTool::new(store.clone())),
        Arc::new(RecallTool::new(store.clone())),
        Arc::new(ForgetTool::new(store.clone())),
        Arc::new(ListTool::new(store)),
    ]
}
