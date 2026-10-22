//! Fergents LLM layer.
//!
//! A single normalized [`Provider`] surface over multiple backends, plus the
//! task-based [`Router`] with fallback chains, budgets, cost accounting, and
//! caches (`LLM.md` §2–§18). CI never touches the network: the deterministic
//! [`MockProvider`] is used in tests (spec decision D14).

mod anthropic;
mod budget;
mod cache;
mod error;
mod events;
mod google;
mod mock;
mod openai;
mod provider;
mod router;
mod tokens;
mod types;

pub use anthropic::AnthropicProvider;
pub use budget::{Budget, BudgetTracker, CostTable, Spend};
pub use cache::ResponseCache;
pub use error::{ProviderError, RouterError};
pub use events::{LlmEvent, LlmEventSink, NoopSink, RecordingSink};
pub use google::GoogleProvider;
pub use mock::MockProvider;
pub use openai::OpenAiCompatProvider;
pub use provider::{Provider, ProviderFuture, ProviderHealth, ProviderStream};
pub use router::{ModelRoute, RouteRule, Router, RouterConfig};
pub use tokens::TokenCounter;
pub use types::{
    ChatRequest, ChatResponse, ContentBlock, FinishReason, Message, ModelInfo, Role, StreamFrame,
    ToolCall, ToolDef, Usage,
};
