//! The skill registry and execution pipeline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use fergents_events::{now_ms, EventBus};
use fergents_packages::{create_archive, extract_archive, ArchiveLimits};
use fergents_storage::{Storage, Value};
use fergents_wasm::{ComponentHandle, HostBridge, HostError, Limits, Sandbox};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::SkillError;
use crate::manifest::{self, ParsedSkill, SkillKind, SkillManifest};
use crate::schema;
use crate::MAX_DEPTH;

/// Skill execution services provided by the kernel.
///
/// Implemented by `fergents-core`; tests use [`UnavailableSkillServices`].
pub trait SkillServices: Send + Sync {
    /// The kernel data directory.
    fn data_dir(&self) -> &Path;

    /// Call a model. An empty `model` means "the chat route".
    fn llm_chat(
        &self,
        model: &str,
        prompt: &str,
        input: &serde_json::Value,
        temperature: f64,
        max_tokens: u32,
    ) -> Result<String, String>;

    /// Call a kernel or MCP tool.
    fn tool_call(&self, tool: &str, input: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// A services implementation that reaches nothing.
pub struct UnavailableSkillServices {
    data_dir: PathBuf,
}

impl UnavailableSkillServices {
    /// Build it for `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

impl SkillServices for UnavailableSkillServices {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn llm_chat(
        &self,
        _model: &str,
        _prompt: &str,
        _input: &serde_json::Value,
        _temperature: f64,
        _max_tokens: u32,
    ) -> Result<String, String> {
        Err("llm is unavailable".to_string())
    }

    fn tool_call(
        &self,
        tool: &str,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err(format!("tool is unavailable: {tool}"))
    }
}

/// A row in the `skills` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRecord {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semver.
    pub version: String,
    /// `prompt`, `tool`, `code`, or `hybrid`.
    pub kind: String,
    /// `local`, `git`, `marketplace`, `ai`, or `builtin`.
    pub source: String,
    /// Absolute folder.
    pub path: String,
    /// Digest of the folder.
    pub manifest_hash: String,
    /// Whether the skill is loaded.
    pub enabled: bool,
    /// Installed-at timestamp.
    pub installed_at: i64,
    /// Updated-at timestamp.
    pub updated_at: i64,
}

impl SkillRecord {
    fn from_row(row: &fergents_storage::Row) -> Option<Self> {
        Some(Self {
            id: row.get("id").and_then(Value::as_str)?.to_string(),
            name: row.get("name").and_then(Value::as_str)?.to_string(),
            version: row.get("version").and_then(Value::as_str)?.to_string(),
            kind: row.get("kind").and_then(Value::as_str)?.to_string(),
            source: row.get("source").and_then(Value::as_str)?.to_string(),
            path: row.get("path").and_then(Value::as_str)?.to_string(),
            manifest_hash: row
                .get("manifest_hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            enabled: row.get("enabled").and_then(Value::as_i64).unwrap_or(0) != 0,
            installed_at: row.get("installed_at").and_then(Value::as_i64).unwrap_or(0),
            updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        })
    }
}

/// The API view of a skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillView {
    /// The persisted record.
    #[serde(flatten)]
    pub record: SkillRecord,
    /// Description.
    pub description: Option<String>,
    /// Input schema.
    pub input: serde_json::Value,
    /// Output schema.
    pub output: serde_json::Value,
    /// Declared permissions.
    pub permissions: Vec<String>,
    /// Assigned tool name (`skill.<id>`).
    pub tool: String,
}

/// The model-facing tool for an attached skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillTool {
    /// `skill.<id>`.
    pub name: String,
    /// Description.
    pub description: String,
    /// Input schema.
    pub parameters: serde_json::Value,
    /// The skill id.
    pub skill_id: String,
}

/// Where a skill install comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// A `.fergskill` archive.
    File(PathBuf),
    /// A folder containing `SKILL.md`.
    Dir(PathBuf),
}

impl SkillSource {
    /// Parse from command params.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SkillError> {
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("dir");
        let path = value
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| SkillError::Invalid("missing path".to_string()))?;
        match kind {
            "file" => Ok(SkillSource::File(path)),
            "dir" => Ok(SkillSource::Dir(path)),
            other => Err(SkillError::Invalid(format!("unsupported source: {other}"))),
        }
    }
}

struct LoadedSkill {
    record: SkillRecord,
    manifest: SkillManifest,
    prompt: String,
    component: Option<ComponentHandle>,
}

/// An install review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillReview {
    /// Parsed manifest.
    pub manifest: SkillManifest,
    /// Digest.
    pub digest: String,
    /// Declared permissions.
    pub permissions: Vec<String>,
    /// Conflicts.
    pub conflicts: Vec<String>,
}

/// The skill service.
pub struct SkillService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    data_dir: PathBuf,
    sandbox: Arc<Sandbox>,
    services: Arc<dyn SkillServices>,
    loaded: RwLock<HashMap<String, LoadedSkill>>,
}

impl SkillService {
    /// Build the service.
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        data_dir: PathBuf,
        sandbox: Arc<Sandbox>,
        services: Arc<dyn SkillServices>,
    ) -> Self {
        Self {
            storage,
            bus,
            data_dir,
            sandbox,
            services,
            loaded: RwLock::new(HashMap::new()),
        }
    }

    /// Load every enabled skill; return the number loaded.
    pub fn start(&self) -> Result<usize, SkillError> {
        let mut count = 0;
        for record in self.records()? {
            if record.enabled && self.load_record(&record).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// All installed skills.
    pub fn list(&self) -> Result<Vec<SkillView>, SkillError> {
        self.records()?
            .into_iter()
            .map(|record| self.view_record(record))
            .collect()
    }

    /// One skill.
    pub fn get(&self, id: &str) -> Result<SkillView, SkillError> {
        self.view_record(self.record(id)?)
    }

    /// Inspect a folder before installing.
    pub fn inspect(&self, source: &SkillSource) -> Result<SkillReview, SkillError> {
        let work = self
            .data_dir
            .join("cache")
            .join("skill-review")
            .join(ulid::Ulid::new().to_string());
        std::fs::create_dir_all(&work)?;
        match source {
            SkillSource::Dir(path) => copy_dir(path, &work)?,
            SkillSource::File(path) => {
                let bytes = std::fs::read(path)?;
                extract_archive(&bytes, &work, &ArchiveLimits::default())?;
            }
        }
        let parsed = read_skill(&work)?;
        let digest = fergents_packages::compute_digest(&work)?;
        let mut conflicts = Vec::new();
        if self.record(&parsed.manifest.id).is_ok() {
            conflicts.push(format!("skill {} is already installed", parsed.manifest.id));
        }
        let review = SkillReview {
            permissions: parsed.manifest.permissions.clone(),
            manifest: parsed.manifest,
            digest,
            conflicts,
        };
        let _ = std::fs::remove_dir_all(&work);
        Ok(review)
    }

    /// Install a skill folder.
    ///
    /// `approve_permissions` grants every declared permission and enables the
    /// skill. When false, a skill that declares permissions is installed
    /// disabled (spec D17).
    pub fn install(
        &self,
        source: &SkillSource,
        approve_permissions: bool,
    ) -> Result<SkillView, SkillError> {
        let work = self
            .data_dir
            .join("cache")
            .join("skill-install")
            .join(ulid::Ulid::new().to_string());
        std::fs::create_dir_all(&work)?;
        match source {
            SkillSource::Dir(path) => copy_dir(path, &work)?,
            SkillSource::File(path) => {
                let bytes = std::fs::read(path)?;
                extract_archive(&bytes, &work, &ArchiveLimits::default())?;
            }
        }
        let parsed = read_skill(&work)?;
        if self.record(&parsed.manifest.id).is_ok() {
            let _ = std::fs::remove_dir_all(&work);
            return Err(SkillError::Invalid(format!(
                "skill {} is already installed",
                parsed.manifest.id
            )));
        }
        let digest = fergents_packages::compute_digest(&work)?;
        let target = self.skill_dir(&parsed.manifest.id);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        move_dir(&work, &target)?;
        let enabled = parsed.manifest.permissions.is_empty() || approve_permissions;
        self.insert_record(&parsed.manifest, &target, &digest, "local", enabled)?;
        if approve_permissions {
            self.insert_grants(&parsed.manifest)?;
        }
        if enabled {
            let record = self.record(&parsed.manifest.id)?;
            self.load_record(&record)?;
        }
        self.emit(
            "skill.installed",
            json!({ "skill_id": parsed.manifest.id, "kind": parsed.manifest.kind.as_str(), "enabled": enabled }),
        );
        self.get(&parsed.manifest.id)
    }

    /// Enable and load a skill.
    pub fn enable(&self, id: &str) -> Result<SkillView, SkillError> {
        self.set_enabled(id, true)?;
        let record = self.record(id)?;
        if let Err(err) = self.load_record(&record) {
            self.set_enabled(id, false)?;
            return Err(err);
        }
        self.emit("skill.enabled", json!({ "skill_id": id }));
        self.get(id)
    }

    /// Unload and disable a skill.
    pub fn disable(&self, id: &str) -> Result<SkillView, SkillError> {
        self.unload(id);
        self.set_enabled(id, false)?;
        self.emit("skill.disabled", json!({ "skill_id": id }));
        self.get(id)
    }

    /// Remove a skill, its grants, and (optionally) its storage.
    pub fn delete(&self, id: &str, remove_storage: bool) -> Result<(), SkillError> {
        let record = self.record(id)?;
        self.unload(id);
        let trash = self
            .data_dir
            .join("trash")
            .join("skills")
            .join(format!("{id}-{}", ulid::Ulid::new()));
        if let Some(parent) = trash.parent() {
            std::fs::create_dir_all(parent)?;
        }
        move_dir(Path::new(&record.path), &trash)?;
        let params = [Value::String(id.to_string())];
        self.storage
            .execute("DELETE FROM skills WHERE id = ?1", &params)?;
        self.storage
            .execute("DELETE FROM skill_permissions WHERE skill_id = ?1", &params)?;
        if remove_storage {
            self.storage
                .execute("DELETE FROM skill_storage WHERE skill_id = ?1", &params)?;
        }
        self.storage
            .execute("DELETE FROM skill_cache WHERE skill_id = ?1", &params)?;
        self.emit("skill.removed", json!({ "skill_id": id }));
        Ok(())
    }

    /// Duplicate a skill under a new id (installed disabled).
    pub fn duplicate(&self, id: &str, new_id: &str) -> Result<SkillView, SkillError> {
        let record = self.record(id)?;
        let work = self
            .data_dir
            .join("cache")
            .join("skill-duplicate")
            .join(ulid::Ulid::new().to_string());
        copy_dir(Path::new(&record.path), &work)?;
        let text = std::fs::read_to_string(work.join("SKILL.md"))?;
        let updated = text.replacen(&format!("id: {}", record.id), &format!("id: {new_id}"), 1);
        std::fs::write(work.join("SKILL.md"), updated)?;
        let source = SkillSource::Dir(work.clone());
        let view = self.install(&source, false);
        let _ = std::fs::remove_dir_all(&work);
        view
    }

    /// Execute a skill.
    pub fn invoke(
        self: &Arc<Self>,
        id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, SkillError> {
        self.invoke_at(id, input, 0, &[], None)
    }

    /// Test a skill with a sample input.
    pub fn test(
        self: &Arc<Self>,
        id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, SkillError> {
        self.invoke(id, input)
    }

    /// The tools contributed by enabled skills (for agent definitions).
    pub fn attached_tools(&self, ids: &[String]) -> Vec<SkillTool> {
        let loaded = self.loaded.read().expect("skills poisoned");
        ids.iter()
            .filter_map(|id| loaded.get(id))
            .filter(|skill| skill.record.enabled)
            .map(|skill| SkillTool {
                name: format!("skill.{}", skill.record.id),
                description: skill
                    .manifest
                    .description
                    .clone()
                    .unwrap_or_else(|| skill.manifest.name.clone()),
                parameters: skill.manifest.input_schema(),
                skill_id: skill.record.id.clone(),
            })
            .collect()
    }

    /// Recent invocation statistics.
    pub fn stats(&self, id: &str) -> Result<serde_json::Value, SkillError> {
        let rows = self.storage.query(
            "SELECT status, COUNT(*) AS count, AVG(duration_ms) AS avg_ms \
             FROM skill_invocations WHERE skill_id = ?1 GROUP BY status",
            &[Value::String(id.to_string())],
        )?;
        Ok(json!({
            "invocations": rows.iter().map(|row| json!({
                "status": row.get("status").and_then(Value::as_str),
                "count": row.get("count").and_then(Value::as_i64),
                "avg_ms": row.get("avg_ms").and_then(Value::as_f64),
            })).collect::<Vec<_>>()
        }))
    }

    // ---- execution -----------------------------------------------------

    fn invoke_at(
        self: &Arc<Self>,
        id: &str,
        input: serde_json::Value,
        depth: usize,
        stack: &[String],
        parent: Option<&str>,
    ) -> Result<serde_json::Value, SkillError> {
        if depth > MAX_DEPTH {
            return Err(SkillError::Depth(format!(
                "maximum depth {MAX_DEPTH} exceeded"
            )));
        }
        if stack.iter().any(|entry| entry == id) {
            return Err(SkillError::Depth(format!("cycle detected at {id}")));
        }
        let loaded = self.loaded_skill(id)?;
        if !loaded.record.enabled {
            return Err(SkillError::Disabled(id.to_string()));
        }
        let started = Instant::now();
        let result = self.execute(&loaded, input, depth, stack, parent);
        let status = if result.is_ok() { "ok" } else { "error" };
        self.metric(
            id,
            parent,
            depth,
            status,
            started.elapsed().as_millis() as i64,
        );
        match &result {
            Ok(_) => self.emit(
                "skill.completed",
                json!({ "skill_id": id, "depth": depth, "duration_ms": started.elapsed().as_millis() as i64 }),
            ),
            Err(err) => self.emit(
                "skill.failed",
                json!({ "skill_id": id, "code": err.code(), "message": err.to_string() }),
            ),
        }
        result
    }

    fn execute(
        self: &Arc<Self>,
        loaded: &LoadedSkill,
        input: serde_json::Value,
        depth: usize,
        stack: &[String],
        parent: Option<&str>,
    ) -> Result<serde_json::Value, SkillError> {
        schema::validate(&loaded.manifest.input_schema(), &input)
            .map_err(SkillError::InvalidInput)?;
        let output = match loaded.manifest.kind {
            SkillKind::Prompt => self.run_prompt(loaded, input, depth, stack),
            SkillKind::Tool => self.run_tool(&loaded.manifest, input, depth, stack),
            SkillKind::Code => self.run_code(loaded, input, depth, stack),
            SkillKind::Hybrid => self.run_hybrid(loaded, input, depth, stack),
        }?;
        schema::validate(&loaded.manifest.output_schema(), &output)
            .map_err(SkillError::InvalidOutput)?;
        let _ = parent;
        Ok(output)
    }

    fn run_prompt(
        self: &Arc<Self>,
        loaded: &LoadedSkill,
        input: serde_json::Value,
        depth: usize,
        _stack: &[String],
    ) -> Result<serde_json::Value, SkillError> {
        let model = loaded.manifest.model.clone().unwrap_or_default();
        let key = cache_key(&loaded.manifest, &input, &model);
        if loaded.manifest.cache_enabled() {
            if let Some(cached) = self.cache_get(&loaded.record.id, &key)? {
                self.emit("skill.cache.hit", json!({ "skill_id": loaded.record.id }));
                return Ok(cached);
            }
        }
        let text = self
            .services
            .llm_chat(
                &model,
                &loaded.prompt,
                &input,
                loaded.manifest.temperature,
                loaded.manifest.max_tokens,
            )
            .map_err(SkillError::Failed)?;
        let parsed = match parse_json(&text) {
            Some(value) => value,
            None => {
                let repaired = self
                    .services
                    .llm_chat(
                        &model,
                        &format!(
                            "{}\n\nReturn ONLY a single JSON value that matches this schema:\n{}",
                            loaded.prompt,
                            loaded.manifest.output_schema()
                        ),
                        &input,
                        loaded.manifest.temperature,
                        loaded.manifest.max_tokens,
                    )
                    .map_err(SkillError::Failed)?;
                parse_json(&repaired).ok_or_else(|| {
                    SkillError::InvalidOutput(format!(
                        "model did not return JSON: {}",
                        truncate(&repaired)
                    ))
                })?
            }
        };
        if loaded.manifest.cache_enabled() {
            let expires = now_ms() + (loaded.manifest.cache_ttl_s() as i64) * 1000;
            self.cache_put(&loaded.record.id, &key, &parsed, expires)?;
        }
        let _ = depth;
        Ok(parsed)
    }

    fn run_tool(
        self: &Arc<Self>,
        manifest: &SkillManifest,
        input: serde_json::Value,
        _depth: usize,
        _stack: &[String],
    ) -> Result<serde_json::Value, SkillError> {
        let wraps = loaded_tool(&manifest.wraps)?;
        self.services
            .tool_call(&wraps, input)
            .map_err(SkillError::Failed)
    }

    fn run_code(
        self: &Arc<Self>,
        loaded: &LoadedSkill,
        input: serde_json::Value,
        depth: usize,
        stack: &[String],
    ) -> Result<serde_json::Value, SkillError> {
        let component = loaded
            .component
            .as_ref()
            .ok_or_else(|| SkillError::Invalid("code skill has no component".to_string()))?;
        let bridge = Arc::new(SkillHost::new(
            self.clone(),
            loaded.record.id.clone(),
            loaded.manifest.permissions.clone(),
            depth,
            stack.to_vec(),
            self.storage.clone(),
        ));
        let output =
            self.sandbox
                .invoke_skill(component, bridge, &input.to_string(), &Limits::SKILL)?;
        serde_json::from_str(&output).map_err(SkillError::Json)
    }

    fn run_hybrid(
        self: &Arc<Self>,
        loaded: &LoadedSkill,
        input: serde_json::Value,
        depth: usize,
        stack: &[String],
    ) -> Result<serde_json::Value, SkillError> {
        let mut current = input;
        for step in loaded.manifest.steps() {
            let next = match step.as_str() {
                "code" => self.run_code(loaded, current.clone(), depth, stack)?,
                "prompt" => self.run_prompt(loaded, current.clone(), depth, stack)?,
                other => return Err(SkillError::Invalid(format!("unknown hybrid step: {other}"))),
            };
            current = merge(current, next);
        }
        Ok(current)
    }

    // ---- registry ------------------------------------------------------

    fn records(&self) -> Result<Vec<SkillRecord>, SkillError> {
        let rows = self
            .storage
            .query("SELECT * FROM skills ORDER BY name", &[])?;
        Ok(rows.iter().filter_map(SkillRecord::from_row).collect())
    }

    fn record(&self, id: &str) -> Result<SkillRecord, SkillError> {
        let rows = self.storage.query(
            "SELECT * FROM skills WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        rows.first()
            .and_then(SkillRecord::from_row)
            .ok_or_else(|| SkillError::NotFound(id.to_string()))
    }

    fn loaded_skill(&self, id: &str) -> Result<LoadedSkill, SkillError> {
        let loaded = self.loaded.read().expect("skills poisoned");
        let skill = loaded
            .get(id)
            .ok_or_else(|| SkillError::NotFound(id.to_string()))?;
        Ok(LoadedSkill {
            record: skill.record.clone(),
            manifest: skill.manifest.clone(),
            prompt: skill.prompt.clone(),
            component: skill.component.clone(),
        })
    }

    fn view_record(&self, record: SkillRecord) -> Result<SkillView, SkillError> {
        let parsed = read_skill(Path::new(&record.path)).ok();
        let (description, input, output, permissions) = parsed.map_or_else(
            || {
                (
                    None,
                    json!({"type": "object", "additionalProperties": false}),
                    json!({"type": "object", "additionalProperties": false}),
                    Vec::new(),
                )
            },
            |parsed| {
                (
                    parsed.manifest.description.clone(),
                    parsed.manifest.input_schema(),
                    parsed.manifest.output_schema(),
                    parsed.manifest.permissions.clone(),
                )
            },
        );
        Ok(SkillView {
            tool: format!("skill.{}", record.id),
            record,
            description,
            input,
            output,
            permissions,
        })
    }

    fn skill_dir(&self, id: &str) -> PathBuf {
        self.data_dir.join("skills").join(id)
    }

    fn insert_record(
        &self,
        manifest: &SkillManifest,
        path: &Path,
        hash: &str,
        source: &str,
        enabled: bool,
    ) -> Result<(), SkillError> {
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO skills \
             (id, name, version, kind, source, path, manifest_hash, enabled, config, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?9)",
            &[
                Value::String(manifest.id.clone()),
                Value::String(manifest.name.clone()),
                Value::String(manifest.version.clone()),
                Value::String(manifest.kind.as_str().to_string()),
                Value::String(source.to_string()),
                Value::String(path.to_string_lossy().to_string()),
                Value::String(hash.to_string()),
                Value::from(i64::from(enabled)),
                Value::from(now),
            ],
        )?;
        Ok(())
    }

    fn insert_grants(&self, manifest: &SkillManifest) -> Result<(), SkillError> {
        for permission in &manifest.permissions {
            self.storage.execute(
                "INSERT INTO skill_permissions (skill_id, category, scope, granted) \
                 VALUES (?1, ?2, NULL, 1) ON CONFLICT(skill_id, category) DO UPDATE SET granted = 1",
                &[
                    Value::String(manifest.id.clone()),
                    Value::String(permission.clone()),
                ],
            )?;
        }
        Ok(())
    }

    fn load_record(&self, record: &SkillRecord) -> Result<(), SkillError> {
        let parsed = read_skill(Path::new(&record.path))?;
        let mut component = None;
        if matches!(parsed.manifest.kind, SkillKind::Code | SkillKind::Hybrid) {
            if let Some(entry) = &parsed.manifest.entry {
                let path = Path::new(&record.path).join(entry);
                let bytes =
                    std::fs::read(&path).map_err(|err| SkillError::Invalid(err.to_string()))?;
                component = Some(self.sandbox.load(&bytes)?);
            }
        }
        let loaded = LoadedSkill {
            record: record.clone(),
            manifest: parsed.manifest,
            prompt: parsed.prompt,
            component,
        };
        self.loaded
            .write()
            .expect("skills poisoned")
            .insert(record.id.clone(), loaded);
        Ok(())
    }

    fn unload(&self, id: &str) {
        self.loaded.write().expect("skills poisoned").remove(id);
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), SkillError> {
        self.storage.execute(
            "UPDATE skills SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::from(i64::from(enabled)),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    fn metric(&self, id: &str, parent: Option<&str>, depth: usize, status: &str, ms: i64) {
        let _ = self.storage.execute(
            "INSERT INTO skill_invocations (id, skill_id, parent_id, depth, status, duration_ms, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(id.to_string()),
                parent.map_or(Value::Null, |value| Value::String(value.to_string())),
                Value::from(depth as i64),
                Value::String(status.to_string()),
                Value::from(ms),
                Value::from(now_ms()),
            ],
        );
    }

    fn cache_get(&self, id: &str, key: &str) -> Result<Option<serde_json::Value>, SkillError> {
        let rows = self.storage.query(
            "SELECT output, expires_at FROM skill_cache WHERE skill_id = ?1 AND input_hash = ?2",
            &[
                Value::String(id.to_string()),
                Value::String(key.to_string()),
            ],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        if row.get("expires_at").and_then(Value::as_i64).unwrap_or(0) <= now_ms() {
            return Ok(None);
        }
        Ok(row
            .get("output")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str(text).ok()))
    }

    fn cache_put(
        &self,
        id: &str,
        key: &str,
        output: &serde_json::Value,
        expires_at: i64,
    ) -> Result<(), SkillError> {
        self.storage.execute(
            "INSERT INTO skill_cache (skill_id, input_hash, output, expires_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(skill_id, input_hash) DO UPDATE SET output = ?3, expires_at = ?4",
            &[
                Value::String(id.to_string()),
                Value::String(key.to_string()),
                Value::String(output.to_string()),
                Value::from(expires_at),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Err(err) = self
            .bus
            .publish(kind.to_string(), "skills".to_string(), payload)
        {
            tracing::warn!(error = %err, kind, "failed to publish skill event");
        }
    }
}

/// The host bridge for code skills: chaining, storage, and logs only.
struct SkillHost {
    service: Arc<SkillService>,
    skill_id: String,
    permissions: Vec<String>,
    depth: usize,
    stack: Vec<String>,
    storage: Arc<dyn Storage>,
}

impl SkillHost {
    fn new(
        service: Arc<SkillService>,
        skill_id: String,
        permissions: Vec<String>,
        depth: usize,
        stack: Vec<String>,
        storage: Arc<dyn Storage>,
    ) -> Self {
        Self {
            service,
            skill_id,
            permissions,
            depth,
            stack,
            storage,
        }
    }

    fn require(&self, permission: &str) -> Result<(), HostError> {
        if self.permissions.iter().any(|item| item == permission) {
            Ok(())
        } else {
            Err(HostError::denied(format!(
                "{} does not declare {permission}",
                self.skill_id
            )))
        }
    }
}

impl HostBridge for SkillHost {
    fn call(&self, op: &str, payload: &str) -> Result<String, HostError> {
        let payload: serde_json::Value =
            serde_json::from_str(payload).unwrap_or_else(|_| json!({}));
        match op {
            "log.debug" | "log.info" | "log.warn" | "log.error" => {
                tracing::info!(skill = %self.skill_id, op, message = payload.get("message").and_then(serde_json::Value::as_str).unwrap_or(""), "skill log");
                Ok("{}".to_string())
            }
            "skill.invoke" => {
                self.require("skill.invoke")?;
                let child = payload
                    .get("skill_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HostError::failed("missing skill_id"))?;
                let input = payload.get("input").cloned().unwrap_or_else(|| json!({}));
                let mut stack = self.stack.clone();
                stack.push(self.skill_id.clone());
                self.service
                    .invoke_at(child, input, self.depth + 1, &stack, Some(&self.skill_id))
                    .map(|value| value.to_string())
                    .map_err(|err| HostError::failed(err.to_string()))
            }
            "storage.get" => {
                self.require("storage.local")?;
                let key = payload
                    .get("key")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HostError::failed("missing key"))?;
                let rows = self
                    .storage
                    .query(
                        "SELECT value FROM skill_storage WHERE skill_id = ?1 AND key = ?2",
                        &[
                            Value::String(self.skill_id.clone()),
                            Value::String(key.to_string()),
                        ],
                    )
                    .map_err(|err| HostError::failed(err.to_string()))?;
                Ok(serde_json::to_string(
                    &rows
                        .first()
                        .and_then(|row| row.get("value"))
                        .and_then(Value::as_str),
                )
                .unwrap_or_else(|_| "null".to_string()))
            }
            "storage.set" => {
                self.require("storage.local")?;
                let key = payload
                    .get("key")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HostError::failed("missing key"))?;
                let value = payload
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
                    .to_string();
                if value.len() > 1024 * 1024 {
                    return Err(HostError::failed("skill storage entry is too large"));
                }
                self.storage
                    .execute(
                        "INSERT INTO skill_storage (skill_id, key, value, updated_at) \
                         VALUES (?1, ?2, ?3, ?4) \
                         ON CONFLICT(skill_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
                        &[
                            Value::String(self.skill_id.clone()),
                            Value::String(key.to_string()),
                            Value::String(value),
                            Value::from(now_ms()),
                        ],
                    )
                    .map_err(|err| HostError::failed(err.to_string()))?;
                Ok("{}".to_string())
            }
            "storage.delete" => {
                self.require("storage.local")?;
                let key = payload
                    .get("key")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HostError::failed("missing key"))?;
                self.storage
                    .execute(
                        "DELETE FROM skill_storage WHERE skill_id = ?1 AND key = ?2",
                        &[
                            Value::String(self.skill_id.clone()),
                            Value::String(key.to_string()),
                        ],
                    )
                    .map_err(|err| HostError::failed(err.to_string()))?;
                Ok("{}".to_string())
            }
            "llm.chat" => {
                self.require("llm.call")?;
                let model = payload
                    .get("model")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let messages = payload
                    .get("messages")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                let text = self
                    .service
                    .services
                    .llm_chat(model, "", &messages, 0.2, 2048)
                    .map_err(HostError::failed)?;
                Ok(json!({ "text": text }).to_string())
            }
            other => Err(HostError::unsupported(other)),
        }
    }
}

fn read_skill(root: &Path) -> Result<ParsedSkill, SkillError> {
    let text = std::fs::read_to_string(root.join("SKILL.md"))?;
    let manifest = manifest::parse(&text)?;
    let mut prompt = manifest::body(&text);
    if let Ok(prompt_file) = std::fs::read_to_string(root.join("prompt.md")) {
        prompt = prompt_file;
    }
    if matches!(manifest.kind, SkillKind::Prompt | SkillKind::Hybrid) && prompt.trim().is_empty() {
        return Err(SkillError::Invalid("skill prompt is empty".to_string()));
    }
    if let Some(entry) = &manifest.entry {
        if entry.is_empty()
            || Path::new(entry).components().any(|component| {
                !matches!(
                    component,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
        {
            return Err(SkillError::Invalid(format!("unsafe skill entry: {entry}")));
        }
        if !root.join(entry).is_file() {
            return Err(SkillError::Invalid(format!("missing skill entry: {entry}")));
        }
    }
    Ok(ParsedSkill { manifest, prompt })
}

fn loaded_tool(wraps: &Option<String>) -> Result<String, SkillError> {
    let wraps = wraps
        .as_deref()
        .ok_or_else(|| SkillError::Invalid("missing wraps".to_string()))?;
    Ok(wraps.to_string())
}

fn cache_key(manifest: &SkillManifest, input: &serde_json::Value, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest.id.as_bytes());
    hasher.update([0u8]);
    hasher.update(manifest.version.as_bytes());
    hasher.update([0u8]);
    hasher.update(model.as_bytes());
    hasher.update(manifest.temperature.to_le_bytes());
    hasher.update(input.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }
    let start = trimmed.find("```")?;
    let after = &trimmed[start + 3..];
    let after = after
        .split_once('\n')
        .map(|(_, rest)| rest)
        .unwrap_or(after);
    let end = after.rfind("```")?;
    serde_json::from_str(after[..end].trim()).ok()
}

fn merge(base: serde_json::Value, next: serde_json::Value) -> serde_json::Value {
    match (base, next) {
        (serde_json::Value::Object(mut left), serde_json::Value::Object(right)) => {
            left.extend(right);
            serde_json::Value::Object(left)
        }
        (_, next) => next,
    }
}

fn truncate(text: &str) -> String {
    text.chars().take(200).collect()
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), SkillError> {
    std::fs::create_dir_all(target)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|err| SkillError::Io(std::io::Error::other(err)))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| SkillError::Invalid("bad path".to_string()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(SkillError::Invalid(format!(
                "symlink not allowed: {}",
                relative.display()
            )));
        }
        let destination = target.join(relative);
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

fn move_dir(source: &Path, target: &Path) -> Result<(), SkillError> {
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

/// Build a `.fergskill` archive from a folder.
pub fn package_skill(source: &Path, out: &Path) -> Result<(), SkillError> {
    create_archive(source, out, false).map_err(SkillError::from)
}
