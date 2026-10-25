//! The provider registry: configured LLM backends and task routes (task 2.17).
//!
//! Configuration only. API keys are never stored here: a provider references a
//! secret by name and the vault resolves it at build time.

use std::sync::Arc;

use fergents_llm::{
    AnthropicProvider, ChatRequest, ChatResponse, GoogleProvider, MockProvider, ModelInfo,
    OpenAiCompatProvider, Provider, ProviderError, ProviderFuture, ProviderHealth, ProviderStream,
    StreamFrame,
};
use fergents_secrets::SecretVault;
use fergents_storage::{Storage, StorageError, Value};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised by the provider registry.
#[derive(Debug, Error)]
pub enum ProviderRegistryError {
    /// The id is empty or contains whitespace.
    #[error("invalid provider id: {0}")]
    InvalidId(String),
    /// No provider is configured under that id.
    #[error("unknown provider: {0}")]
    Unknown(String),
    /// The kind is not one the kernel can build.
    #[error("unknown provider kind: {0}")]
    InvalidKind(String),
    /// A required endpoint is missing.
    #[error("provider {0} requires a base_url")]
    MissingBaseUrl(String),
    /// A required API key is missing.
    #[error("provider {0} requires a secret")]
    MissingSecret(String),
    /// The referenced secret is missing.
    #[error(transparent)]
    Secret(#[from] fergents_secrets::SecretError),
    /// The provider call failed.
    #[error(transparent)]
    Llm(#[from] fergents_llm::ProviderError),
    /// The row could not be read or written.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// A configured provider (never holds a key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Stable id, e.g. `anthropic`.
    pub id: String,
    /// One of `anthropic`, `google`, `openai`, `ollama`, `mock`.
    pub kind: String,
    /// Endpoint override for compatible servers.
    pub base_url: Option<String>,
    /// Name of the secret holding the API key, if any.
    pub secret_name: Option<String>,
    /// Model ids advertised by this provider.
    pub models: Vec<String>,
    /// When the provider was configured (ms since the epoch).
    pub created_at: i64,
}

/// The outcome of `provider.test`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTest {
    /// The tested provider id.
    pub id: String,
    /// Whether the provider answered.
    pub ok: bool,
    /// Failure detail when `ok` is false.
    pub detail: Option<String>,
}

/// An ordered model chain for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRoute {
    /// The task name, e.g. `chat`.
    pub task: String,
    /// Candidate models, in preference order.
    pub models: Vec<String>,
}

const KINDS: [&str; 5] = ["anthropic", "google", "openai", "ollama", "mock"];

fn validate_id(id: &str) -> Result<(), ProviderRegistryError> {
    if id.is_empty() || id.len() > 64 || id.chars().any(char::is_whitespace) {
        return Err(ProviderRegistryError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn encode_models(models: &[String]) -> Result<String, ProviderRegistryError> {
    serde_json::to_string(models)
        .map_err(|err| ProviderRegistryError::InvalidId(format!("models: {err}")))
}

fn decode_models(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The provider registry facade.
pub struct ProviderService {
    storage: Arc<dyn Storage>,
    secrets: Arc<SecretVault>,
}

impl ProviderService {
    /// Build the registry over `storage`, resolving keys through `secrets`.
    pub fn new(storage: Arc<dyn Storage>, secrets: Arc<SecretVault>) -> Self {
        Self { storage, secrets }
    }

    /// Every configured provider, oldest first.
    pub fn list(&self) -> Result<Vec<ProviderConfig>, ProviderRegistryError> {
        let rows = self.storage.query(
            "SELECT id, kind, base_url, secret_name, models, created_at \
             FROM provider_configs ORDER BY created_at ASC",
            &[],
        )?;
        Ok(rows.into_iter().map(row_to_config).collect())
    }

    /// One provider by id.
    pub fn get(&self, id: &str) -> Result<ProviderConfig, ProviderRegistryError> {
        let rows = self.storage.query(
            "SELECT id, kind, base_url, secret_name, models, created_at \
             FROM provider_configs WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        rows.into_iter()
            .next()
            .map(row_to_config)
            .ok_or_else(|| ProviderRegistryError::Unknown(id.to_string()))
    }

    /// Add or replace a provider.
    pub fn add(
        &self,
        id: &str,
        kind: &str,
        base_url: Option<&str>,
        secret_name: Option<&str>,
        models: &[String],
    ) -> Result<ProviderConfig, ProviderRegistryError> {
        validate_id(id)?;
        if !KINDS.contains(&kind) {
            return Err(ProviderRegistryError::InvalidKind(kind.to_string()));
        }
        if kind == "openai" && base_url.is_none() {
            return Err(ProviderRegistryError::MissingBaseUrl(id.to_string()));
        }
        let created_at = self
            .storage
            .query(
                "SELECT created_at FROM provider_configs WHERE id = ?1",
                &[Value::String(id.to_string())],
            )?
            .first()
            .and_then(|row| row.get("created_at"))
            .and_then(Value::as_i64)
            .unwrap_or_else(now_ms);
        self.storage.execute(
            "INSERT INTO provider_configs (id, kind, base_url, secret_name, models, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, base_url = excluded.base_url, \
             secret_name = excluded.secret_name, models = excluded.models",
            &[
                Value::String(id.to_string()),
                Value::String(kind.to_string()),
                base_url.map_or(Value::Null, |url| Value::String(url.to_string())),
                secret_name.map_or(Value::Null, |name| Value::String(name.to_string())),
                Value::String(encode_models(models)?),
                Value::from(created_at),
            ],
        )?;
        self.get(id)
    }

    /// Remove a provider. Returns whether it existed.
    pub fn remove(&self, id: &str) -> Result<bool, ProviderRegistryError> {
        validate_id(id)?;
        let removed = self.storage.execute(
            "DELETE FROM provider_configs WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(removed > 0)
    }

    /// Resolve credentials and ask the backend how it is doing.
    pub async fn test(&self, id: &str) -> Result<ProviderTest, ProviderRegistryError> {
        let config = self.get(id)?;
        let provider = self.build(&config)?;
        match provider.health().await {
            Ok(health) => Ok(ProviderTest {
                id: id.to_string(),
                ok: health.ok,
                detail: health.detail,
            }),
            Err(err) => Ok(ProviderTest {
                id: id.to_string(),
                ok: false,
                detail: Some(err.to_string()),
            }),
        }
    }

    /// The model ids a provider advertises, falling back to the configured list.
    pub async fn models(&self, id: &str) -> Result<Vec<String>, ProviderRegistryError> {
        let config = self.get(id)?;
        let provider = self.build(&config)?;
        let advertised = provider.models().await.unwrap_or_default();
        if advertised.is_empty() {
            return Ok(config.models);
        }
        Ok(advertised.into_iter().map(|model| model.id).collect())
    }

    /// The provider configuration that advertises `model`.
    pub fn config_for_model(&self, model: &str) -> Result<ProviderConfig, ProviderRegistryError> {
        self.list()?
            .into_iter()
            .find(|config| config.models.iter().any(|id| id == model))
            .ok_or_else(|| ProviderRegistryError::Unknown(model.to_string()))
    }

    /// The model the kernel should use when nothing is pinned: the `chat`
    /// route first, then the first model of the first provider.
    pub fn default_model(&self) -> Option<String> {
        if let Ok(routes) = self.routes() {
            if let Some(model) = routes
                .into_iter()
                .find(|route| route.task == "chat")
                .and_then(|route| route.models.into_iter().next())
            {
                return Some(model);
            }
        }
        self.list()
            .ok()?
            .into_iter()
            .find_map(|config| config.models.into_iter().next())
    }

    /// Run a non-streaming completion against the provider serving `model`.
    pub async fn complete(
        &self,
        model: &str,
        mut request: ChatRequest,
    ) -> Result<ChatResponse, ProviderRegistryError> {
        let config = self.config_for_model(model)?;
        let provider = self.build(&config)?;
        request.model = model.to_string();
        Ok(provider.chat(request).await?)
    }

    /// Every task route, by task name.
    pub fn routes(&self) -> Result<Vec<ProviderRoute>, ProviderRegistryError> {
        let rows = self.storage.query(
            "SELECT task, models FROM provider_routes ORDER BY task ASC",
            &[],
        )?;
        Ok(rows
            .into_iter()
            .map(|row| ProviderRoute {
                task: row
                    .get("task")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                models: row
                    .get("models")
                    .and_then(Value::as_str)
                    .map(decode_models)
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// Replace the model chain for `task`.
    pub fn set_route(
        &self,
        task: &str,
        models: &[String],
    ) -> Result<ProviderRoute, ProviderRegistryError> {
        if task.is_empty() {
            return Err(ProviderRegistryError::InvalidId(task.to_string()));
        }
        self.storage.execute(
            "INSERT INTO provider_routes (task, models, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(task) DO UPDATE SET models = excluded.models, \
             updated_at = excluded.updated_at",
            &[
                Value::String(task.to_string()),
                Value::String(encode_models(models)?),
                Value::from(now_ms()),
            ],
        )?;
        Ok(ProviderRoute {
            task: task.to_string(),
            models: models.to_vec(),
        })
    }

    fn resolve_key(
        &self,
        config: &ProviderConfig,
    ) -> Result<Option<String>, ProviderRegistryError> {
        match &config.secret_name {
            Some(name) => Ok(Some(self.secrets.resolve(name)?)),
            None => Ok(None),
        }
    }

    /// Build a live provider from its configuration.
    fn build(&self, config: &ProviderConfig) -> Result<Arc<dyn Provider>, ProviderRegistryError> {
        let known = config
            .models
            .iter()
            .map(|id| ModelInfo::new(id.clone()))
            .collect::<Vec<_>>();
        let provider: Arc<dyn Provider> = match config.kind.as_str() {
            "mock" => Arc::new(MockProvider::new(config.id.clone()).with_models(known)),
            "ollama" => {
                let mut provider = OpenAiCompatProvider::new(
                    config.id.clone(),
                    config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
                );
                if let Some(key) = self.resolve_key(config)? {
                    provider = provider.with_api_key(key);
                }
                Arc::new(provider)
            }
            "openai" => {
                let url = config
                    .base_url
                    .clone()
                    .ok_or_else(|| ProviderRegistryError::MissingBaseUrl(config.id.clone()))?;
                let mut provider = OpenAiCompatProvider::new(config.id.clone(), url);
                if let Some(key) = self.resolve_key(config)? {
                    provider = provider.with_api_key(key);
                }
                Arc::new(provider)
            }
            "anthropic" => {
                let key = self
                    .resolve_key(config)?
                    .ok_or_else(|| ProviderRegistryError::MissingSecret(config.id.clone()))?;
                let mut provider = AnthropicProvider::new(key);
                if let Some(url) = &config.base_url {
                    provider = provider.with_base_url(url.clone());
                }
                Arc::new(provider)
            }
            "google" => {
                let key = self
                    .resolve_key(config)?
                    .ok_or_else(|| ProviderRegistryError::MissingSecret(config.id.clone()))?;
                let mut provider = GoogleProvider::new(key);
                if let Some(url) = &config.base_url {
                    provider = provider.with_base_url(url.clone());
                }
                Arc::new(provider)
            }
            other => return Err(ProviderRegistryError::InvalidKind(other.to_string())),
        };
        Ok(provider)
    }
}

/// The agent-runtime provider backed by the configured provider registry.
///
/// Every request is resolved to a configured provider: the requested model
/// (a chat session pin or an agent definition) wins when a provider advertises
/// it, otherwise the `chat` route, then the first configured model. When no
/// provider is configured at all the runtime serves the deterministic mock, so
/// the kernel still works fully offline.
pub struct RegistryProvider {
    providers: Arc<ProviderService>,
}

impl RegistryProvider {
    /// Wrap the provider registry.
    pub fn new(providers: Arc<ProviderService>) -> Self {
        Self { providers }
    }

    /// Resolve `requested` to a provider configuration and the model it serves.
    fn resolve(&self, requested: &str) -> Option<(ProviderConfig, String)> {
        let configs = self.providers.list().ok()?;
        if let Some(config) = configs
            .iter()
            .find(|config| config.models.iter().any(|model| model == requested))
        {
            return Some((config.clone(), requested.to_string()));
        }
        if let Ok(routes) = self.providers.routes() {
            if let Some(route) = routes.into_iter().find(|route| route.task == "chat") {
                for model in route.models {
                    if let Some(config) =
                        configs.iter().find(|config| config.models.contains(&model))
                    {
                        return Some((config.clone(), model));
                    }
                }
            }
        }
        let config = configs
            .into_iter()
            .find(|config| !config.models.is_empty())?;
        let model = config.models.first()?.clone();
        Some((config, model))
    }
}

impl Provider for RegistryProvider {
    fn id(&self) -> &str {
        "registry"
    }

    fn chat(&self, request: ChatRequest) -> ProviderFuture<'_, ChatResponse> {
        Box::pin(async move {
            let Some((config, model)) = self.resolve(&request.model) else {
                return MockProvider::new("mock").chat(request).await;
            };
            let provider = self.providers.build(&config).map_err(error_for)?;
            let mut request = request;
            request.model = model;
            provider.chat(request).await
        })
    }

    fn chat_stream<'a>(&'a self, request: ChatRequest) -> ProviderFuture<'a, ProviderStream<'a>> {
        Box::pin(async move {
            use futures::StreamExt;

            let Some((config, model)) = self.resolve(&request.model) else {
                let response = MockProvider::new("mock").chat(request).await?;
                return Ok(replay(response));
            };
            let provider = self.providers.build(&config).map_err(error_for)?;
            let mut request = request;
            request.model = model;
            if tokio::runtime::Handle::try_current().is_err() {
                // No executor to drive a forwarding task: collect, then replay.
                let mut stream = provider.chat_stream(request).await?;
                let mut frames = Vec::new();
                while let Some(frame) = stream.next().await {
                    frames.push(frame);
                }
                return Ok(Box::pin(futures::stream::iter(frames)) as ProviderStream<'_>);
            }
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                match provider.chat_stream(request).await {
                    Ok(mut stream) => {
                        while let Some(frame) = stream.next().await {
                            if tx.send(frame).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                    }
                }
            });
            let stream = futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|frame| (frame, rx))
            });
            Ok(Box::pin(stream) as ProviderStream<'_>)
        })
    }

    fn models(&self) -> ProviderFuture<'_, Vec<ModelInfo>> {
        Box::pin(async move {
            Ok(self
                .providers
                .list()
                .unwrap_or_default()
                .into_iter()
                .flat_map(|config| config.models.into_iter().map(ModelInfo::new))
                .collect())
        })
    }

    fn health(&self) -> ProviderFuture<'_, ProviderHealth> {
        Box::pin(async move {
            match self.providers.list() {
                Ok(configs) if !configs.is_empty() => Ok(ProviderHealth::healthy()),
                Ok(_) => Ok(ProviderHealth::unhealthy("no provider configured")),
                Err(err) => Ok(ProviderHealth::unhealthy(err.to_string())),
            }
        })
    }
}

/// Flatten a registry failure into the runtime's provider error.
fn error_for(err: ProviderRegistryError) -> ProviderError {
    match err {
        ProviderRegistryError::Llm(inner) => inner,
        other => ProviderError::Other(other.to_string()),
    }
}

/// Replay a completed response as a stream of frames.
fn replay(response: ChatResponse) -> ProviderStream<'static> {
    let mut frames: Vec<Result<StreamFrame, ProviderError>> = vec![Ok(StreamFrame::Start {
        model: response.model.clone(),
    })];
    let text = response.text();
    if !text.is_empty() {
        frames.push(Ok(StreamFrame::TextDelta { text }));
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
    Box::pin(futures::stream::iter(frames))
}

fn row_to_config(row: fergents_storage::Row) -> ProviderConfig {
    ProviderConfig {
        id: row
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: row
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: row
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        secret_name: row
            .get("secret_name")
            .and_then(Value::as_str)
            .map(str::to_string),
        models: row
            .get("models")
            .and_then(Value::as_str)
            .map(decode_models)
            .unwrap_or_default(),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
    }
}
