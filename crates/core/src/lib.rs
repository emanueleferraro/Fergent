//! Fergents kernel core: wiring, registries, and the kernel facade.
//!
//! The [`Kernel`] owns the storage, the event bus, and the runtime registries
//! (tools, providers, brains, sessions) and exposes the operations the API
//! server dispatches to (`ARCHITECTURE.md` §3.2).

mod brains;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use fergents_agents::{memory_tools, AgentService, AllowAllGate, MemoryStore, ToolRegistry};
use fergents_capabilities::{default_catalogue, CapabilityError, CapabilityRegistry};
use fergents_events::{now_ms, EventBus};
use fergents_llm::MockProvider;
use fergents_protocol::{
    BrainSummary, HealthSnapshot, HealthStatus, StorageStatus, SystemInfoResult,
};
use fergents_storage::{SqliteStorage, Storage, StorageError, Value};

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

/// A tool registered in the runtime registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    /// Tool name, e.g. `brain.search`.
    pub name: String,
    /// Short human-readable description.
    pub description: String,
}

/// The kernel facade.
pub struct Kernel {
    storage: Arc<SqliteStorage>,
    bus: Arc<EventBus>,
    brains: Arc<BrainService>,
    capabilities: Arc<CapabilityRegistry>,
    agents: Arc<AgentService>,
    memory: Arc<MemoryStore>,
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
        // The agent runtime. Until the provider manager (task 2.17) wires real
        // providers, the kernel runs agents against the deterministic mock
        // provider so `agent.run` works fully offline.
        let storage_for_agents: Arc<dyn Storage> = storage.clone();
        let storage_for_memory: Arc<dyn Storage> = storage.clone();
        let memory = Arc::new(MemoryStore::new(storage_for_memory));
        let provider: Arc<dyn fergents_llm::Provider> = Arc::new(MockProvider::new("mock"));
        let tools = Arc::new(ToolRegistry::new());
        for tool in memory_tools(memory.clone()) {
            tools.register(tool);
        }
        let agents = Arc::new(AgentService::new(
            config.data_dir.join("agents"),
            storage_for_agents,
            provider,
            tools,
            Arc::new(AllowAllGate),
            bus.clone(),
        ));
        let _ = agents.load();
        let kernel = Arc::new(Self {
            storage,
            bus,
            brains,
            capabilities,
            agents,
            memory,
            started: Instant::now(),
            started_ms: now_ms(),
            tools: RwLock::new(HashMap::new()),
            providers: RwLock::new(Vec::new()),
        });
        kernel.reconcile_sessions()?;
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
