//! Anthropic Messages API adapter (chat only) — `LLM.md` §2.

use serde_json::{json, Value};

use crate::error::ProviderError;
use crate::provider::{Provider, ProviderFuture, ProviderHealth};
use crate::types::{
    ChatRequest, ChatResponse, ContentBlock, FinishReason, Message, ModelInfo, Role, Usage,
};

/// A provider for the Anthropic Messages API.
pub struct AnthropicProvider {
    id: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Create an adapter (default base is `https://api.anthropic.com`).
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            id: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key: Some(api_key.into()),
            client: reqwest::Client::new(),
        }
    }

    /// Override the base URL (for proxies or mocks).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    fn anthropic_messages(messages: &[Message]) -> (Option<String>, Vec<Value>) {
        let mut system = None;
        let mut out = Vec::new();
        for message in messages {
            match message.role {
                Role::System => system = Some(message.text_content()),
                Role::Tool => {
                    let parts: Vec<Value> = message
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::ToolResult { id, content } => Some(json!({
                                "type": "tool_result",
                                "tool_use_id": id,
                                "content": content,
                            })),
                            _ => None,
                        })
                        .collect();
                    out.push(json!({ "role": "user", "content": parts }));
                }
                Role::User | Role::Assistant => {
                    let role = if message.role == Role::Assistant {
                        "assistant"
                    } else {
                        "user"
                    };
                    let mut parts: Vec<Value> = Vec::new();
                    if !message.text_content().is_empty() {
                        parts.push(json!({ "type": "text", "text": message.text_content() }));
                    }
                    for block in &message.content {
                        if let ContentBlock::ToolCall {
                            id,
                            name,
                            arguments,
                        } = block
                        {
                            parts.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": arguments,
                            }));
                        }
                    }
                    out.push(json!({ "role": role, "content": parts }));
                }
            }
        }
        (system, out)
    }

    async fn post(&self, url: &str, body: &Value) -> Result<Value, ProviderError> {
        let mut request = self
            .client
            .post(url)
            .header("anthropic-version", "2023-06-01")
            .json(body);
        if let Some(key) = &self.api_key {
            request = request.header("x-api-key", key);
        }
        let response = request.send().await.map_err(map_reqwest)?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        let text = response.text().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: text,
            });
        }
        serde_json::from_str(&text).map_err(ProviderError::from)
    }
}

impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse> {
        Box::pin(async move {
            let (system, messages) = Self::anthropic_messages(&request.messages);
            let mut body = json!({
                "model": request.model,
                "max_tokens": request.max_tokens.unwrap_or(1024),
                "messages": messages,
            });
            if let Some(system) = system {
                body["system"] = json!(system);
            }
            if let Some(temperature) = request.temperature {
                body["temperature"] = json!(temperature);
            }
            if !request.tools.is_empty() {
                body["tools"] = Value::Array(
                    request
                        .tools
                        .iter()
                        .map(|tool| {
                            json!({
                                "name": tool.name,
                                "description": tool.description,
                                "input_schema": tool.parameters,
                            })
                        })
                        .collect(),
                );
            }
            let url = format!("{}/v1/messages", self.base_url);
            let value = self.post(&url, &body).await?;

            let mut content = Vec::new();
            if let Some(blocks) = value.get("content").and_then(Value::as_array) {
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                content.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                        Some("tool_use") => content.push(ContentBlock::ToolCall {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            arguments: block.get("input").cloned().unwrap_or(Value::Null),
                        }),
                        _ => {}
                    }
                }
            }
            Ok(ChatResponse {
                model: value
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or(&request.model)
                    .to_string(),
                message: Message {
                    role: Role::Assistant,
                    content,
                },
                finish_reason: map_stop_reason(value.get("stop_reason").and_then(Value::as_str)),
                usage: value.get("usage").map(parse_usage).unwrap_or_default(),
            })
        })
    }

    fn models(&self) -> ProviderFuture<'_, Vec<ModelInfo>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async { Ok(ProviderHealth::healthy()) })
    }
}

fn parse_usage(value: &Value) -> Usage {
    Usage {
        input_tokens: value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        output_tokens: value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        cached_tokens: value
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn map_stop_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("end_turn") | Some("stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolCalls,
        _ => FinishReason::Other,
    }
}

fn map_reqwest(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Http(err.to_string())
    }
}
