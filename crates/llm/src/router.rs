//! Task-based routing with fallback chains, budgets, cost, and caching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::budget::{Budget, BudgetTracker, CostTable};
use crate::cache::ResponseCache;
use crate::error::{ProviderError, RouterError};
use crate::events::{LlmEvent, LlmEventSink, NoopSink};
use crate::provider::{Provider, ProviderStream};
use crate::tokens::TokenCounter;
use crate::types::{ChatRequest, ChatResponse, Usage};

/// The mapping from a model id to the provider that serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRoute {
    /// The model id.
    pub model: String,
    /// The provider id.
    pub provider: String,
}

/// An ordered candidate list for a task (first = primary, rest = fallbacks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRule {
    /// The task name, e.g. `chat`, `summarize`, `code.edit`.
    pub task: String,
    /// The candidate models, in preference order.
    pub models: Vec<String>,
}

/// Router configuration: model→provider, task→models, and a default chain.
#[derive(Debug, Clone, Default)]
pub struct RouterConfig {
    /// Model to provider mappings.
    pub models: Vec<ModelRoute>,
    /// Per-task routes.
    pub routes: Vec<RouteRule>,
    /// The fallback chain when a task has no explicit route.
    pub default_models: Vec<String>,
}

impl RouterConfig {
    /// An empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a model→provider mapping.
    pub fn with_model(mut self, model: impl Into<String>, provider: impl Into<String>) -> Self {
        self.models.push(ModelRoute {
            model: model.into(),
            provider: provider.into(),
        });
        self
    }

    /// Add a task route.
    pub fn with_route(mut self, task: impl Into<String>, models: Vec<String>) -> Self {
        self.routes.push(RouteRule {
            task: task.into(),
            models,
        });
        self
    }

    /// Set the default chain.
    pub fn with_default(mut self, models: Vec<String>) -> Self {
        self.default_models = models;
        self
    }

    /// The provider id for `model`, if mapped.
    pub fn provider_of(&self, model: &str) -> Option<&str> {
        self.models
            .iter()
            .find(|route| route.model == model)
            .map(|route| route.provider.as_str())
    }
}

/// Routes requests to providers, applying fallbacks, budgets, and caches.
pub struct Router {
    config: RouterConfig,
    providers: HashMap<String, Arc<dyn Provider>>,
    budget: BudgetTracker,
    costs: CostTable,
    cache: Option<ResponseCache>,
    sink: Arc<dyn LlmEventSink>,
    tokens: TokenCounter,
}

impl Router {
    /// Create a router from `config`.
    pub fn new(config: RouterConfig) -> Self {
        Self {
            config,
            providers: HashMap::new(),
            budget: BudgetTracker::new(Budget::default()),
            costs: CostTable::new(),
            cache: None,
            sink: Arc::new(NoopSink),
            tokens: TokenCounter::new(),
        }
    }

    /// Register a provider under `id`.
    pub fn with_provider(mut self, id: impl Into<String>, provider: Arc<dyn Provider>) -> Self {
        self.providers.insert(id.into(), provider);
        self
    }

    /// Set the budget.
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = BudgetTracker::new(budget);
        self
    }

    /// Set the cost table.
    pub fn with_cost_table(mut self, costs: CostTable) -> Self {
        self.costs = costs;
        self
    }

    /// Enable the response cache with the given capacity.
    pub fn with_cache(mut self, capacity: usize) -> Self {
        self.cache = Some(ResponseCache::new(capacity));
        self
    }

    /// Set the event sink.
    pub fn with_sink(mut self, sink: Arc<dyn LlmEventSink>) -> Self {
        self.sink = sink;
        self
    }

    /// The budget tracker.
    pub fn budget(&self) -> &BudgetTracker {
        &self.budget
    }

    /// The configured model→provider mappings.
    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    /// The ordered candidate models for `task`.
    pub fn candidates(&self, task: &str) -> Vec<String> {
        if let Some(rule) = self.config.routes.iter().find(|rule| rule.task == task) {
            if !rule.models.is_empty() {
                return rule.models.clone();
            }
        }
        if !self.config.default_models.is_empty() {
            return self.config.default_models.clone();
        }
        self.config
            .models
            .iter()
            .map(|route| route.model.clone())
            .collect()
    }

    fn provider_for(&self, model: &str) -> Result<&dyn Provider, RouterError> {
        let provider_id = self
            .config
            .provider_of(model)
            .ok_or_else(|| RouterError::NoProvider(model.to_string()))?;
        self.providers
            .get(provider_id)
            .map(Arc::as_ref)
            .ok_or_else(|| RouterError::NoProvider(format!("{model} ({provider_id})")))
    }

    fn estimate_cost(&self, model: &str, request: &ChatRequest) -> f64 {
        let prompt = self.tokens.count_messages(&request.messages);
        let output = request.max_tokens.unwrap_or(256) as usize;
        let usage = Usage {
            input_tokens: prompt.min(u32::MAX as usize) as u32,
            output_tokens: output.min(u32::MAX as usize) as u32,
            cached_tokens: 0,
        };
        self.costs.cost(model, &usage)
    }

    fn emit(&self, event: LlmEvent) {
        self.sink.emit(event);
    }

    /// Route and run a non-streaming completion, falling back across models.
    pub async fn chat(
        &self,
        task: &str,
        mut request: ChatRequest,
    ) -> Result<ChatResponse, RouterError> {
        let candidates = self.candidates(task);
        if candidates.is_empty() {
            return Err(RouterError::NoRoute(task.to_string()));
        }

        let key = ResponseCache::key(&request, task);
        if let Some(cache) = &self.cache {
            if let Some(hit) = cache.get(key) {
                self.emit(LlmEvent::CacheHit {
                    model: hit.model.clone(),
                });
                return Ok(hit);
            }
        }

        let mut last_error: Option<String> = None;
        for model in candidates {
            let provider = match self.provider_for(&model) {
                Ok(provider) => provider,
                Err(err) => {
                    last_error = Some(err.to_string());
                    continue;
                }
            };
            request.model = model.clone();

            let estimated = self.estimate_cost(&model, &request);
            self.budget.check(estimated)?;

            self.emit(LlmEvent::Routed {
                task: task.to_string(),
                model: model.clone(),
                provider: provider.id().to_string(),
            });
            self.emit(LlmEvent::Request {
                task: task.to_string(),
                model: model.clone(),
            });

            let started = Instant::now();
            match provider.chat(request.clone()).await {
                Ok(response) => {
                    let usage = response.usage;
                    let cost = self.costs.cost(&model, &usage);
                    self.budget.record(&usage, cost);
                    self.emit(LlmEvent::Response {
                        model: model.clone(),
                        usage,
                        cost_usd: cost,
                        duration_ms: started.elapsed().as_millis() as u64,
                    });
                    if let Some(cache) = &self.cache {
                        cache.put(key, response.clone());
                    }
                    return Ok(response);
                }
                Err(err) => {
                    if matches!(err, ProviderError::RateLimited) {
                        self.emit(LlmEvent::RateLimited {
                            model: model.clone(),
                        });
                    }
                    self.emit(LlmEvent::Error {
                        model: model.clone(),
                        message: err.to_string(),
                    });
                    last_error = Some(err.to_string());
                }
            }
        }
        Err(RouterError::AllFailed(
            last_error.unwrap_or_else(|| "no candidate available".to_string()),
        ))
    }

    /// Route and run a streaming completion, falling back across models.
    pub async fn chat_stream(
        &self,
        task: &str,
        mut request: ChatRequest,
    ) -> Result<ProviderStream<'_>, RouterError> {
        let candidates = self.candidates(task);
        if candidates.is_empty() {
            return Err(RouterError::NoRoute(task.to_string()));
        }

        let mut last_error: Option<String> = None;
        for model in candidates {
            let provider = match self.provider_for(&model) {
                Ok(provider) => provider,
                Err(err) => {
                    last_error = Some(err.to_string());
                    continue;
                }
            };
            request.model = model.clone();
            self.emit(LlmEvent::Routed {
                task: task.to_string(),
                model: model.clone(),
                provider: provider.id().to_string(),
            });
            match provider.chat_stream(request.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(err) => {
                    self.emit(LlmEvent::Error {
                        model: model.clone(),
                        message: err.to_string(),
                    });
                    last_error = Some(err.to_string());
                }
            }
        }
        Err(RouterError::AllFailed(
            last_error.unwrap_or_else(|| "no candidate available".to_string()),
        ))
    }
}
