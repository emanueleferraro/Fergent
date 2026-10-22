//! Google Gemini adapter (chat only) — `LLM.md` §2.

use serde_json::{json, Value};

use crate::error::ProviderError;
use crate::provider::{Provider, ProviderFuture, ProviderHealth};
use crate::types::{
    ChatRequest, ChatResponse, ContentBlock, FinishReason, Message, ModelInfo, Role, Usage,
};

/// A provider for the Google Gemini `generateContent` API.
pub struct GoogleProvider {
    id: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl GoogleProvider {
    /// Create an adapter (default base is the Gemini v1beta endpoint).
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            id: "google".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key: Some(api_key.into()),
            client: reqwest::Client::new(),
        }
    }

    /// Override the base URL (for proxies or mocks).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    fn contents(messages: &[Message]) -> (Option<Value>, Vec<Value>) {
        let mut system = None;
        let mut out = Vec::new();
        for message in messages {
            match message.role {
                Role::System => system = Some(json!({ "parts": [{ "text": message.text_content() }] })),
                Role::Tool => out.push(json!({
                    "role": "user",
                    "parts": [{ "functionResponse": { "name": "tool", "response": { "content": message.text_content() } } }],
                })),
                Role::User | Role::Assistant => {
                    let role = if message.role == Role::Assistant {
                        "model"
                    } else {
                        "user"
                    };
                    let mut parts: Vec<Value> = Vec::new();
                    if !message.text_content().is_empty() {
                        parts.push(json!({ "text": message.text_content() }));
                    }
                    for block in &message.content {
                        if let ContentBlock::ToolCall {
                            name, arguments, ..
                        } = block
                        {
                            parts.push(json!({ "functionCall": { "name": name, "args": arguments } }));
                        }
                    }
                    out.push(json!({ "role": role, "parts": parts }));
                }
            }
        }
        (system, out)
    }

    async fn post(&self, url: &str, body: &Value) -> Result<Value, ProviderError> {
        let response = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(map_reqwest)?;
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

impl Provider for GoogleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse> {
        Box::pin(async move {
            let (system, contents) = Self::contents(&request.messages);
            let mut body = json!({ "contents": contents });
            if let Some(system) = system {
                body["systemInstruction"] = system;
            }
            if !request.tools.is_empty() {
                body["tools"] = json!([{
                    "functionDeclarations": request
                        .tools
                        .iter()
                        .map(|tool| json!({
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }))
                        .collect::<Vec<_>>(),
                }]);
            }
            if let Some(temperature) = request.temperature {
                body["generationConfig"] = json!({ "temperature": temperature });
            }
            let url = format!(
                "{}/models/{}:generateContent?key={}",
                self.base_url,
                request.model,
                self.api_key.as_deref().unwrap_or_default()
            );
            let value = self.post(&url, &body).await?;

            let candidate = value
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|candidates| candidates.first())
                .cloned()
                .unwrap_or(Value::Null);
            let mut content = Vec::new();
            if let Some(parts) = candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            content.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                    if let Some(call) = part.get("functionCall") {
                        content.push(ContentBlock::ToolCall {
                            id: call
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: call
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            arguments: call.get("args").cloned().unwrap_or(Value::Null),
                        });
                    }
                }
            }
            Ok(ChatResponse {
                model: request.model.clone(),
                message: Message {
                    role: Role::Assistant,
                    content,
                },
                finish_reason: map_finish_reason(
                    candidate.get("finishReason").and_then(Value::as_str),
                ),
                usage: value
                    .get("usageMetadata")
                    .map(parse_usage)
                    .unwrap_or_default(),
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
            .get("promptTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        output_tokens: value
            .get("candidatesTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        cached_tokens: value
            .get("cachedContentTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn map_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("STOP") => FinishReason::Stop,
        Some("MAX_TOKENS") => FinishReason::Length,
        Some("SAFETY") | Some("RECITATION") => FinishReason::ContentFilter,
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
