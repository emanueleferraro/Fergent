//! Kernel adapters for plugin host calls and skill execution.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_agents::{AgentError, Tool, ToolFuture, ToolRegistry};
use fergents_events::EventBus;
use fergents_llm::ChatRequest;
use fergents_plugins::{HostServices, PluginService, ToolSpec};
use fergents_skills::{SkillService, SkillServices, SkillTool};
use serde_json::{json, Value};
use tokio::runtime::Handle;

use crate::providers::ProviderService;

/// Host services reached by plugin components (fs/storage/events are handled
/// by `fergents-plugins` itself).
pub struct KernelHostServices {
    data_dir: PathBuf,
    providers: Arc<ProviderService>,
    secrets: Arc<fergents_secrets::SecretVault>,
    bus: Arc<EventBus>,
    runtime: Option<Handle>,
    client: reqwest::Client,
}

impl KernelHostServices {
    /// Build the adapter.
    pub fn new(
        data_dir: PathBuf,
        providers: Arc<ProviderService>,
        secrets: Arc<fergents_secrets::SecretVault>,
        bus: Arc<EventBus>,
        runtime: Option<Handle>,
    ) -> Self {
        Self {
            data_dir,
            providers,
            secrets,
            bus,
            runtime,
            client: reqwest::Client::new(),
        }
    }

    fn runtime(&self) -> Result<&Handle, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| "no tokio runtime for host call".to_string())
    }

    fn complete(&self, model: &str, prompt: &str, input: &Value) -> Result<String, String> {
        let model = if model.is_empty() {
            self.providers
                .default_model()
                .ok_or_else(|| "no model configured".to_string())?
        } else {
            model.to_string()
        };
        let request = ChatRequest::user(
            model.clone(),
            format!(
                "{prompt}\n\nINPUT:\n{}",
                serde_json::to_string_pretty(input).unwrap_or_default()
            ),
        );
        self.runtime()?
            .block_on(self.providers.complete(&model, request))
            .map(|response| response.text())
            .map_err(|err| err.to_string())
    }
}

impl HostServices for KernelHostServices {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &Value,
        body: Option<&str>,
    ) -> Result<String, String> {
        let mut request = match method.to_ascii_uppercase().as_str() {
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "PATCH" => self.client.patch(url),
            "DELETE" => self.client.delete(url),
            "HEAD" => self.client.head(url),
            _ => self.client.get(url),
        };
        if let Some(headers) = headers.as_object() {
            for (key, value) in headers {
                if let Some(value) = value.as_str() {
                    request = request.header(key, value);
                }
            }
        }
        if let Some(body) = body {
            request = request.body(body.to_string());
        }
        let response = self
            .runtime()?
            .block_on(request.send())
            .map_err(|err| err.to_string())?;
        let status = response.status();
        let text = self
            .runtime()?
            .block_on(response.text())
            .map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "http {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        Ok(text)
    }

    fn llm_chat(&self, model: &str, messages: &Value) -> Result<String, String> {
        self.complete(model, "Messages:", messages)
    }

    fn secret(&self, name: &str) -> Result<Option<String>, String> {
        match self.secrets.resolve(name) {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None),
        }
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.bus
            .publish(
                "ui.notify".to_string(),
                "plugin".to_string(),
                json!({ "title": title, "body": body }),
            )
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

/// Skill execution services.
pub struct KernelSkillServices {
    data_dir: PathBuf,
    providers: Arc<ProviderService>,
    tools: Arc<ToolRegistry>,
    runtime: Option<Handle>,
}

impl KernelSkillServices {
    /// Build the adapter.
    pub fn new(
        data_dir: PathBuf,
        providers: Arc<ProviderService>,
        tools: Arc<ToolRegistry>,
        runtime: Option<Handle>,
    ) -> Self {
        Self {
            data_dir,
            providers,
            tools,
            runtime,
        }
    }

    fn runtime(&self) -> Result<&Handle, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| "no tokio runtime for skill call".to_string())
    }
}

impl SkillServices for KernelSkillServices {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn llm_chat(
        &self,
        model: &str,
        prompt: &str,
        input: &Value,
        temperature: f64,
        max_tokens: u32,
    ) -> Result<String, String> {
        let model = if model.is_empty() {
            self.providers
                .default_model()
                .ok_or_else(|| "no model configured".to_string())?
        } else {
            model.to_string()
        };
        let mut request = ChatRequest::user(
            model.clone(),
            format!(
                "{prompt}\n\nINPUT:\n{}",
                serde_json::to_string_pretty(input).unwrap_or_default()
            ),
        );
        request.temperature = Some(temperature as f32);
        request.max_tokens = Some(max_tokens);
        self.runtime()?
            .block_on(self.providers.complete(&model, request))
            .map(|response| response.text())
            .map_err(|err| err.to_string())
    }

    fn tool_call(&self, tool: &str, input: Value) -> Result<Value, String> {
        let name = if let Some(rest) = tool.strip_prefix("mcp:") {
            rest.replace(':', ".")
        } else if let Some(rest) = tool.strip_prefix("kernel:") {
            rest.to_string()
        } else {
            tool.to_string()
        };
        let tool = self
            .tools
            .get(&name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        self.runtime()?
            .block_on(tool.call(input))
            .map_err(|err| err.to_string())
    }
}

/// A kernel tool backed by a plugin.
pub struct PluginTool {
    service: Arc<PluginService>,
    plugin_id: String,
    spec: ToolSpec,
}

impl PluginTool {
    /// Wrap one plugin tool.
    pub fn new(service: Arc<PluginService>, plugin_id: String, spec: ToolSpec) -> Self {
        Self {
            service,
            plugin_id,
            spec,
        }
    }
}

impl Tool for PluginTool {
    fn name(&self) -> &str {
        &self.spec.name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn schema(&self) -> Value {
        self.spec.parameters.clone()
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        let service = self.service.clone();
        let plugin_id = self.plugin_id.clone();
        let tool = self.spec.name.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || service.call_tool(&plugin_id, &tool, arguments))
                .await
                .map_err(|err| AgentError::Tool(err.to_string()))?
                .map_err(|err| AgentError::Tool(err.to_string()))
        })
    }
}

/// The generic `skill.invoke` tool.
pub struct SkillInvokeTool {
    service: Arc<SkillService>,
}

impl SkillInvokeTool {
    /// Wrap the skill service.
    pub fn new(service: Arc<SkillService>) -> Self {
        Self { service }
    }
}

impl Tool for SkillInvokeTool {
    fn name(&self) -> &str {
        "skill.invoke"
    }

    fn description(&self) -> &str {
        "Invoke an installed skill by id with a JSON input matching its schema."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string" },
                "input": { "type": "object" }
            },
            "required": ["skill_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        let service = self.service.clone();
        Box::pin(async move {
            let skill_id = arguments
                .get("skill_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::Tool("missing skill_id".to_string()))?
                .to_string();
            let input = arguments.get("input").cloned().unwrap_or_else(|| json!({}));
            tokio::task::spawn_blocking(move || service.invoke(&skill_id, input))
                .await
                .map_err(|err| AgentError::Tool(err.to_string()))?
                .map_err(|err| AgentError::Tool(err.to_string()))
        })
    }
}

/// A kernel tool for one attached skill.
pub struct AttachedSkillTool {
    service: Arc<SkillService>,
    spec: SkillTool,
}

impl AttachedSkillTool {
    /// Wrap one skill.
    pub fn new(service: Arc<SkillService>, spec: SkillTool) -> Self {
        Self { service, spec }
    }
}

impl Tool for AttachedSkillTool {
    fn name(&self) -> &str {
        &self.spec.name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn schema(&self) -> Value {
        self.spec.parameters.clone()
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        let service = self.service.clone();
        let skill_id = self.spec.skill_id.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || service.invoke(&skill_id, arguments))
                .await
                .map_err(|err| AgentError::Tool(err.to_string()))?
                .map_err(|err| AgentError::Tool(err.to_string()))
        })
    }
}
