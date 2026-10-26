//! The plugin service: install, registry, load, tools, and dev mode.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use fergents_capabilities::{Capability, CapabilityError};
use fergents_events::{now_ms, EventBus};
use fergents_packages::{
    create_archive, extract_archive, verify_signature, ArchiveLimits, SignatureStatus,
};
use fergents_permissions::PermissionService;
use fergents_storage::{Storage, Value};
use fergents_wasm::{ComponentHandle, Limits, Sandbox};
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use serde_json::json;

use crate::error::PluginError;
use crate::host::{Grants, HostServices, PluginHostBridge};
use crate::manifest::PluginManifest;
use crate::record::{InstallReview, PermissionGrant, PluginRecord, PluginView, ToolSpec};

/// Where a plugin install comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// A `.fergplugin` archive.
    File(PathBuf),
    /// A directory.
    Dir(PathBuf),
    /// A git repository.
    Git(String),
}

impl PluginSource {
    /// Parse a source from command params.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, PluginError> {
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("dir");
        match kind {
            "file" => Ok(PluginSource::File(PathBuf::from(required(value, "path")?))),
            "dir" => Ok(PluginSource::Dir(PathBuf::from(required(value, "path")?))),
            "git" => Ok(PluginSource::Git(required(value, "url")?)),
            other => Err(PluginError::UnsupportedSource(other.to_string())),
        }
    }

    /// A short label for the install review.
    pub fn label(&self) -> String {
        match self {
            PluginSource::File(path) => format!("file:{}", path.display()),
            PluginSource::Dir(path) => format!("dir:{}", path.display()),
            PluginSource::Git(url) => format!("git:{url}"),
        }
    }
}

fn required(value: &serde_json::Value, key: &str) -> Result<String, PluginError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| PluginError::InvalidManifest(format!("missing {key}")))
}

struct Loaded {
    record: PluginRecord,
    manifest: PluginManifest,
    component: Option<ComponentHandle>,
    tools: Vec<ToolSpec>,
    error: Option<String>,
}

struct PendingInstall {
    dir: PathBuf,
    manifest: PluginManifest,
    digest: String,
    signature: SignatureStatus,
    source: String,
    created_at: i64,
}

/// The plugin service.
pub struct PluginService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    data_dir: PathBuf,
    sandbox: Arc<Sandbox>,
    host: Arc<dyn HostServices>,
    policy: Option<Arc<PermissionService>>,
    capabilities: Arc<fergents_capabilities::CapabilityRegistry>,
    loaded: RwLock<HashMap<String, Loaded>>,
    reviews: Mutex<HashMap<String, PendingInstall>>,
    watchers: Mutex<Vec<notify::RecommendedWatcher>>,
}

impl PluginService {
    /// Build the service.
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        data_dir: PathBuf,
        sandbox: Arc<Sandbox>,
        host: Arc<dyn HostServices>,
        policy: Option<Arc<PermissionService>>,
        capabilities: Arc<fergents_capabilities::CapabilityRegistry>,
    ) -> Self {
        Self {
            storage,
            bus,
            data_dir,
            sandbox,
            host,
            policy,
            capabilities,
            loaded: RwLock::new(HashMap::new()),
            reviews: Mutex::new(HashMap::new()),
            watchers: Mutex::new(Vec::new()),
        }
    }

    /// Load every enabled plugin; return the number loaded.
    pub fn start(&self) -> Result<usize, PluginError> {
        let mut count = 0;
        for record in self.records()? {
            if !record.enabled {
                continue;
            }
            if self.load_record(&record).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// All installed plugins.
    pub fn list(&self) -> Result<Vec<PluginView>, PluginError> {
        self.records()?
            .into_iter()
            .map(|record| self.view_record(record))
            .collect()
    }

    /// One installed plugin.
    pub fn get(&self, id: &str) -> Result<PluginView, PluginError> {
        let record = self.record(id)?;
        self.view_record(record)
    }

    /// Start an install review without changing the installed set.
    pub fn inspect(&self, source: &PluginSource) -> Result<InstallReview, PluginError> {
        let review_id = ulid::Ulid::new().to_string();
        let dir = self.data_dir.join("cache").join("reviews").join(&review_id);
        std::fs::create_dir_all(&dir)?;

        match source {
            PluginSource::File(path) => {
                let bytes = std::fs::read(path)?;
                extract_archive(&bytes, &dir, &ArchiveLimits::default())?;
            }
            PluginSource::Dir(path) => copy_dir(path, &dir)?,
            PluginSource::Git(url) => {
                git2::Repository::clone(url, &dir)
                    .map_err(|err| PluginError::UnsupportedSource(err.to_string()))?;
            }
        }

        let manifest = read_manifest(&dir)?;
        manifest.validate_files(&dir)?;
        let digest = fergents_packages::compute_digest(&dir)?;
        let signature = self.signature_status(&dir)?;
        let permissions = manifest
            .permissions
            .iter()
            .map(|(category, scope)| PermissionGrant {
                category: category.clone(),
                scope: scope_to_json(scope),
                granted: false,
            })
            .collect::<Vec<_>>();
        let mut conflicts = Vec::new();
        if self.record(&manifest.plugin.id).is_ok() {
            conflicts.push(format!(
                "plugin {} is already installed",
                manifest.plugin.id
            ));
        }

        let review = InstallReview {
            review_id: review_id.clone(),
            manifest,
            digest,
            signature_status: signature.as_str().to_string(),
            permissions,
            conflicts,
            source: source.label(),
        };
        self.reviews
            .lock()
            .expect("plugin reviews poisoned")
            .insert(
                review_id,
                PendingInstall {
                    dir,
                    manifest: review.manifest.clone(),
                    digest: review.digest.clone(),
                    signature,
                    source: review.source.clone(),
                    created_at: now_ms(),
                },
            );
        Ok(review)
    }

    /// Install a reviewed plugin after the user approves every permission.
    pub fn install(&self, review_id: &str, approved: &[String]) -> Result<PluginView, PluginError> {
        self.install_internal(review_id, approved, false)
    }

    /// Update an installed plugin from a review.
    pub fn update(&self, review_id: &str, approved: &[String]) -> Result<PluginView, PluginError> {
        self.install_internal(review_id, approved, true)
    }

    fn install_internal(
        &self,
        review_id: &str,
        approved: &[String],
        updating: bool,
    ) -> Result<PluginView, PluginError> {
        let pending = self
            .reviews
            .lock()
            .expect("plugin reviews poisoned")
            .remove(review_id)
            .ok_or_else(|| PluginError::ReviewNotFound(review_id.to_string()))?;
        if now_ms() - pending.created_at > 10 * 60 * 1000 {
            let _ = std::fs::remove_dir_all(&pending.dir);
            return Err(PluginError::ReviewNotFound(review_id.to_string()));
        }
        for category in pending.manifest.permissions.keys() {
            if !approved.iter().any(|approved| approved == category) {
                return Err(PluginError::PermissionDenied(category.clone()));
            }
        }
        if pending.manifest.plugin.id.is_empty() {
            return Err(PluginError::InvalidManifest("empty id".to_string()));
        }

        let target = self.installed_dir(&pending.manifest.plugin.id);
        if target.exists() {
            if !updating {
                return Err(PluginError::AlreadyInstalled(
                    pending.manifest.plugin.id.clone(),
                ));
            }
            self.unload(&pending.manifest.plugin.id);
            self.move_to_trash(&target, &pending.manifest.plugin.id)?;
            self.delete_record(&pending.manifest.plugin.id, false)?;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        move_dir(&pending.dir, &target)?;

        self.insert_record(
            &pending.manifest,
            &target,
            &pending.digest,
            pending.signature,
            &pending.source,
            false,
        )?;
        self.insert_grants(&pending.manifest)?;

        let record = self.record(&pending.manifest.plugin.id)?;
        self.load_record(&record)?;
        self.emit(
            "plugin.installed",
            json!({
                "plugin_id": record.id,
                "version": record.version,
                "kind": record.kind,
                "signature": record.signature_status,
            }),
        );
        self.view(&record.id)
    }

    /// Remove a plugin, its grants and (optionally) its storage.
    pub fn uninstall(&self, id: &str, remove_storage: bool) -> Result<(), PluginError> {
        let record = self.record(id)?;
        self.unload(id);
        if let Ok(manifest) = read_manifest(Path::new(&record.path)) {
            self.unregister_caps(&manifest);
        }
        if !record.dev {
            self.move_to_trash(Path::new(&record.path), id)?;
        }
        self.delete_record(id, remove_storage)?;
        self.emit("plugin.uninstalled", json!({ "plugin_id": id }));
        Ok(())
    }

    /// Enable and load a plugin.
    pub fn enable(&self, id: &str) -> Result<PluginView, PluginError> {
        let mut record = self.record(id)?;
        record.enabled = true;
        record.degraded = false;
        self.set_enabled(id, true)?;
        if let Err(err) = self.load_record(&record) {
            self.set_enabled(id, false)?;
            return Err(err);
        }
        self.emit("plugin.enabled", json!({ "plugin_id": id }));
        self.view(id)
    }

    /// Unload and disable a plugin.
    pub fn disable(&self, id: &str) -> Result<PluginView, PluginError> {
        self.unload(id);
        if let Ok(manifest) = self.manifest_for(id) {
            self.unregister_caps(&manifest);
        }
        self.set_enabled(id, false)?;
        self.emit("plugin.disabled", json!({ "plugin_id": id }));
        self.view(id)
    }

    /// Reload a plugin (dev mode).
    pub fn reload(&self, id: &str) -> Result<PluginView, PluginError> {
        self.unload(id);
        let record = self.record(id)?;
        self.load_record(&record)?;
        self.emit("plugin.dev.reloaded", json!({ "plugin_id": id }));
        self.view(id)
    }

    /// Link a development directory (trusted, unsigned, hot-reloading).
    pub fn dev_link(self: &Arc<Self>, path: &Path) -> Result<PluginView, PluginError> {
        let path = path
            .canonicalize()
            .map_err(|err| PluginError::Unavailable(err.to_string()))?;
        let manifest = read_manifest(&path)?;
        manifest.validate_files(&path)?;
        if self.record(&manifest.plugin.id).is_ok() {
            self.unload(&manifest.plugin.id);
            self.delete_record(&manifest.plugin.id, false)?;
        }
        let digest = fergents_packages::compute_digest(&path)?;
        self.insert_record(&manifest, &path, &digest, SignatureStatus::Dev, "dev", true)?;
        self.insert_grants(&manifest)?;
        let record = self.record(&manifest.plugin.id)?;
        self.load_record(&record)?;
        self.watch_dev(path.clone(), manifest.plugin.id.clone());
        self.emit(
            "plugin.dev.linked",
            json!({ "plugin_id": manifest.plugin.id, "path": path }),
        );
        self.view(&manifest.plugin.id)
    }

    /// Unlink a development directory.
    pub fn dev_unlink(&self, id: &str) -> Result<(), PluginError> {
        let record = self.record(id)?;
        if !record.dev {
            return Err(PluginError::Unavailable(format!(
                "{id} is not a dev plugin"
            )));
        }
        self.unload(id);
        self.delete_record(id, false)?;
        self.emit("plugin.dev.unlinked", json!({ "plugin_id": id }));
        Ok(())
    }

    /// Every loaded plugin tool, with its owning plugin id.
    pub fn tool_specs(&self) -> Vec<(String, ToolSpec)> {
        self.loaded
            .read()
            .expect("plugins poisoned")
            .values()
            .flat_map(|loaded| {
                loaded
                    .tools
                    .iter()
                    .cloned()
                    .map(|tool| (loaded.record.id.clone(), tool))
            })
            .collect()
    }

    /// Call a plugin tool.
    pub fn call_tool(
        &self,
        plugin_id: &str,
        tool: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let loaded = self.loaded_record(plugin_id)?;
        if !loaded.tools.iter().any(|spec| spec.name == tool) {
            return Err(PluginError::Unavailable(format!(
                "{plugin_id} does not declare {tool}"
            )));
        }
        let component = loaded
            .component
            .as_ref()
            .ok_or_else(|| PluginError::Unavailable(format!("{plugin_id} has no kernel module")))?;
        let bridge = self.bridge(plugin_id)?;
        let started = std::time::Instant::now();
        let result =
            self.sandbox
                .call_tool(component, bridge, tool, &input.to_string(), &Limits::PLUGIN);
        match result {
            Ok(output) => {
                self.metric(
                    plugin_id,
                    tool,
                    "ok",
                    started.elapsed().as_millis() as i64,
                    None,
                );
                serde_json::from_str(&output).map_err(PluginError::Json)
            }
            Err(err) => {
                self.metric(
                    plugin_id,
                    tool,
                    "error",
                    started.elapsed().as_millis() as i64,
                    Some(err.code()),
                );
                self.emit(
                    "plugin.tool.failed",
                    json!({ "plugin_id": plugin_id, "tool": tool, "code": err.code(), "message": err.to_string() }),
                );
                Err(PluginError::Sandbox(err))
            }
        }
    }

    /// Run a plugin command.
    pub fn run_command(
        &self,
        plugin_id: &str,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let loaded = self.loaded_record(plugin_id)?;
        if !loaded
            .manifest
            .commands
            .iter()
            .any(|item| item.id == command)
        {
            return Err(PluginError::Unavailable(format!(
                "{plugin_id} does not declare command {command}"
            )));
        }
        let component = loaded
            .component
            .as_ref()
            .ok_or_else(|| PluginError::Unavailable(format!("{plugin_id} has no kernel module")))?;
        let bridge = self.bridge(plugin_id)?;
        let output = self.sandbox.on_command(
            component,
            bridge,
            command,
            &args.to_string(),
            &Limits::PLUGIN,
        )?;
        serde_json::from_str(&output).map_err(PluginError::Json)
    }

    /// Deliver an event to every subscribed plugin.
    pub fn deliver_event(&self, topic: &str, payload: &str) {
        let grants = self
            .loaded
            .read()
            .expect("plugins poisoned")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for plugin_id in grants {
            let Ok(scopes) = self.subscriptions(&plugin_id) else {
                continue;
            };
            if let Some(scopes) = scopes {
                if !matches_topic(&scopes, topic) {
                    continue;
                }
            }
            let Ok(loaded) = self.loaded_record(&plugin_id) else {
                continue;
            };
            let Some(component) = loaded.component.as_ref() else {
                continue;
            };
            let Ok(bridge) = self.bridge(&plugin_id) else {
                continue;
            };
            if let Err(err) =
                self.sandbox
                    .on_event(component, bridge, topic, payload, &Limits::PLUGIN)
            {
                tracing::warn!(plugin = %plugin_id, topic, error = %err, "plugin event failed");
            }
        }
    }

    /// Deliver bus events to plugins in the background.
    pub fn spawn_event_router(self: &Arc<Self>) -> Result<(), PluginError> {
        let mut subscription = self.bus.subscribe();
        let service = self.clone();
        tokio::spawn(async move {
            while let Ok(event) = subscription.recv().await {
                if event.kind.starts_with("plugin.") {
                    continue;
                }
                let payload =
                    serde_json::to_string(&event.payload).unwrap_or_else(|_| "null".into());
                let service = service.clone();
                let topic = event.kind.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    service.deliver_event(&topic, &payload);
                })
                .await;
            }
        });
        Ok(())
    }

    /// Number of loaded plugins.
    pub fn loaded_count(&self) -> usize {
        self.loaded.read().expect("plugins poisoned").len()
    }

    /// Recent call statistics.
    pub fn stats(&self, id: &str) -> Result<serde_json::Value, PluginError> {
        let rows = self.storage.query(
            "SELECT status, COUNT(*) AS count, AVG(duration_ms) AS avg_ms \
             FROM plugin_calls WHERE plugin_id = ?1 GROUP BY status",
            &[Value::String(id.to_string())],
        )?;
        Ok(json!({
            "calls": rows.iter().map(|row| json!({
                "status": row.get("status").and_then(Value::as_str),
                "count": row.get("count").and_then(Value::as_i64),
                "avg_ms": row.get("avg_ms").and_then(Value::as_f64),
            })).collect::<Vec<_>>()
        }))
    }

    // ---- internals -----------------------------------------------------

    fn records(&self) -> Result<Vec<PluginRecord>, PluginError> {
        let rows = self
            .storage
            .query("SELECT * FROM plugins ORDER BY name", &[])?;
        Ok(rows.iter().filter_map(PluginRecord::from_row).collect())
    }

    fn record(&self, id: &str) -> Result<PluginRecord, PluginError> {
        let rows = self.storage.query(
            "SELECT * FROM plugins WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        rows.first()
            .and_then(PluginRecord::from_row)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))
    }

    fn manifest_for(&self, id: &str) -> Result<PluginManifest, PluginError> {
        let record = self.record(id)?;
        read_manifest(Path::new(&record.path))
    }

    fn loaded_record(&self, id: &str) -> Result<Loaded, PluginError> {
        let loaded = self.loaded.read().expect("plugins poisoned");
        let entry = loaded
            .get(id)
            .ok_or_else(|| PluginError::Unavailable(id.to_string()))?;
        Ok(Loaded {
            record: entry.record.clone(),
            manifest: entry.manifest.clone(),
            component: entry.component.clone(),
            tools: entry.tools.clone(),
            error: entry.error.clone(),
        })
    }

    fn view_record(&self, record: PluginRecord) -> Result<PluginView, PluginError> {
        let manifest = read_manifest(Path::new(&record.path)).ok();
        let grants = self.grants(&record.id)?;
        let (tools, error) = {
            let loaded = self.loaded.read().expect("plugins poisoned");
            loaded
                .get(&record.id)
                .map(|entry| (entry.tools.clone(), entry.error.clone()))
                .unwrap_or_default()
        };
        Ok(PluginView {
            permissions: grant_view(&grants, manifest.as_ref()),
            tools,
            slots: manifest
                .as_ref()
                .map(|manifest| manifest.slots.clone())
                .unwrap_or_default(),
            commands: manifest
                .as_ref()
                .map(|manifest| manifest.commands.clone())
                .unwrap_or_default(),
            capabilities: manifest
                .as_ref()
                .map(|manifest| manifest.capabilities.clone())
                .unwrap_or_default(),
            error,
            record,
        })
    }

    fn view(&self, id: &str) -> Result<PluginView, PluginError> {
        let record = self.record(id)?;
        self.view_record(record)
    }

    fn installed_dir(&self, id: &str) -> PathBuf {
        self.data_dir.join("plugins").join("installed").join(id)
    }

    fn trash_dir(&self, id: &str) -> PathBuf {
        self.data_dir
            .join("trash")
            .join("plugins")
            .join(format!("{id}-{}", ulid::Ulid::new()))
    }

    fn signature_status(&self, dir: &Path) -> Result<SignatureStatus, PluginError> {
        let path = dir.join("signature.ed25519");
        if !path.is_file() {
            return Ok(SignatureStatus::None);
        }
        let text = std::fs::read_to_string(&path)?;
        let signature: SignatureFile = match serde_json::from_str(&text) {
            Ok(signature) => signature,
            Err(_) => return Ok(SignatureStatus::Invalid),
        };
        let key_path = self
            .data_dir
            .join("trust")
            .join("keys")
            .join(format!("{}.pub", signature.key_id));
        let Ok(public_key) = std::fs::read_to_string(&key_path) else {
            return Ok(SignatureStatus::UnknownKey);
        };
        match verify_signature(dir, public_key.trim(), &signature.signature) {
            Ok(()) => Ok(SignatureStatus::Verified),
            Err(_) => Ok(SignatureStatus::Invalid),
        }
    }

    fn insert_record(
        &self,
        manifest: &PluginManifest,
        path: &Path,
        manifest_hash: &str,
        signature: SignatureStatus,
        source: &str,
        dev: bool,
    ) -> Result<(), PluginError> {
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO plugins \
             (id, name, version, api_version, kind, source, path, manifest_hash, \
              signature_status, enabled, dev, degraded, config, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, NULL, ?12, ?12)",
            &[
                Value::String(manifest.plugin.id.clone()),
                Value::String(manifest.plugin.name.clone()),
                Value::String(manifest.plugin.version.clone()),
                Value::from(i64::from(manifest.plugin.api_version)),
                Value::String(manifest.kind().to_string()),
                Value::String(source.to_string()),
                Value::String(path.to_string_lossy().to_string()),
                Value::String(manifest_hash.to_string()),
                Value::String(signature.as_str().to_string()),
                Value::from(1i64),
                Value::from(i64::from(dev)),
                Value::from(now),
            ],
        )?;
        Ok(())
    }

    fn insert_grants(&self, manifest: &PluginManifest) -> Result<(), PluginError> {
        for (capability, scope) in &manifest.permissions {
            self.storage.execute(
                "INSERT INTO plugin_permissions (plugin_id, category, scope, granted, granted_at) \
                 VALUES (?1, ?2, ?3, 1, ?4) \
                 ON CONFLICT(plugin_id, category) DO UPDATE SET scope = ?3, granted = 1",
                &[
                    Value::String(manifest.plugin.id.clone()),
                    Value::String(capability.clone()),
                    match scope_to_json(scope) {
                        Some(value) => Value::String(value.to_string()),
                        None => Value::Null,
                    },
                    Value::from(now_ms()),
                ],
            )?;
        }
        Ok(())
    }

    fn grants(&self, id: &str) -> Result<Grants, PluginError> {
        let rows = self.storage.query(
            "SELECT category, scope, granted FROM plugin_permissions WHERE plugin_id = ?1",
            &[Value::String(id.to_string())],
        )?;
        let mut grants = Grants::new();
        for row in rows {
            let Some(category) = row.get("category").and_then(Value::as_str) else {
                continue;
            };
            if row.get("granted").and_then(Value::as_i64).unwrap_or(0) == 0 {
                continue;
            }
            let scope = row
                .get("scope")
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str::<Vec<String>>(text).ok());
            grants.insert(category.to_string(), scope);
        }
        Ok(grants)
    }

    fn subscriptions(&self, id: &str) -> Result<Option<Vec<String>>, PluginError> {
        let mut grants = self.grants(id)?;
        Ok(grants.remove("events.subscribe").flatten())
    }

    fn bridge(&self, id: &str) -> Result<Arc<PluginHostBridge>, PluginError> {
        Ok(Arc::new(PluginHostBridge::new(
            id,
            self.grants(id)?,
            self.storage.clone(),
            self.bus.clone(),
            self.host.clone(),
            self.policy.clone(),
        )))
    }

    fn load_record(&self, record: &PluginRecord) -> Result<(), PluginError> {
        let root = Path::new(&record.path);
        let manifest = read_manifest(root).map_err(|err| self.fail(record, err))?;
        if let Err(err) = manifest.validate_files(root) {
            return Err(self.fail(record, err));
        }
        let mut component = None;
        let mut tools = Vec::new();
        if let Some(kernel) = &manifest.kernel {
            let bytes = std::fs::read(root.join(&kernel.module))?;
            let handle = self.sandbox.load(&bytes)?;
            let bridge = self.bridge(&record.id)?;
            self.sandbox
                .init_plugin(&handle, bridge.clone(), &Limits::PLUGIN)
                .map_err(|err| self.fail(record, PluginError::Sandbox(err)))?;
            let listed = self
                .sandbox
                .list_tools(&handle, bridge, &Limits::PLUGIN)
                .map_err(|err| self.fail(record, PluginError::Sandbox(err)))?;
            tools = parse_tools(&listed)?;
            {
                let loaded = self.loaded.read().expect("plugins poisoned");
                for (plugin_id, existing) in loaded
                    .iter()
                    .flat_map(|(id, entry)| entry.tools.iter().map(move |tool| (id.clone(), tool)))
                {
                    if plugin_id != record.id && tools.iter().any(|tool| tool.name == existing.name)
                    {
                        return Err(
                            self.fail(record, PluginError::ToolConflict(existing.name.clone()))
                        );
                    }
                }
            }
            component = Some(handle);
        }
        self.register_caps(&manifest);
        let tool_count = tools.len();
        let loaded = Loaded {
            record: record.clone(),
            manifest: manifest.clone(),
            component,
            tools,
            error: None,
        };
        self.loaded
            .write()
            .expect("plugins poisoned")
            .insert(record.id.clone(), loaded);
        self.set_degraded(&record.id, false)?;
        self.emit(
            "plugin.loaded",
            json!({
                "plugin_id": record.id,
                "kind": record.kind,
                "tools": tool_count,
            }),
        );
        Ok(())
    }

    fn fail(&self, record: &PluginRecord, err: PluginError) -> PluginError {
        let _ = self.set_degraded(&record.id, true);
        self.emit(
            "plugin.load_failed",
            json!({
                "plugin_id": record.id,
                "code": err.code(),
                "message": err.to_string(),
            }),
        );
        err
    }

    fn unload(&self, id: &str) {
        let entry = self.loaded.write().expect("plugins poisoned").remove(id);
        if let Some(entry) = entry {
            if let (Some(component), Ok(bridge)) = (entry.component.as_ref(), self.bridge(id)) {
                let _ = self
                    .sandbox
                    .shutdown_plugin(component, bridge, &Limits::PLUGIN);
            }
            self.emit("plugin.shutdown", json!({ "plugin_id": id }));
        }
    }

    fn register_caps(&self, manifest: &PluginManifest) {
        for declared in &manifest.capabilities {
            let capability = Capability {
                id: declared.id.clone(),
                name: declared.name.clone(),
                version: manifest.plugin.version.clone(),
                source: format!("plugin:{}", manifest.plugin.id),
                description: declared.description.clone(),
                dependencies: Vec::new(),
                resources: Vec::new(),
                default_enabled: false,
                hot_toggle: true,
                config: None,
            };
            match self.capabilities.register(capability) {
                Ok(_) | Err(CapabilityError::DuplicateId(_)) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "failed to register plugin capability");
                }
            }
        }
    }

    fn unregister_caps(&self, manifest: &PluginManifest) {
        for declared in &manifest.capabilities {
            let _ = self.capabilities.unregister(&declared.id);
        }
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PluginError> {
        self.storage.execute(
            "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::from(i64::from(enabled)),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    fn set_degraded(&self, id: &str, degraded: bool) -> Result<(), PluginError> {
        self.storage.execute(
            "UPDATE plugins SET degraded = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::from(i64::from(degraded)),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    fn delete_record(&self, id: &str, remove_storage: bool) -> Result<(), PluginError> {
        let params = [Value::String(id.to_string())];
        self.storage
            .execute("DELETE FROM plugins WHERE id = ?1", &params)?;
        self.storage.execute(
            "DELETE FROM plugin_permissions WHERE plugin_id = ?1",
            &params,
        )?;
        if remove_storage {
            self.storage
                .execute("DELETE FROM plugin_storage WHERE plugin_id = ?1", &params)?;
        }
        Ok(())
    }

    fn move_to_trash(&self, path: &Path, id: &str) -> Result<(), PluginError> {
        let trash = self.trash_dir(id);
        if let Some(parent) = trash.parent() {
            std::fs::create_dir_all(parent)?;
        }
        move_dir(path, &trash)
    }

    fn metric(&self, plugin_id: &str, tool: &str, status: &str, ms: i64, error: Option<&str>) {
        let _ = self.storage.execute(
            "INSERT INTO plugin_calls (id, plugin_id, tool, status, duration_ms, error_code, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(plugin_id.to_string()),
                Value::String(tool.to_string()),
                Value::String(status.to_string()),
                Value::from(ms),
                error.map_or(Value::Null, |code| Value::String(code.to_string())),
                Value::from(now_ms()),
            ],
        );
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Err(err) = self
            .bus
            .publish(kind.to_string(), "plugins".to_string(), payload)
        {
            tracing::warn!(error = %err, kind, "failed to publish plugin event");
        }
    }

    fn watch_dev(self: &Arc<Self>, path: PathBuf, id: String) {
        let weak = Arc::downgrade(self);
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if result.is_err() {
                return;
            }
            let Some(service) = weak.upgrade() else {
                return;
            };
            std::thread::sleep(Duration::from_millis(300));
            if let Err(err) = service.reload(&id) {
                tracing::warn!(plugin = %id, error = %err, "dev reload failed");
            }
        });
        if let Ok(mut watcher) = watcher {
            if watcher.watch(&path, RecursiveMode::Recursive).is_ok() {
                self.watchers
                    .lock()
                    .expect("plugin watchers poisoned")
                    .push(watcher);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct SignatureFile {
    key_id: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct RawTool {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parameters: serde_json::Value,
    #[serde(default)]
    schema: serde_json::Value,
}

fn parse_tools(json_text: &str) -> Result<Vec<ToolSpec>, PluginError> {
    let raw: Vec<RawTool> = serde_json::from_str(json_text)
        .map_err(|err| PluginError::InvalidManifest(err.to_string()))?;
    let mut names = std::collections::HashSet::new();
    raw.into_iter()
        .map(|tool| {
            if tool.name.is_empty() || !names.insert(tool.name.clone()) {
                return Err(PluginError::InvalidManifest(format!(
                    "duplicate or empty tool name: {}",
                    tool.name
                )));
            }
            let parameters = if tool.parameters.is_null() {
                if tool.schema.is_null() {
                    json!({ "type": "object" })
                } else {
                    tool.schema
                }
            } else {
                tool.parameters
            };
            Ok(ToolSpec {
                name: tool.name,
                description: tool.description,
                parameters,
            })
        })
        .collect()
}

fn read_manifest(root: &Path) -> Result<PluginManifest, PluginError> {
    let text = std::fs::read_to_string(root.join("plugin.toml"))?;
    PluginManifest::parse(&text)
}

fn scope_to_json(scope: &toml::Value) -> Option<serde_json::Value> {
    match scope {
        toml::Value::Boolean(true) => None,
        toml::Value::Boolean(false) => Some(json!([])),
        toml::Value::Array(items) => Some(json!(items
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>())),
        _ => None,
    }
}

fn grant_view(grants: &Grants, manifest: Option<&PluginManifest>) -> Vec<PermissionGrant> {
    let mut out = Vec::new();
    if let Some(manifest) = manifest {
        for (category, scope) in &manifest.permissions {
            out.push(PermissionGrant {
                category: category.clone(),
                scope: scope_to_json(scope),
                granted: grants.contains_key(category),
            });
        }
    }
    out
}

fn matches_topic(scopes: &[String], topic: &str) -> bool {
    scopes.iter().any(|scope| {
        globset::Glob::new(scope)
            .map(|glob| glob.compile_matcher().is_match(topic))
            .unwrap_or(false)
    })
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), PluginError> {
    std::fs::create_dir_all(target)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|err| PluginError::Io(std::io::Error::other(err)))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| PluginError::InvalidManifest("bad path".to_string()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let destination = target.join(relative);
        if entry.file_type().is_symlink() {
            return Err(PluginError::InvalidManifest(format!(
                "symlink not allowed: {}",
                relative.display()
            )));
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn move_dir(source: &Path, target: &Path) -> Result<(), PluginError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    copy_dir(source, target)?;
    std::fs::remove_dir_all(source)?;
    Ok(())
}

/// Build a `.fergplugin` archive from a directory.
pub fn package_plugin(source: &Path, out: &Path) -> Result<(), PluginError> {
    create_archive(source, out, true).map_err(PluginError::from)
}
