//! The tool surface: a `Tool` trait and an in-memory [`ToolRegistry`].

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use fergents_llm::ToolDef;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::AgentError;

/// A boxed, sendable future returned by [`Tool::call`].
pub type ToolFuture<'a> = BoxFuture<'a, Result<Value, AgentError>>;

/// A callable function exposed to an agent (`AGENTS.md` §7).
pub trait Tool: Send + Sync {
    /// The tool name, e.g. `brain.search`.
    fn name(&self) -> &str;

    /// A description for the model.
    fn description(&self) -> &str;

    /// A JSON Schema for the arguments.
    fn schema(&self) -> Value {
        serde_json::json!({ "type": "object" })
    }

    /// Execute the tool.
    fn call(&self, arguments: Value) -> ToolFuture<'_>;
}

/// The tools available to the runtime, keyed by name.
#[derive(Default)]
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool, replacing any tool with the same name.
    pub fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools
            .write()
            .expect("tool registry poisoned")
            .insert(name, tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .read()
            .expect("tool registry poisoned")
            .get(name)
            .cloned()
    }

    /// Whether a tool is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools
            .read()
            .expect("tool registry poisoned")
            .contains_key(name)
    }

    /// The registered tool names, sorted.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .tools
            .read()
            .expect("tool registry poisoned")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// The registered tools as model-facing definitions, sorted by name.
    pub fn defs(&self) -> Vec<ToolDef> {
        let tools = self.tools.read().expect("tool registry poisoned");
        let mut defs: Vec<ToolDef> = tools
            .values()
            .map(|tool| ToolDef {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.schema(),
            })
            .collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Tool definitions visible to an agent that has `skills` attached.
    ///
    /// Skill tools (`skill.<id>`) are hidden unless the concept is assigned,
    /// and the generic `skill.invoke` tool is always available.
    pub fn defs_for(&self, skills: &[String]) -> Vec<ToolDef> {
        self.defs()
            .into_iter()
            .filter(|definition| {
                if definition.name == "skill.invoke" {
                    return true;
                }
                match definition.name.strip_prefix("skill.") {
                    Some(id) => skills.iter().any(|skill| skill == id),
                    None => true,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl Tool for Echo {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo the arguments."
        }

        fn call(&self, arguments: Value) -> ToolFuture<'_> {
            Box::pin(async move { Ok(arguments) })
        }
    }

    #[test]
    fn registers_and_lists() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(Echo));
        assert!(registry.contains("echo"));
        assert_eq!(registry.list(), vec!["echo".to_string()]);
        let defs = registry.defs();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
    }
}
