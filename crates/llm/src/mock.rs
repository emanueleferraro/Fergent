//! A deterministic in-memory provider for tests and offline runs (spec D14).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::error::ProviderError;
use crate::provider::{
    response_from_text, Provider, ProviderFuture, ProviderHealth, ProviderStream,
};
use crate::types::{ChatRequest, ChatResponse, ModelInfo, Role, StreamFrame, Usage};

/// A scriptable provider: queued responses, queued failures, and an echo
/// fallback. Never performs I/O.
#[derive(Debug)]
pub struct MockProvider {
    id: String,
    models: Vec<ModelInfo>,
    queue: Mutex<VecDeque<ChatResponse>>,
    failures: Mutex<VecDeque<ProviderError>>,
    calls: AtomicUsize,
}

impl MockProvider {
    /// Create a provider with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            models: vec![ModelInfo {
                id: "mock-1".to_string(),
                context_window: Some(8192),
                supports_tools: true,
                supports_vision: true,
                supports_streaming: true,
                input_cost_per_mtok: None,
                output_cost_per_mtok: None,
            }],
            queue: Mutex::new(VecDeque::new()),
            failures: Mutex::new(VecDeque::new()),
            calls: AtomicUsize::new(0),
        }
    }

    /// Replace the advertised models.
    pub fn with_models(mut self, models: Vec<ModelInfo>) -> Self {
        self.models = models;
        self
    }

    /// Queue a response to be returned by the next call.
    pub fn push_response(&self, response: ChatResponse) {
        self.queue
            .lock()
            .expect("mock queue poisoned")
            .push_back(response);
    }

    /// Queue a failure to be returned by the next call.
    pub fn push_failure(&self, error: ProviderError) {
        self.failures
            .lock()
            .expect("mock failures poisoned")
            .push_back(error);
    }

    /// How many times `chat`/`chat_stream` has been invoked.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Provider for MockProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self
            .failures
            .lock()
            .expect("mock failures poisoned")
            .pop_front()
        {
            return Box::pin(async move { Err(error) });
        }
        if let Some(response) = self.queue.lock().expect("mock queue poisoned").pop_front() {
            return Box::pin(async move { Ok(response) });
        }
        Box::pin(async move {
            let prompt = request
                .messages
                .iter()
                .rev()
                .find(|message| message.role == Role::User)
                .map(|message| message.text_content())
                .unwrap_or_default();
            let text = format!("echo: {prompt}");
            let usage = Usage {
                input_tokens: prompt.chars().count().div_ceil(4) as u32,
                output_tokens: text.chars().count().div_ceil(4) as u32,
                cached_tokens: 0,
            };
            Ok(response_from_text(request.model.clone(), text, usage))
        })
    }

    fn chat_stream<'a>(&'a self, request: ChatRequest) -> ProviderFuture<'a, ProviderStream<'a>> {
        Box::pin(async move {
            let response = self.chat(request).await?;
            let mut frames: Vec<Result<StreamFrame, ProviderError>> =
                vec![Ok(StreamFrame::Start {
                    model: response.model.clone(),
                })];
            for word in response.text().split_inclusive(' ') {
                frames.push(Ok(StreamFrame::TextDelta {
                    text: word.to_string(),
                }));
            }
            for call in response.tool_calls() {
                frames.push(Ok(StreamFrame::ToolCall { call }));
            }
            frames.push(Ok(StreamFrame::Usage {
                usage: response.usage,
            }));
            frames.push(Ok(StreamFrame::Stop {
                finish_reason: response.finish_reason,
            }));
            let stream: ProviderStream<'a> = Box::pin(futures::stream::iter(frames));
            Ok(stream)
        })
    }

    fn models(&self) -> ProviderFuture<'_, Vec<ModelInfo>> {
        let models = self.models.clone();
        Box::pin(async move { Ok(models) })
    }

    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async { Ok(ProviderHealth::healthy()) })
    }
}
