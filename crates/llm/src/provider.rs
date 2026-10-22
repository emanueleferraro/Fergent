//! The object-safe [`Provider`] trait and its streaming aliases.

use futures::future::BoxFuture;
use futures::stream::BoxStream;

use crate::error::ProviderError;
use crate::types::{ChatRequest, ChatResponse, FinishReason, ModelInfo, StreamFrame};

/// A boxed, sendable future returned by provider methods.
pub type ProviderFuture<'a, T> = BoxFuture<'a, Result<T, ProviderError>>;

/// A boxed, sendable stream of frames returned by `chat_stream`.
pub type ProviderStream<'a> = BoxStream<'a, Result<StreamFrame, ProviderError>>;

/// The health of a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHealth {
    /// Whether the provider is reachable/usable.
    pub ok: bool,
    /// Optional human-readable detail.
    pub detail: Option<String>,
}

impl ProviderHealth {
    /// A healthy provider.
    pub fn healthy() -> Self {
        Self {
            ok: true,
            detail: None,
        }
    }

    /// An unhealthy provider with a reason.
    pub fn unhealthy(detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            detail: Some(detail.into()),
        }
    }
}

/// A model backend behind a single normalized surface (`LLM.md` §2).
///
/// Only `id` and `chat` are required; the rest have sensible defaults so an
/// adapter can implement just what its backend supports.
pub trait Provider: Send + Sync {
    /// The provider id, e.g. `openai`, `ollama`.
    fn id(&self) -> &str;

    /// Run a non-streaming chat completion.
    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse>;

    /// Run a streaming chat completion.
    ///
    /// The default implementation calls [`Provider::chat`] and replays the
    /// result as frames; adapters override it for true incremental streaming.
    fn chat_stream<'a>(&'a self, request: ChatRequest) -> ProviderFuture<'a, ProviderStream<'a>> {
        Box::pin(async move {
            let response = self.chat(request).await?;
            Ok(default_stream(response))
        })
    }

    /// List the models the provider advertises.
    fn models(&self) -> ProviderFuture<'_, Vec<ModelInfo>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Check whether the provider is reachable.
    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async { Ok(ProviderHealth::healthy()) })
    }

    /// Embed texts into vectors.
    fn embed<'a>(
        &'a self,
        _texts: Vec<String>,
        _model: Option<&'a str>,
    ) -> ProviderFuture<'a, Vec<Vec<f32>>> {
        Box::pin(async { Err(ProviderError::Unsupported("embed".to_string())) })
    }

    /// Rerank documents against a query.
    fn rerank<'a>(
        &'a self,
        _query: String,
        _documents: Vec<String>,
        _model: Option<&'a str>,
    ) -> ProviderFuture<'a, Vec<f32>> {
        Box::pin(async { Err(ProviderError::Unsupported("rerank".to_string())) })
    }
}

/// Replay a completed response as a stream of frames.
pub(crate) fn default_stream(response: ChatResponse) -> ProviderStream<'static> {
    let mut frames = vec![StreamFrame::Start {
        model: response.model.clone(),
    }];
    let text = response.text();
    if !text.is_empty() {
        frames.push(StreamFrame::TextDelta { text });
    }
    for call in response.tool_calls() {
        frames.push(StreamFrame::ToolCall { call });
    }
    frames.push(StreamFrame::Usage {
        usage: response.usage,
    });
    frames.push(StreamFrame::Stop {
        finish_reason: response.finish_reason,
    });
    Box::pin(futures::stream::iter(frames.into_iter().map(Ok)))
}

/// Build a response from a text body (used by the mock and by adapters that
/// receive non-streaming JSON).
pub(crate) fn response_from_text(
    model: impl Into<String>,
    text: impl Into<String>,
    usage: crate::types::Usage,
) -> ChatResponse {
    ChatResponse {
        model: model.into(),
        message: crate::types::Message::assistant(text),
        finish_reason: FinishReason::Stop,
        usage,
    }
}
