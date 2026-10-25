//! Fergents kernel core: wiring, registries, and the kernel facade.
//!
//! The [`Kernel`] owns the storage, the event bus, and the runtime registries
//! (tools, providers, brains, sessions) and exposes the operations the API
//! server dispatches to (`ARCHITECTURE.md` §3.2).

mod brains;
mod composer;
mod inline;
mod mcp;
mod providers;
mod subagents;

pub use composer::{
    apply_hunks, diff_hunks, CompletionClient, ComposerError, ComposerService, ModelResolver,
    ProposalRequest,
};
pub use inline::{CompleteRequest, InlineError, InlineService};
pub use mcp::{McpHealth, McpServerRecord, McpService, McpServiceError, McpTest, McpTool};
pub use subagents::{SubAgentError, SubAgentManager, SubAgentRecord};

pub use providers::{
    ProviderConfig, ProviderRegistryError, ProviderRoute, ProviderService, ProviderTest,
    RegistryProvider,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use fergents_agents::{
    memory_tools, AgentService, ContextBuilder, ContextSnapshot, MemoryKind, MemoryScope,
    MemoryStore, SectionInput, SectionKind, ToolRegistry, WorkflowService,
};
use fergents_capabilities::{default_catalogue, CapabilityError, CapabilityRegistry};
use fergents_chat::{ChatMessage, ChatService};
use fergents_events::{now_ms, EventBus};
use fergents_git::GitService;
use fergents_ide::{file_tools, FileService, ProjectService};
use fergents_llm::ChatRequest;
use fergents_lsp::LspService;
use fergents_permissions::PermissionService;
use fergents_protocol::{
    BrainSummary, HealthSnapshot, HealthStatus, StorageStatus, SystemInfoResult,
};
use fergents_reversibility::ReversibilityService;
use fergents_secrets::{Keychain, KeyringKeychain, SecretVault};
use fergents_storage::{SqliteStorage, Storage, StorageError, Value};
use fergents_terminal::{terminal_tools, TerminalService};

pub use brains::{BrainService, BusSink};

/// Errors raised by the kernel core.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// A brain subsystem failure.
    #[error(transparent)]
    Brain(#[from] fergents_brain::BrainError),
    /// A per-brain index failure.
    #[error(transparent)]
    Index(#[from] fergents_index::IndexError),
    /// A graph subsystem failure.
    #[error(transparent)]
    Graph(#[from] fergents_graph::GraphError),
    /// A capability registry failure.
    #[error(transparent)]
    Capability(#[from] CapabilityError),
    /// An agent runtime failure.
    #[error(transparent)]
    Agent(#[from] fergents_agents::AgentError),
    /// A permission engine failure.
    #[error(transparent)]
    Permission(#[from] fergents_permissions::PermissionError),
    /// A reversibility failure.
    #[error(transparent)]
    Reversibility(#[from] fergents_reversibility::ReversibilityError),
    /// A chat failure.
    #[error(transparent)]
    Chat(#[from] fergents_chat::ChatError),
    /// A secret vault failure.
    #[error(transparent)]
    Secret(#[from] fergents_secrets::SecretError),
    /// A provider registry failure.
    #[error(transparent)]
    Provider(#[from] ProviderRegistryError),
    /// An MCP registry failure.
    #[error(transparent)]
    Mcp(#[from] McpServiceError),
    /// A project layer failure.
    #[error(transparent)]
    Project(#[from] fergents_ide::ProjectError),
    /// An LSP failure.
    #[error(transparent)]
    Lsp(#[from] fergents_lsp::LspError),
    /// A terminal failure.
    #[error(transparent)]
    Terminal(#[from] fergents_terminal::TerminalError),
    /// A git failure.
    #[error(transparent)]
    Git(#[from] fergents_git::GitError),
    /// A Composer failure.
    #[error(transparent)]
    Composer(#[from] ComposerError),
    /// An Inline AI failure.
    #[error(transparent)]
    Inline(#[from] InlineError),
}

/// Kernel configuration.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Directory holding the global store and runtime artifacts
    /// (defaults to `~/.fergents`).
    pub data_dir: PathBuf,
    /// Directory containing numbered SQL migrations.
    pub migrations_dir: PathBuf,
    /// Broadcast buffer capacity per subscriber.
    pub event_capacity: usize,
}

impl KernelConfig {
    /// Path of the global SQLite store.
    pub fn global_db_path(&self) -> PathBuf {
        self.data_dir.join("global.sqlite")
    }
}

/// Render memory entries as a bulleted list for a context section.
fn render_memory(entries: &[fergents_agents::MemoryEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("- {}", entry.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A tool registered in the runtime registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    /// Tool name, e.g. `brain.search`.
    pub name: String,
    /// Short human-readable description.
    pub description: String,
}

/// One side of a compare run (spec decision D9).
#[derive(Debug, Clone)]
pub struct CompareSide {
    /// The session holding this side of the comparison.
    pub session_id: String,
    /// The model that answered.
    pub model: String,
    /// The assistant reply, when the call succeeded.
    pub message: Option<ChatMessage>,
    /// The failure detail, when the call failed.
    pub error: Option<String>,
}

/// The kernel facade.
pub struct Kernel {
    storage: Arc<SqliteStorage>,
    bus: Arc<EventBus>,
    brains: Arc<BrainService>,
    capabilities: Arc<CapabilityRegistry>,
    agents: Arc<AgentService>,
    memory: Arc<MemoryStore>,
    context: Arc<ContextBuilder>,
    workflow: Arc<WorkflowService>,
    permissions: Arc<PermissionService>,
    reversibility: Arc<ReversibilityService>,
    chat: Arc<ChatService>,
    secrets: Arc<SecretVault>,
    provider_service: Arc<ProviderService>,
    mcp_service: Arc<McpService>,
    projects: Arc<ProjectService>,
    files: Arc<FileService>,
    lsp: Arc<LspService>,
    terminal: Arc<TerminalService>,
    git: Arc<GitService>,
    composer: Arc<ComposerService>,
    inline: Arc<InlineService>,
    subagents: Arc<SubAgentManager>,
    started: Instant,
    started_ms: i64,
    tools: RwLock<HashMap<String, ToolInfo>>,
    providers: RwLock<Vec<String>>,
}

impl Kernel {
    /// Start the kernel: open storage (applying migrations), reconcile stale
    /// sessions, create the event bus, wire the brain subsystem, and emit
    /// `kernel.started`.
    pub fn start(config: &KernelConfig) -> Result<Arc<Self>, CoreError> {
        Self::start_with_keychain(config, Arc::new(KeyringKeychain::new("fergents")))
    }

    /// Start the kernel with an explicit keychain backend (tests, headless tooling).
    pub fn start_with_keychain(
        config: &KernelConfig,
        keychain: Arc<dyn Keychain>,
    ) -> Result<Arc<Self>, CoreError> {
        std::fs::create_dir_all(&config.data_dir).ok();
        let storage = SqliteStorage::open(&config.global_db_path(), &config.migrations_dir)?;
        let storage = Arc::new(storage);
        let bus = Arc::new(EventBus::new(storage.clone(), config.event_capacity));
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let brains = Arc::new(BrainService::new(
            storage_dyn,
            bus.clone(),
            config.migrations_dir.join("brain"),
            config.data_dir.join("templates/brains"),
            config.data_dir.join("brains"),
        ));
        let storage_for_caps: Arc<dyn Storage> = storage.clone();
        let capabilities = Arc::new(CapabilityRegistry::new(storage_for_caps, bus.clone())?);
        capabilities.seed(&default_catalogue())?;
        // The agent runtime routes every request through the configured
        // providers (task 2.17); with none configured it stays on the
        // deterministic mock so `agent.run` still works fully offline.
        let storage_for_agents: Arc<dyn Storage> = storage.clone();
        let storage_for_memory: Arc<dyn Storage> = storage.clone();
        let storage_for_context: Arc<dyn Storage> = storage.clone();
        let memory = Arc::new(MemoryStore::new(storage_for_memory));
        let context = Arc::new(ContextBuilder::new(storage_for_context));
        let storage_for_runs: Arc<dyn Storage> = storage.clone();
        let workflow = Arc::new(WorkflowService::new(
            storage_for_runs,
            bus.clone(),
            config.data_dir.join("specs"),
        ));
        let storage_for_perms: Arc<dyn Storage> = storage.clone();
        let permissions = Arc::new(PermissionService::new(storage_for_perms, bus.clone()));
        let storage_for_backups: Arc<dyn Storage> = storage.clone();
        let reversibility = Arc::new(ReversibilityService::new(
            storage_for_backups,
            bus.clone(),
            config.data_dir.join("backups"),
        ));
        let tools = Arc::new(ToolRegistry::new());
        for tool in memory_tools(memory.clone()) {
            tools.register(tool);
        }
        let storage_for_chat: Arc<dyn Storage> = storage.clone();
        let chat = Arc::new(ChatService::new(storage_for_chat, bus.clone()));
        let storage_for_secrets: Arc<dyn Storage> = storage.clone();
        let secrets = Arc::new(SecretVault::new(storage_for_secrets, keychain));
        let storage_for_providers: Arc<dyn Storage> = storage.clone();
        let provider_service =
            Arc::new(ProviderService::new(storage_for_providers, secrets.clone()));
        let provider: Arc<dyn fergents_llm::Provider> =
            Arc::new(RegistryProvider::new(provider_service.clone()));
        let storage_for_mcp: Arc<dyn Storage> = storage.clone();
        let mcp_service = Arc::new(McpService::new(storage_for_mcp));
        let storage_for_projects: Arc<dyn Storage> = storage.clone();
        let projects = Arc::new(ProjectService::new(storage_for_projects, bus.clone()));
        let files = Arc::new(FileService::new(
            projects.clone(),
            reversibility.clone(),
            bus.clone(),
        ));
        for tool in file_tools(files.clone()) {
            tools.register(tool);
        }
        // Composer and Inline AI share one completion surface and one model
        // resolver (task routes first, then the first configured provider).
        let completion: Arc<dyn CompletionClient> = Arc::new(RegistryCompletion {
            providers: provider_service.clone(),
        });
        let model_for = task_model_resolver(provider_service.clone());
        let composer = Arc::new(ComposerService::new(
            projects.clone(),
            files.clone(),
            bus.clone(),
            completion.clone(),
            model_for.clone(),
        ));
        let inline = Arc::new(InlineService::new(
            projects.clone(),
            composer.clone(),
            completion,
            model_for,
            bus.clone(),
        ));
        let lsp = Arc::new(LspService::new(
            storage.clone(),
            bus.clone(),
            projects.clone(),
        ));
        let terminal = Arc::new(TerminalService::new(
            storage.clone(),
            bus.clone(),
            projects.clone(),
        ));
        for tool in terminal_tools(terminal.clone()) {
            tools.register(tool);
        }
        // The dispatch layer enforces `capability.git-remote` for fetch/pull/push.
        let git = Arc::new(GitService::new(
            storage.clone(),
            bus.clone(),
            projects.clone(),
            true,
        ));
        let agents = Arc::new(AgentService::new(
            config.data_dir.join("agents"),
            storage_for_agents,
            provider,
            tools,
            permissions.gate(None, None),
            bus.clone(),
        ));
        let _ = agents.load();
        let subagents = Arc::new(SubAgentManager::new(agents.clone(), bus.clone()));
        let kernel = Arc::new(Self {
            storage,
            bus,
            brains,
            capabilities,
            agents,
            memory,
            context,
            workflow,
            permissions,
            reversibility,
            chat,
            secrets,
            provider_service,
            mcp_service,
            projects,
            files,
            lsp,
            terminal,
            git,
            composer,
            inline,
            subagents,
            started: Instant::now(),
            started_ms: now_ms(),
            tools: RwLock::new(HashMap::new()),
            providers: RwLock::new(Vec::new()),
        });
        kernel.reconcile_sessions()?;
        // Resume watching every known project (file.changed events).
        let _ = kernel.files.watch_all();
        // PTY sessions do not survive a kernel restart.
        let _ = kernel.terminal.reconcile();
        kernel.bus.publish(
            "kernel.started",
            "kernel",
            serde_json::json!({ "version": fergents_protocol::KERNEL_VERSION }),
        )?;
        Ok(kernel)
    }

    /// The shared event bus.
    pub fn bus(&self) -> &EventBus {
        self.bus.as_ref()
    }

    /// The brain subsystem (manager, indexer, and resolved paths).
    pub fn brains(&self) -> &BrainService {
        self.brains.as_ref()
    }

    /// The capability registry.
    pub fn capabilities(&self) -> &CapabilityRegistry {
        self.capabilities.as_ref()
    }

    /// The agent runtime service.
    pub fn agents(&self) -> &AgentService {
        self.agents.as_ref()
    }

    /// The shared agent service handle, for offloading/awaiting runs.
    pub fn agents_service(&self) -> &Arc<AgentService> {
        &self.agents
    }

    /// The agent memory store.
    pub fn memory(&self) -> &MemoryStore {
        self.memory.as_ref()
    }

    /// The context builder (Context Viewer backend).
    pub fn context(&self) -> &ContextBuilder {
        self.context.as_ref()
    }

    /// The spec/plan/tasks workflow service.
    pub fn workflow(&self) -> &WorkflowService {
        self.workflow.as_ref()
    }

    /// The permission engine.
    pub fn permissions(&self) -> &PermissionService {
        self.permissions.as_ref()
    }

    /// The shared permission engine, for constructing per-session gates.
    pub fn permissions_service(&self) -> &Arc<PermissionService> {
        &self.permissions
    }

    /// The reversibility (backup/restore) service.
    pub fn reversibility(&self) -> &ReversibilityService {
        self.reversibility.as_ref()
    }

    /// The chat backend.
    pub fn chat(&self) -> &ChatService {
        self.chat.as_ref()
    }

    /// The secret vault (OS keychain values, reference-only rows).
    pub fn secrets(&self) -> &SecretVault {
        self.secrets.as_ref()
    }

    /// The provider registry and task routes.
    pub fn providers(&self) -> &ProviderService {
        self.provider_service.as_ref()
    }

    /// The MCP server registry and live host.
    pub fn mcp(&self) -> &McpService {
        self.mcp_service.as_ref()
    }

    /// The project layer (open, trust, file index, search).
    pub fn projects(&self) -> &ProjectService {
        self.projects.as_ref()
    }

    /// Project-scoped file operations, backups, and the external-change watcher.
    pub fn files(&self) -> &FileService {
        self.files.as_ref()
    }

    /// Language-server sessions, document sync, and diagnostics.
    pub fn lsp(&self) -> &LspService {
        self.lsp.as_ref()
    }

    /// PTY sessions, scrollback, and visible shell execution.
    pub fn terminal(&self) -> &TerminalService {
        self.terminal.as_ref()
    }

    /// Repository status, diffs, commits, branches, and history.
    pub fn git(&self) -> &GitService {
        self.git.as_ref()
    }

    /// The Composer: model-authored multi-file proposals with per-hunk approval.
    pub fn composer(&self) -> &ComposerService {
        self.composer.as_ref()
    }

    /// Inline AI: bounded completions and selection quick actions.
    pub fn inline(&self) -> &InlineService {
        self.inline.as_ref()
    }

    /// The sub-agent manager (async children, collect, cancel, handoff).
    pub fn subagents(&self) -> &SubAgentManager {
        self.subagents.as_ref()
    }

    /// Run one prompt against several models in parallel, each side in its own
    /// session (`capability.compare`, spec decision D9).
    pub async fn compare(
        &self,
        agent_id: &str,
        prompt: &str,
        models: &[String],
        title: Option<&str>,
    ) -> Result<Vec<CompareSide>, CoreError> {
        if models.len() < 2 {
            return Err(fergents_chat::ChatError::Invalid(
                "compare needs at least two models".to_string(),
            )
            .into());
        }
        let mut sessions = Vec::with_capacity(models.len());
        for model in models {
            let session = self.chat.create_session(agent_id, title, Some(model))?;
            self.chat.send(&session.id, prompt)?;
            sessions.push((session.id, model.clone()));
        }
        let responses = futures::future::join_all(sessions.iter().map(|(_, model)| {
            self.providers()
                .complete(model, ChatRequest::user(model.clone(), prompt))
        }))
        .await;
        let mut sides = Vec::with_capacity(sessions.len());
        for ((session_id, model), response) in sessions.into_iter().zip(responses) {
            match response {
                Ok(response) => {
                    let message =
                        self.chat
                            .append(&session_id, "assistant", &response.text(), None)?;
                    sides.push(CompareSide {
                        session_id,
                        model,
                        message: Some(message),
                        error: None,
                    });
                }
                Err(err) => sides.push(CompareSide {
                    session_id,
                    model,
                    message: None,
                    error: Some(err.to_string()),
                }),
            }
        }
        Ok(sides)
    }

    /// Assemble, count, persist, and emit a context snapshot for a session.
    ///
    /// The `rag` section is included only when `capability.search` is enabled.
    pub fn build_context(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
        input: &str,
    ) -> Result<ContextSnapshot, CoreError> {
        let settings = self.context.settings()?;
        let mut parts = Vec::new();

        let system = agent_id
            .and_then(|id| self.agents.get(id))
            .map_or_else(String::new, |definition| definition.system_prompt);
        parts.push(
            SectionInput::new(SectionKind::System, system)
                .with_refs(vec![format!("agent:{}", agent_id.unwrap_or("none"))])
                .cacheable(),
        );

        let capabilities = self
            .capabilities
            .list()
            .into_iter()
            .filter(|capability| capability.enabled)
            .map(|capability| capability.id)
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(SectionInput::new(SectionKind::Capabilities, capabilities).cacheable());
        parts.push(SectionInput::new(
            SectionKind::Constraints,
            "permission mode: auto",
        ));

        if let Some(id) = agent_id {
            let scope = MemoryScope::Agent(id.to_string());
            let facts = self.memory.list(&scope, Some(MemoryKind::Fact))?;
            if !facts.is_empty() {
                parts.push(SectionInput::new(
                    SectionKind::MemorySemantic,
                    render_memory(&facts),
                ));
            }
            let episodic = self.memory.list(&scope, Some(MemoryKind::Episodic))?;
            if !episodic.is_empty() {
                parts.push(SectionInput::new(
                    SectionKind::MemoryEpisodic,
                    render_memory(&episodic),
                ));
            }
        }

        // RAG is capability-gated: the section does not exist when search is off.
        if self.capabilities.is_enabled("capability.search") {
            parts.push(SectionInput::new(SectionKind::Rag, String::new()));
        }

        if let Some(session) = self.agents.session(session_id)? {
            if let Some(summary) = session.summary {
                parts.push(SectionInput::new(SectionKind::Summary, summary));
            }
        }

        parts.push(SectionInput::new(SectionKind::Input, input.to_string()));

        let turn_seq = self.context.snapshots(session_id)?.len() as u32;
        let snapshot = self.context.build(session_id, turn_seq, parts, &settings);
        self.context.persist(&snapshot)?;
        self.bus.publish(
            "context.built",
            "context",
            serde_json::json!({
                "session_id": session_id,
                "turn_seq": turn_seq,
                "total_tokens": snapshot.total_tokens,
                "sections": snapshot
                    .sections
                    .iter()
                    .map(|section| serde_json::json!({
                        "name": section.name,
                        "tokens": section.tokens,
                        "enabled": section.enabled,
                    }))
                    .collect::<Vec<_>>(),
            }),
        )?;
        Ok(snapshot)
    }

    /// The shared brain subsystem handle, for offloading synchronous work.
    pub fn brains_service(&self) -> &Arc<BrainService> {
        &self.brains
    }

    /// The storage backend.
    pub fn storage(&self) -> &Arc<SqliteStorage> {
        &self.storage
    }

    /// Close any sessions left open by an unclean previous stop.
    pub fn reconcile_sessions(&self) -> Result<usize, CoreError> {
        let affected = self.storage.execute(
            "UPDATE sessions SET closed_at = ?1 WHERE closed_at IS NULL",
            &[Value::from(now_ms())],
        )?;
        Ok(affected)
    }

    /// Register a new client session and return its id.
    pub fn session_start(
        &self,
        client_kind: &str,
        client_version: Option<&str>,
        protocol_version: u32,
        capabilities: &[String],
    ) -> Result<String, CoreError> {
        let id = ulid::Ulid::new().to_string();
        let caps = serde_json::to_string(capabilities).unwrap_or_else(|_| "[]".to_string());
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO sessions \
             (id, client_kind, client_version, protocol_version, capabilities, started_at, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                Value::String(id.clone()),
                Value::String(client_kind.to_string()),
                client_version.map_or(Value::Null, |v| Value::String(v.to_string())),
                Value::from(protocol_version),
                Value::String(caps),
                Value::from(now),
                Value::from(now),
            ],
        )?;
        Ok(id)
    }

    /// Mark a session as closed.
    pub fn session_end(&self, session_id: &str) -> Result<(), CoreError> {
        self.storage.execute(
            "UPDATE sessions SET closed_at = ?1, last_seen_at = ?1 WHERE id = ?2",
            &[Value::from(now_ms()), Value::String(session_id.to_string())],
        )?;
        Ok(())
    }

    /// Register a tool in the in-memory runtime registry.
    pub fn register_tool(&self, name: &str, description: &str) {
        let mut tools = self.tools.write().expect("tools registry poisoned");
        tools.insert(
            name.to_string(),
            ToolInfo {
                name: name.to_string(),
                description: description.to_string(),
            },
        );
    }

    /// List the registered tools, sorted by name.
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        let tools = self.tools.read().expect("tools registry poisoned");
        let mut out: Vec<ToolInfo> = tools.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Register a provider id in the in-memory runtime registry.
    pub fn register_provider(&self, provider_id: &str) {
        let mut providers = self.providers.write().expect("providers registry poisoned");
        if !providers.iter().any(|p| p == provider_id) {
            providers.push(provider_id.to_string());
        }
    }

    /// List the registered providers.
    pub fn list_providers(&self) -> Vec<String> {
        self.providers
            .read()
            .expect("providers registry poisoned")
            .clone()
    }

    /// List the registered brains, ordered by name.
    pub fn list_brains(&self) -> Result<Vec<BrainSummary>, CoreError> {
        let rows = self
            .storage
            .query("SELECT * FROM brains ORDER BY name", &[])?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let brain_id = row.get("brain_id").and_then(Value::as_str)?.to_string();
                Some(BrainSummary {
                    brain_id,
                    name: row.get("name").and_then(Value::as_str)?.to_string(),
                    path: row.get("path").and_then(Value::as_str)?.to_string(),
                    role: row.get("role").and_then(Value::as_str).map(str::to_string),
                })
            })
            .collect())
    }

    /// Current health snapshot.
    pub fn health(&self) -> HealthSnapshot {
        let storage_ok = self.storage.query("SELECT 1", &[]).is_ok();
        let storage = if storage_ok {
            StorageStatus::Ok
        } else {
            StorageStatus::Error
        };
        let status = if storage_ok {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };
        HealthSnapshot {
            status,
            uptime_ms: self.started.elapsed().as_millis() as i64,
            kernel_version: fergents_protocol::KERNEL_VERSION.to_string(),
            protocol_version: fergents_protocol::PROTOCOL_VERSION,
            storage,
        }
    }

    /// Kernel/system identity.
    pub fn system_info(&self) -> SystemInfoResult {
        SystemInfoResult {
            kernel_version: fergents_protocol::KERNEL_VERSION.to_string(),
            protocol_version: fergents_protocol::PROTOCOL_VERSION,
            os: std::env::consts::OS.to_string(),
            pid: std::process::id(),
        }
    }

    /// Emit `kernel.stopping`. Call before shutting down.
    pub fn announce_stopping(&self) -> Result<(), CoreError> {
        self.bus
            .publish("kernel.stopping", "kernel", serde_json::json!({}))?;
        Ok(())
    }

    /// Emit `kernel.error` for an unrecoverable error. Call before exiting.
    pub fn announce_error(&self, message: &str) -> Result<(), CoreError> {
        self.bus.publish(
            "kernel.error",
            "kernel",
            serde_json::json!({ "message": message }),
        )?;
        Ok(())
    }

    /// Milliseconds since the Unix epoch at kernel start.
    pub fn started_ms(&self) -> i64 {
        self.started_ms
    }
}

/// Locate the repository `migrations/` directory from a starting path.
///
/// Walks up until a `migrations/` directory is found.
pub fn find_migrations_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(current) = dir {
        let candidate = current.join("migrations");
        if candidate.is_dir() {
            return Some(candidate);
        }
        dir = current.parent();
    }
    None
}

/// The default data directory: `$FERGENTS_HOME` or `~/.fergents`.
pub fn default_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("FERGENTS_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".fergents")
}

/// The completion surface backed by the provider registry.
struct RegistryCompletion {
    providers: Arc<ProviderService>,
}

impl CompletionClient for RegistryCompletion {
    fn complete<'a>(
        &'a self,
        model: &'a str,
        request: ChatRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(async move {
            self.providers
                .complete(model, request)
                .await
                .map(|response| response.text())
                .map_err(|err| err.to_string())
        })
    }
}

/// Resolve a model for a task: its route first, then the first provider model.
fn task_model_resolver(providers: Arc<ProviderService>) -> ModelResolver {
    Arc::new(move |task: &str| {
        if let Ok(routes) = providers.routes() {
            if let Some(model) = routes
                .into_iter()
                .find(|route| route.task == task)
                .and_then(|route| route.models.into_iter().next())
            {
                return Some(model);
            }
        }
        providers
            .list()
            .ok()?
            .into_iter()
            .find_map(|config| config.models.into_iter().next())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> (KernelConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        (
            KernelConfig {
                data_dir: dir.path().to_path_buf(),
                migrations_dir,
                event_capacity: 64,
            },
            dir,
        )
    }

    #[test]
    fn start_reports_healthy() {
        let (cfg, _dir) = config();
        let kernel = Kernel::start(&cfg).unwrap();
        let health = kernel.health();
        assert_eq!(health.status, HealthStatus::Healthy);
        assert_eq!(health.storage, StorageStatus::Ok);
        assert_eq!(health.protocol_version, fergents_protocol::PROTOCOL_VERSION);
    }

    #[test]
    fn restart_reconciles_open_sessions() {
        let (cfg, _dir) = config();
        let session_id = {
            let kernel = Kernel::start(&cfg).unwrap();
            kernel.session_start("cli", Some("0.1.0"), 1, &[]).unwrap()
        };
        // Simulate a fresh kernel start on the same data dir.
        let kernel = Kernel::start(&cfg).unwrap();
        let rows = kernel
            .storage()
            .query(
                "SELECT closed_at FROM sessions WHERE id = ?1",
                &[Value::String(session_id)],
            )
            .unwrap();
        assert!(rows[0].get("closed_at").and_then(Value::as_i64).is_some());
        assert!(kernel.session_end("does-not-exist").is_ok());
    }

    #[test]
    fn tools_and_providers_registry() {
        let (cfg, _dir) = config();
        let kernel = Kernel::start(&cfg).unwrap();
        kernel.register_tool("brain.search", "Search the brain.");
        kernel.register_tool("web.fetch", "Fetch a URL.");
        kernel.register_provider("ollama");
        kernel.register_provider("ollama");
        let names: Vec<String> = kernel.list_tools().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["brain.search", "web.fetch"]);
        assert_eq!(kernel.list_providers(), vec!["ollama"]);
    }

    #[test]
    fn brains_are_empty_in_phase_zero() {
        let (cfg, _dir) = config();
        let kernel = Kernel::start(&cfg).unwrap();
        assert!(kernel.list_brains().unwrap().is_empty());
    }

    #[test]
    fn lifecycle_error_event_is_persisted() {
        let (cfg, _dir) = config();
        let kernel = Kernel::start(&cfg).unwrap();
        kernel.announce_error("boom").unwrap();
        let events = kernel.bus().replay(&[], None, Some(100)).unwrap();
        assert!(events.iter().any(|e| e.kind == "kernel.error"));
    }

    #[test]
    fn capabilities_are_seeded_at_start() {
        let (cfg, _dir) = config();
        let kernel = Kernel::start(&cfg).unwrap();
        let caps = kernel.capabilities();
        assert!(caps.count() >= 20);
        assert!(caps.is_enabled("capability.search"));
        assert!(caps.is_enabled("capability.chat"));
        assert!(!caps.is_enabled("capability.charts"));
        assert!(caps.require("capability.search").is_ok());
        assert!(caps.require("capability.charts").is_err());
    }
}
