//! Factory tools exposed to agents (`spec.md` §6).
//!
//! The Plugin Factory lives in the host application; these tools are thin
//! adapters over a [`FactoryOps`] so the agent crate stays independent of
//! kernel internals. Every call still passes through the permission gate:
//! `shell.exec` for builds, `fs.write` for the workspace, `fs.read` for
//! reports, and `plugin.install` for installs.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::AgentError;
use crate::tool::{Tool, ToolRegistry};

/// The template names accepted by `factory.scaffold`.
pub const FACTORY_TEMPLATES: [&str; 6] = [
    "kernel-tool",
    "ui-panel",
    "hybrid",
    "theme",
    "skill-prompt",
    "skill-code",
];

/// The host implementation behind the factory tools.
pub trait FactoryOps: Send + Sync {
    /// Run `command` with `arguments`.
    ///
    /// Errors must be returned as `<code>: <message>` so the model can react.
    fn call(&self, command: &str, arguments: Value) -> Result<Value, String>;
}

/// One factory command exposed to agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryCommand {
    /// Scaffold a workspace from a template.
    Scaffold,
    /// Build the workspace with the host toolchain.
    Build,
    /// Validate the workspace manifest, files, and schemas.
    Test,
    /// Package the workspace into an installable archive.
    Package,
    /// Resolve the install review for the workspace.
    Install,
    /// Summarize the workspace.
    Report,
}

impl FactoryCommand {
    /// Every command, in spec order.
    pub const ALL: [FactoryCommand; 6] = [
        FactoryCommand::Scaffold,
        FactoryCommand::Build,
        FactoryCommand::Test,
        FactoryCommand::Package,
        FactoryCommand::Install,
        FactoryCommand::Report,
    ];

    /// The tool name.
    pub fn name(self) -> &'static str {
        match self {
            FactoryCommand::Scaffold => "factory.scaffold",
            FactoryCommand::Build => "factory.build",
            FactoryCommand::Test => "factory.test",
            FactoryCommand::Package => "factory.package",
            FactoryCommand::Install => "factory.install",
            FactoryCommand::Report => "factory.report",
        }
    }

    fn description(self) -> &'static str {
        match self {
            FactoryCommand::Scaffold => {
                "Create a plugin or skill workspace from a factory template. Returns the run id."
            }
            FactoryCommand::Build => {
                "Build a factory workspace with the host toolchain; templates without a build step are skipped."
            }
            FactoryCommand::Test => {
                "Validate a factory workspace: manifest, files, schemas, and style warnings."
            }
            FactoryCommand::Package => {
                "Package a factory workspace into a `.fergplugin` or `.fergskill` archive."
            }
            FactoryCommand::Install => {
                "Return the install review for a factory workspace. Installing still requires explicit user approval and never happens automatically."
            }
            FactoryCommand::Report => {
                "Summarize a factory workspace: files, manifest, build log, and test report."
            }
        }
    }

    fn schema(self) -> Value {
        match self {
            FactoryCommand::Scaffold => json!({
                "type": "object",
                "properties": {
                    "template": { "type": "string", "enum": FACTORY_TEMPLATES },
                    "id": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["template", "id"],
                "additionalProperties": false
            }),
            _ => json!({
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"],
                "additionalProperties": false
            }),
        }
    }
}

/// A factory command as an agent tool.
pub struct FactoryTool {
    ops: Arc<dyn FactoryOps>,
    command: FactoryCommand,
}

impl FactoryTool {
    /// Wrap `command` over `ops`.
    pub fn new(ops: Arc<dyn FactoryOps>, command: FactoryCommand) -> Self {
        Self { ops, command }
    }
}

impl Tool for FactoryTool {
    fn name(&self) -> &str {
        self.command.name()
    }

    fn description(&self) -> &str {
        self.command.description()
    }

    fn schema(&self) -> Value {
        self.command.schema()
    }

    fn call(&self, arguments: Value) -> crate::tool::ToolFuture<'_> {
        let ops = self.ops.clone();
        let command = self.command.name();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || ops.call(command, arguments))
                .await
                .map_err(|err| AgentError::Tool(err.to_string()))?
                .map_err(AgentError::Tool)
        })
    }
}

/// Every factory tool, in spec order.
pub fn factory_tools(ops: Arc<dyn FactoryOps>) -> Vec<Arc<dyn Tool>> {
    FactoryCommand::ALL
        .into_iter()
        .map(|command| Arc::new(FactoryTool::new(ops.clone(), command)) as Arc<dyn Tool>)
        .collect()
}

/// Register every factory tool in `registry`.
pub fn register_factory_tools(registry: &ToolRegistry, ops: Arc<dyn FactoryOps>) {
    for tool in factory_tools(ops) {
        registry.register(tool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recording {
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl FactoryOps for Recording {
        fn call(&self, command: &str, arguments: Value) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap()
                .push((command.to_string(), arguments.clone()));
            if arguments.get("run_id").and_then(Value::as_str) == Some("missing") {
                return Err("factory_run_not_found: missing".to_string());
            }
            Ok(json!({ "command": command, "arguments": arguments }))
        }
    }

    #[test]
    fn exposes_every_command_with_a_closed_schema() {
        let ops: Arc<dyn FactoryOps> = Arc::new(Recording::default());
        let tools = factory_tools(ops);
        let names: Vec<String> = tools.iter().map(|tool| tool.name().to_string()).collect();
        assert_eq!(
            names,
            vec![
                "factory.scaffold",
                "factory.build",
                "factory.test",
                "factory.package",
                "factory.install",
                "factory.report",
            ]
        );
        for tool in &tools {
            assert_eq!(tool.schema()["additionalProperties"], json!(false));
        }
        let scaffold = tools[0].schema();
        assert_eq!(scaffold["required"], json!(["template", "id"]));
        assert_eq!(
            scaffold["properties"]["template"]["enum"],
            json!(FACTORY_TEMPLATES)
        );
        let install = tools[4].schema();
        assert_eq!(install["required"], json!(["run_id"]));
        assert!(install["properties"].get("permissions").is_none());
    }

    #[tokio::test]
    async fn delegates_to_the_host() {
        let ops = Arc::new(Recording::default());
        let tool = FactoryTool::new(ops.clone(), FactoryCommand::Install);
        let result = tool.call(json!({ "run_id": "01J8" })).await.unwrap();
        assert_eq!(result["command"], json!("factory.install"));
        assert_eq!(ops.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn maps_host_errors_to_tool_errors() {
        let ops = Arc::new(Recording::default());
        let tool = FactoryTool::new(ops, FactoryCommand::Report);
        let error = tool.call(json!({ "run_id": "missing" })).await.unwrap_err();
        assert!(error.to_string().contains("factory_run_not_found"));
        assert_eq!(error.code(), "agent_tool_error");
    }

    #[test]
    fn registers_into_a_registry() {
        let registry = ToolRegistry::new();
        register_factory_tools(&registry, Arc::new(Recording::default()));
        assert_eq!(
            registry.list(),
            vec![
                "factory.build",
                "factory.install",
                "factory.package",
                "factory.report",
                "factory.scaffold",
                "factory.test",
            ]
        );
    }
}
