//! Errors raised by providers and the router.

/// Errors raised by a [`Provider`](crate::Provider).
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// A transport-level failure.
    #[error("http error: {0}")]
    Http(String),
    /// The provider returned a non-success status with a message.
    #[error("api error ({status}): {message}")]
    Api {
        /// The HTTP status code.
        status: u16,
        /// The provider's message.
        message: String,
    },
    /// The provider is rate limiting.
    #[error("rate limited")]
    RateLimited,
    /// The request timed out.
    #[error("request timed out")]
    Timeout,
    /// A serialization/deserialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The provider does not support the requested capability.
    #[error("unsupported capability: {0}")]
    Unsupported(String),
    /// Any other provider error.
    #[error("provider error: {0}")]
    Other(String),
}

/// Errors raised by the [`Router`](crate::Router).
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// No route is configured for the task.
    #[error("no route configured for task: {0}")]
    NoRoute(String),
    /// No provider is registered for the model.
    #[error("no provider registered for model: {0}")]
    NoProvider(String),
    /// A budget would be exceeded.
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    /// A provider failed.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// Every candidate model failed.
    #[error("all candidate models failed: {0}")]
    AllFailed(String),
}
