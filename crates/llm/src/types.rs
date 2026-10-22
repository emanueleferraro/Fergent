//! Normalized request/response types shared by every provider.

use serde::{Deserialize, Serialize};

/// A conversation participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System instructions.
    System,
    /// The user.
    User,
    /// The assistant.
    Assistant,
    /// A tool result.
    Tool,
}

/// A single content block in a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text.
    Text {
        /// The text.
        text: String,
    },
    /// An inline image (base64-encoded, no data-URL prefix).
    Image {
        /// The media type, e.g. `image/png`.
        media_type: String,
        /// Base64-encoded image bytes.
        data_base64: String,
    },
    /// A tool call requested by the assistant.
    ToolCall {
        /// Provider-assigned call id.
        id: String,
        /// The tool name.
        name: String,
        /// The parsed arguments.
        arguments: serde_json::Value,
    },
    /// The result of a tool call.
    ToolResult {
        /// The id of the call this result answers.
        id: String,
        /// The result content.
        content: String,
    },
}

/// A message: a role plus one or more content blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The participant.
    pub role: Role,
    /// The content blocks.
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// A system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self::text(Role::System, text)
    }

    /// A user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    /// An assistant message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    /// A message with a single text block.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// The concatenation of this message's text blocks.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        for block in &self.content {
            if let ContentBlock::Text { text } = block {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        out
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    /// The tool name.
    pub name: String,
    /// A description for the model.
    pub description: String,
    /// A JSON Schema for the arguments.
    pub parameters: serde_json::Value,
}

/// A tool call emitted by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned call id.
    pub id: String,
    /// The tool name.
    pub name: String,
    /// The parsed arguments.
    pub arguments: serde_json::Value,
}

/// Token usage for a request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input (prompt) tokens.
    #[serde(default)]
    pub input_tokens: u32,
    /// Output (completion) tokens.
    #[serde(default)]
    pub output_tokens: u32,
    /// Tokens served from a provider cache.
    #[serde(default)]
    pub cached_tokens: u32,
}

impl Usage {
    /// Total tokens (input + output).
    pub fn total(&self) -> u32 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// A natural stop.
    Stop,
    /// The token limit was reached.
    Length,
    /// The model requested tool calls.
    ToolCalls,
    /// Content was filtered.
    ContentFilter,
    /// Anything else.
    Other,
}

/// A normalized chat request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The model id.
    pub model: String,
    /// The conversation.
    pub messages: Vec<Message>,
    /// Tools available to the model.
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Output token cap.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Whether to stream (handled by `chat_stream`).
    #[serde(default)]
    pub stream: bool,
}

impl ChatRequest {
    /// A minimal request for `model` with a single user message.
    pub fn user(model: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: vec![Message::user(text)],
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            stream: false,
        }
    }
}

/// A normalized chat response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The model that answered.
    pub model: String,
    /// The assistant message (text and/or tool-call blocks).
    pub message: Message,
    /// Why generation stopped.
    pub finish_reason: FinishReason,
    /// Token usage.
    pub usage: Usage,
}

impl ChatResponse {
    /// The text of the assistant message.
    pub fn text(&self) -> String {
        self.message.text_content()
    }

    /// The tool calls in the assistant message.
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

/// A normalized streaming frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamFrame {
    /// The stream started.
    Start {
        /// The model.
        model: String,
    },
    /// A text delta.
    TextDelta {
        /// The increment.
        text: String,
    },
    /// A completed tool call.
    ToolCall {
        /// The call.
        call: ToolCall,
    },
    /// Usage reported at the end of the stream.
    Usage {
        /// The usage.
        usage: Usage,
    },
    /// The stream ended.
    Stop {
        /// Why generation stopped.
        finish_reason: FinishReason,
    },
}

/// A model advertised by a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model id.
    pub id: String,
    /// The context window, if known.
    #[serde(default)]
    pub context_window: Option<u32>,
    /// Whether it supports tool calling.
    #[serde(default)]
    pub supports_tools: bool,
    /// Whether it supports images.
    #[serde(default)]
    pub supports_vision: bool,
    /// Whether it supports streaming.
    #[serde(default)]
    pub supports_streaming: bool,
    /// Cost per million input tokens, if known.
    #[serde(default)]
    pub input_cost_per_mtok: Option<f64>,
    /// Cost per million output tokens, if known.
    #[serde(default)]
    pub output_cost_per_mtok: Option<f64>,
}

impl ModelInfo {
    /// A minimal description of `id`.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_window: None,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            input_cost_per_mtok: None,
            output_cost_per_mtok: None,
        }
    }
}
