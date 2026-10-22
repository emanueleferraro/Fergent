//! OpenAI-compatible adapter (covers OpenAI, Ollama, llama.cpp, and custom
//! OpenAI-compatible endpoints) — `LLM.md` §2, §5.

use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;

use bytes::Bytes;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::error::ProviderError;
use crate::provider::{Provider, ProviderFuture, ProviderHealth, ProviderStream};
use crate::types::{
    ChatRequest, ChatResponse, ContentBlock, FinishReason, Message, ModelInfo, Role, StreamFrame,
    ToolCall, Usage,
};

type ByteStream = Pin<Box<dyn futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

/// A provider for any OpenAI-compatible `/chat/completions` API.
pub struct OpenAiCompatProvider {
    id: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    /// Create an adapter for `base_url` (which should include the `/v1` suffix).
    pub fn new(id: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
            client: reqwest::Client::new(),
        }
    }

    /// A local Ollama endpoint.
    pub fn ollama() -> Self {
        Self::new("ollama", "http://localhost:11434/v1")
    }

    /// A local llama.cpp server endpoint.
    pub fn llama_cpp() -> Self {
        Self::new("llama.cpp", "http://localhost:8080/v1")
    }

    /// Set the bearer API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Use a preconfigured client (timeouts, proxies).
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    fn build_body(&self, request: &ChatRequest, stream: bool) -> Value {
        let mut body = json!({
            "model": request.model,
            "messages": openai_messages(&request.messages),
        });
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools.iter().map(openai_tool).collect());
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if stream {
            body["stream"] = json!(true);
        }
        body
    }

    async fn post_stream(
        &self,
        url: &str,
        body: &Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let mut request = self.client.post(url).json(body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(map_reqwest)?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message,
            });
        }
        Ok(response)
    }

    async fn post_json(&self, url: &str, body: &Value) -> Result<Value, ProviderError> {
        let response = self.post_stream(url, body).await?;
        let text = response.text().await.map_err(map_reqwest)?;
        serde_json::from_str(&text).map_err(ProviderError::from)
    }
}

impl Provider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url);
            let body = self.build_body(&request, false);
            let value = self.post_json(&url, &body).await?;
            parse_chat(&request.model, &value)
        })
    }

    fn chat_stream<'a>(&'a self, request: ChatRequest) -> ProviderFuture<'a, ProviderStream<'a>> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url);
            let body = self.build_body(&request, true);
            let response = self.post_stream(&url, &body).await?;
            let state = SseState {
                stream: Box::pin(response.bytes_stream()),
                buffer: String::new(),
                model: request.model.clone(),
                pending: VecDeque::new(),
                tool_calls: BTreeMap::new(),
                usage: Usage::default(),
                finish: FinishReason::Stop,
                started: false,
                finalized: false,
            };
            let stream = futures::stream::unfold(state, |mut state| async move {
                loop {
                    if let Some(frame) = state.pending.pop_front() {
                        return Some((frame, state));
                    }
                    if state.finalized {
                        return None;
                    }
                    if !state.started {
                        state.started = true;
                        state.pending.push_back(Ok(StreamFrame::Start {
                            model: state.model.clone(),
                        }));
                        continue;
                    }
                    match state.stream.next().await {
                        Some(Ok(bytes)) => {
                            state.buffer.push_str(&String::from_utf8_lossy(&bytes));
                            while let Some(index) = state.buffer.find('\n') {
                                let line = state.buffer[..index].trim().to_string();
                                state.buffer.drain(..=index);
                                let Some(data) = line.strip_prefix("data:") else {
                                    continue;
                                };
                                let data = data.trim();
                                if data == "[DONE]" {
                                    finalize(&mut state);
                                    continue;
                                }
                                if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                                    apply_chunk(&mut state, &chunk);
                                }
                            }
                            continue;
                        }
                        Some(Err(err)) => {
                            state.finalized = true;
                            return Some((Err(map_reqwest(err)), state));
                        }
                        None => {
                            finalize(&mut state);
                            continue;
                        }
                    }
                }
            });
            let stream: ProviderStream<'a> = Box::pin(stream);
            Ok(stream)
        })
    }

    fn models(&self) -> ProviderFuture<'_, Vec<ModelInfo>> {
        Box::pin(async move {
            let url = format!("{}/models", self.base_url);
            let mut request = self.client.get(&url);
            if let Some(key) = &self.api_key {
                request = request.bearer_auth(key);
            }
            let response = request.send().await.map_err(map_reqwest)?;
            let text = response.text().await.map_err(map_reqwest)?;
            let value: Value = serde_json::from_str(&text)?;
            Ok(parse_models(&value))
        })
    }

    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async move {
            match self.models().await {
                Ok(_) => Ok(ProviderHealth::healthy()),
                Err(err) => Ok(ProviderHealth::unhealthy(err.to_string())),
            }
        })
    }

    fn embed<'a>(
        &'a self,
        texts: Vec<String>,
        model: Option<&'a str>,
    ) -> ProviderFuture<'a, Vec<Vec<f32>>> {
        Box::pin(async move {
            let url = format!("{}/embeddings", self.base_url);
            let body = json!({
                "model": model.unwrap_or("text-embedding-3-small"),
                "input": texts,
            });
            let value = self.post_json(&url, &body).await?;
            let mut out = Vec::new();
            if let Some(data) = value.get("data").and_then(Value::as_array) {
                for item in data {
                    let vector = item
                        .get("embedding")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_f64)
                                .map(|v| v as f32)
                                .collect()
                        })
                        .unwrap_or_default();
                    out.push(vector);
                }
            }
            Ok(out)
        })
    }
}

struct SseState {
    stream: ByteStream,
    buffer: String,
    model: String,
    pending: VecDeque<Result<StreamFrame, ProviderError>>,
    tool_calls: BTreeMap<usize, (String, String, String)>,
    usage: Usage,
    finish: FinishReason,
    started: bool,
    finalized: bool,
}

fn apply_chunk(state: &mut SseState, chunk: &Value) {
    if let Some(model) = chunk.get("model").and_then(Value::as_str) {
        state.model = model.to_string();
    }
    if let Some(usage) = chunk.get("usage") {
        state.usage = parse_usage(usage);
    }
    let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
        return;
    };
    let Some(choice) = choices.first() else {
        return;
    };
    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
        state.finish = map_finish_reason(reason);
    }
    let Some(delta) = choice.get("delta") else {
        return;
    };
    if let Some(text) = delta.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            state.pending.push_back(Ok(StreamFrame::TextDelta {
                text: text.to_string(),
            }));
        }
    }
    if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let entry = state.tool_calls.entry(index).or_default();
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                entry.0 = id.to_string();
            }
            if let Some(function) = call.get("function") {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    entry.1 = name.to_string();
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    entry.2.push_str(arguments);
                }
            }
        }
    }
}

fn finalize(state: &mut SseState) {
    if state.finalized {
        return;
    }
    state.finalized = true;
    for (index, (id, name, arguments)) in std::mem::take(&mut state.tool_calls) {
        state.pending.push_back(Ok(StreamFrame::ToolCall {
            call: ToolCall {
                id: if id.is_empty() {
                    format!("call_{index}")
                } else {
                    id
                },
                name,
                arguments: serde_json::from_str(&arguments).unwrap_or(Value::Null),
            },
        }));
    }
    state
        .pending
        .push_back(Ok(StreamFrame::Usage { usage: state.usage }));
    state.pending.push_back(Ok(StreamFrame::Stop {
        finish_reason: state.finish,
    }));
}

fn openai_messages(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        let text = message.text_content();
        let images: Vec<&ContentBlock> = message
            .content
            .iter()
            .filter(|block| matches!(block, ContentBlock::Image { .. }))
            .collect();
        if images.is_empty() {
            let mut value = json!({ "role": role, "content": text });
            let tool_calls: Vec<Value> = message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    } => Some(json!({
                        "id": id,
                        "type": "function",
                        "function": { "name": name, "arguments": arguments.to_string() },
                    })),
                    _ => None,
                })
                .collect();
            if !tool_calls.is_empty() {
                value["tool_calls"] = Value::Array(tool_calls);
            }
            if message.role == Role::Tool {
                if let Some(ContentBlock::ToolResult { id, .. }) = message.content.first() {
                    value["tool_call_id"] = json!(id);
                }
            }
            out.push(value);
        } else {
            let mut parts = Vec::new();
            if !text.is_empty() {
                parts.push(json!({ "type": "text", "text": text }));
            }
            for block in images {
                if let ContentBlock::Image {
                    media_type,
                    data_base64,
                } = block
                {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:{media_type};base64,{data_base64}") },
                    }));
                }
            }
            out.push(json!({ "role": role, "content": parts }));
        }
    }
    out
}

fn openai_tool(tool: &crate::types::ToolDef) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        },
    })
}

fn parse_chat(model: &str, value: &Value) -> Result<ChatResponse, ProviderError> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| ProviderError::Other("response has no choices".to_string()))?;
    let message = choice.get("message").cloned().unwrap_or(Value::Null);
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let function = call.get("function").cloned().unwrap_or(Value::Null);
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or(Value::Null);
            content.push(ContentBlock::ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: function
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments,
            });
        }
    }
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(map_finish_reason)
        .unwrap_or(FinishReason::Stop);
    Ok(ChatResponse {
        model: value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(model)
            .to_string(),
        message: Message {
            role: Role::Assistant,
            content,
        },
        finish_reason,
        usage: value.get("usage").map(parse_usage).unwrap_or_default(),
    })
}

fn parse_models(value: &Value) -> Vec<ModelInfo> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(ModelInfo::new)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_usage(value: &Value) -> Usage {
    Usage {
        input_tokens: value
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        output_tokens: value
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        cached_tokens: value
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "tool_calls" | "function_call" => FinishReason::ToolCalls,
        "content_filter" => FinishReason::ContentFilter,
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
