//! The brain lifecycle and multi-brain operations (`BRAIN.md` §7–§9, §14).
//!
//! [`BrainManager`] owns the global `brains` / `projects` / `project_brains`
//! registry and the on-disk brain folders. Indexing is delegated to a
//! [`BrainIndexer`] so the lifecycle is testable without the full pipeline.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fergents_index::EventSink;
use fergents_storage::{Storage, Value};
use serde_json::json;

use crate::config::{BrainConfig, BrainIdentity};
use crate::error::BrainError;
use crate::indexing::{BrainIndexer, IndexReport};
use crate::portable;
use crate::record::BrainRecord;
use crate::roles::{self, BrainRole};
use crate::store::available_schema_version;
use crate::tmpl::BrainTemplate;
use crate::vault::VaultLayout;

/// A request to create a brain.
#[derive(Debug, Clone)]
pub struct CreateBrainSpec {
    /// Human-readable name.
    pub name: String,
    /// URL/filesystem-friendly slug (derived from `name` when empty).
    pub slug: String,
    /// The brain folder to create.
    pub path: PathBuf,
    /// The template to seed from.
    pub template: BrainTemplate,
    /// Optional accent color.
    pub color: Option<String>,
    /// Optional icon.
    pub icon: Option<String>,
}

impl CreateBrainSpec {
    /// A spec with the `blank` template and a slug derived from `name`.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let name = name.into();
        Self {
            slug: slugify(&name),
            name,
            path: path.into(),
            template: BrainTemplate::Blank,
            color: None,
            icon: None,
        }
    }

    /// Select a template.
    pub fn with_template(mut self, template: BrainTemplate) -> Self {
        self.template = template;
        self
    }
}

/// Fields that [`BrainManager::update`] may change.
#[derive(Debug, Clone, Default)]
pub struct BrainPatch {
    /// New name.
    pub name: Option<String>,
    /// New accent color.
    pub color: Option<String>,
    /// New icon.
    pub icon: Option<String>,
}

/// The brain lifecycle and multi-brain manager.
pub struct BrainManager {
    global: Arc<dyn Storage>,
    sink: Arc<dyn EventSink>,
    brain_migrations_dir: PathBuf,
    templates_root: PathBuf,
    open: Mutex<HashSet<String>>,
}

impl BrainManager {
    /// Build a manager over the global `storage`, emitting through `sink`.
    ///
    /// `brain_migrations_dir` is the per-brain migrations directory
    /// (`migrations/brain/`); `templates_root` is `$FERGENTS_HOME/templates/brains/`.
    pub fn new(
        global: Arc<dyn Storage>,
        sink: Arc<dyn EventSink>,
        brain_migrations_dir: impl Into<PathBuf>,
        templates_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            global,
            sink,
            brain_migrations_dir: brain_migrations_dir.into(),
            templates_root: templates_root.into(),
            open: Mutex::new(HashSet::new()),
        }
    }

    /// Whether the brain is currently open in this manager.
    pub fn is_open(&self, brain_id: &str) -> bool {
        self.open
            .lock()
            .expect("open set poisoned")
            .contains(brain_id)
    }

    /// List registered brains, ordered by name. Archived brains are excluded
    /// unless `include_archived` is set.
    pub fn list(&self, include_archived: bool) -> Result<Vec<BrainRecord>, BrainError> {
        let sql = if include_archived {
            "SELECT * FROM brains ORDER BY name"
        } else {
            "SELECT * FROM brains WHERE archived_at IS NULL ORDER BY name"
        };
        let rows = self.global.query(sql, &[])?;
        rows.iter().map(BrainRecord::from_row).collect()
    }

    /// Fetch a brain by id.
    pub fn get(&self, brain_id: &str) -> Result<BrainRecord, BrainError> {
        self.optional_row(brain_id)?
            .ok_or_else(|| BrainError::invalid(format!("unknown brain: {brain_id}")))
    }

    /// Create a brain: write the folder + `brain.toml`, register it, run the
    /// first index, and emit `brain.created` (`BRAIN.md` §9.2).
    pub fn create(
        &self,
        indexer: &dyn BrainIndexer,
        spec: &CreateBrainSpec,
    ) -> Result<BrainRecord, BrainError> {
        if spec.name.trim().is_empty() {
            return Err(BrainError::invalid("brain name must not be empty"));
        }
        require_empty_target(&spec.path)?;

        let brain_id = ulid::Ulid::new().to_string();
        let config = BrainConfig {
            brain: BrainIdentity {
                id: brain_id.clone(),
                name: spec.name.clone(),
                slug: spec.slug.clone(),
                template: Some(spec.template.as_str().to_string()),
                color: spec.color.clone(),
                icon: spec.icon.clone(),
                created_at: rfc3339_now(),
            },
            ..BrainConfig::default()
        };

        let layout = VaultLayout::from_config(spec.path.clone(), &config);
        layout.ensure()?;
        seed_template(&layout, spec.template)?;
        config.save(&spec.path)?;

        let record = BrainRecord {
            brain_id,
            name: spec.name.clone(),
            path: spec.path.clone(),
            role: None,
            schema_version: available_schema_version(&self.brain_migrations_dir)?,
            created_at: now_ms(),
            last_opened_at: None,
            archived_at: None,
        };
        self.insert_registry(&record)?;

        let report = indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;
        self.emit(
            "brain.created",
            &record,
            json!({ "documents": report.documents }),
        );
        Ok(record)
    }

    /// Open a brain: read `brain.toml`, run an incremental index, mark it open,
    /// and emit `brain.opened` (`BRAIN.md` §9.3).
    pub fn open(
        &self,
        indexer: &dyn BrainIndexer,
        brain_id: &str,
    ) -> Result<BrainRecord, BrainError> {
        let record = self.get(brain_id)?;
        let config = BrainConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let report = indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;

        let now = now_ms();
        self.global.execute(
            "UPDATE brains SET last_opened_at = ?1 WHERE brain_id = ?2",
            &[Value::from(now), Value::String(brain_id.to_string())],
        )?;
        self.open
            .lock()
            .expect("open set poisoned")
            .insert(brain_id.to_string());

        let record = BrainRecord {
            last_opened_at: Some(now),
            ..record
        };
        self.emit(
            "brain.opened",
            &record,
            json!({ "documents": report.documents }),
        );
        Ok(record)
    }

    /// Close a brain: flush pending work, stop tracking it, and emit
    /// `brain.closed` (`BRAIN.md` §9.4).
    pub fn close(&self, indexer: &dyn BrainIndexer, brain_id: &str) -> Result<(), BrainError> {
        let record = self.get(brain_id)?;
        indexer.flush(&record.path)?;
        self.open
            .lock()
            .expect("open set poisoned")
            .remove(brain_id);
        self.emit("brain.closed", &record, json!({}));
        Ok(())
    }

    /// Update a brain's name, color, or icon in `brain.toml` and the registry,
    /// then emit `brain.updated`.
    pub fn update(&self, brain_id: &str, patch: &BrainPatch) -> Result<BrainRecord, BrainError> {
        let record = self.get(brain_id)?;
        let mut config = BrainConfig::load(&record.path)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(BrainError::invalid("brain name must not be empty"));
            }
            config.brain.name = name.clone();
        }
        if let Some(color) = &patch.color {
            config.brain.color = Some(color.clone());
        }
        if let Some(icon) = &patch.icon {
            config.brain.icon = Some(icon.clone());
        }
        config.save(&record.path)?;

        if let Some(name) = &patch.name {
            self.global.execute(
                "UPDATE brains SET name = ?1 WHERE brain_id = ?2",
                &[
                    Value::String(name.clone()),
                    Value::String(brain_id.to_string()),
                ],
            )?;
        }
        let record = self.get(brain_id)?;
        self.emit("brain.updated", &record, json!({}));
        Ok(record)
    }

    /// Archive a brain: remove it from the active list, keep its files
    /// (`BRAIN.md` §9.5). Emits `brain.archived`.
    pub fn archive(&self, brain_id: &str) -> Result<BrainRecord, BrainError> {
        let _ = self.get(brain_id)?;
        let now = now_ms();
        self.global.execute(
            "UPDATE brains SET archived_at = ?1 WHERE brain_id = ?2",
            &[Value::from(now), Value::String(brain_id.to_string())],
        )?;
        let record = self.get(brain_id)?;
        self.emit("brain.archived", &record, json!({}));
        Ok(record)
    }

    /// Un-archive a brain, returning it to the active list. Emits `brain.updated`.
    pub fn unarchive(&self, brain_id: &str) -> Result<BrainRecord, BrainError> {
        let _ = self.get(brain_id)?;
        self.global.execute(
            "UPDATE brains SET archived_at = NULL WHERE brain_id = ?1",
            &[Value::String(brain_id.to_string())],
        )?;
        let record = self.get(brain_id)?;
        self.emit("brain.updated", &record, json!({ "archived": false }));
        Ok(record)
    }

    /// Delete a brain (`BRAIN.md` §9.6).
    ///
    /// Requires the caller to echo the brain's exact name; optionally exports
    /// first. The folder is removed from disk and the registry row deleted, and
    /// `brain.deleted` is emitted.
    pub fn delete(
        &self,
        indexer: &dyn BrainIndexer,
        brain_id: &str,
        confirm_name: &str,
        export_to: Option<&Path>,
    ) -> Result<(), BrainError> {
        let record = self.get(brain_id)?;
        if confirm_name != record.name {
            return Err(BrainError::invalid(format!(
                "confirmation {:?} does not match brain name {:?}",
                confirm_name, record.name
            )));
        }
        if let Some(path) = export_to {
            portable::export(&record.path, path)?;
        }
        indexer.flush(&record.path)?;
        self.open
            .lock()
            .expect("open set poisoned")
            .remove(brain_id);

        // Phase 1 has no trash configuration: files are removed permanently.
        if record.path.exists() {
            std::fs::remove_dir_all(&record.path).map_err(|e| BrainError::io(&record.path, e))?;
        }
        self.global.execute(
            "DELETE FROM brains WHERE brain_id = ?1",
            &[Value::String(brain_id.to_string())],
        )?;
        self.emit("brain.deleted", &record, json!({}));
        Ok(())
    }

    /// Duplicate a brain into `target` with a new id and its own index
    /// (`BRAIN.md` §8.5). Emits `brain.created`.
    pub fn duplicate(
        &self,
        indexer: &dyn BrainIndexer,
        brain_id: &str,
        target: &Path,
        new_name: Option<&str>,
    ) -> Result<BrainRecord, BrainError> {
        let source = self.get(brain_id)?;
        require_empty_target(target)?;
        copy_tree(&source.path, target)?;

        let mut config = BrainConfig::load(target)?;
        let new_id = ulid::Ulid::new().to_string();
        config.brain.id = new_id.clone();
        config.brain.name = new_name
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} copy", source.name));
        config.brain.slug = slugify(&config.brain.name);
        config.brain.created_at = rfc3339_now();
        config.save(target)?;

        let layout = VaultLayout::from_config(target.to_path_buf(), &config);
        let record = BrainRecord {
            brain_id: new_id,
            name: config.brain.name.clone(),
            path: target.to_path_buf(),
            role: None,
            schema_version: available_schema_version(&self.brain_migrations_dir)?,
            created_at: now_ms(),
            last_opened_at: None,
            archived_at: None,
        };
        self.insert_registry(&record)?;

        let report = indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;
        self.emit(
            "brain.created",
            &record,
            json!({ "duplicated_from": source.brain_id, "documents": report.documents }),
        );
        Ok(record)
    }

    /// Move a brain to `target`, updating the registry but preserving its id
    /// (`BRAIN.md` §10.4). Emits `brain.updated`.
    pub fn mv(&self, brain_id: &str, target: &Path) -> Result<BrainRecord, BrainError> {
        let record = self.get(brain_id)?;
        require_empty_target(target)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BrainError::io(parent, e))?;
        }
        std::fs::rename(&record.path, target).map_err(|e| BrainError::io(target, e))?;
        self.global.execute(
            "UPDATE brains SET path = ?1 WHERE brain_id = ?2",
            &[
                Value::String(target.to_string_lossy().into_owned()),
                Value::String(brain_id.to_string()),
            ],
        )?;
        let record = self.get(brain_id)?;
        self.emit("brain.updated", &record, json!({ "moved": true }));
        Ok(record)
    }

    /// Re-run the index for a brain and emit `brain.indexed` (`BRAIN.md` §15).
    pub fn reindex(
        &self,
        indexer: &dyn BrainIndexer,
        brain_id: &str,
    ) -> Result<IndexReport, BrainError> {
        let record = self.get(brain_id)?;
        let config = BrainConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let report = indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;
        self.emit(
            "brain.indexed",
            &record,
            json!({ "documents": report.documents }),
        );
        Ok(report)
    }

    /// Write a note into a brain's vault and index it (`BRAIN.md` §8.1).
    ///
    /// When `project_id` is given and the brain is attached as `read-only`, the
    /// write is rejected with a clear error.
    pub fn write_note(
        &self,
        indexer: &dyn BrainIndexer,
        project_id: Option<&str>,
        brain_id: &str,
        rel_path: &str,
        content: &str,
    ) -> Result<PathBuf, BrainError> {
        if let Some(project) = project_id {
            self.require_writable(project, brain_id)?;
        }
        let record = self.get(brain_id)?;
        if record.is_archived() {
            return Err(BrainError::invalid(format!(
                "brain {:?} is archived; writes are not allowed",
                record.name
            )));
        }
        if rel_path.is_empty() || Path::new(rel_path).is_absolute() {
            return Err(BrainError::invalid(format!(
                "note path must be vault-relative: {rel_path:?}"
            )));
        }
        let config = BrainConfig::load(&record.path)?;
        let layout = VaultLayout::from_config(record.path.clone(), &config);
        let abs = layout.vault_root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BrainError::io(parent, e))?;
        }
        std::fs::write(&abs, content).map_err(|e| BrainError::io(&abs, e))?;
        indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;
        Ok(abs)
    }

    /// Attach a brain to a project with a role (`BRAIN.md` §8.1).
    pub fn attach(
        &self,
        project_id: &str,
        brain_id: &str,
        role: BrainRole,
    ) -> Result<(), BrainError> {
        let _ = self.get(brain_id)?;
        self.global.execute(
            "INSERT OR IGNORE INTO projects (project_id, name, path, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            &[
                Value::String(project_id.to_string()),
                Value::String(project_id.to_string()),
                Value::String(String::new()),
                Value::from(now_ms()),
            ],
        )?;
        self.global.execute(
            "INSERT INTO project_brains (project_id, brain_id, role) VALUES (?1, ?2, ?3) \
             ON CONFLICT(project_id, brain_id) DO UPDATE SET role = excluded.role",
            &[
                Value::String(project_id.to_string()),
                Value::String(brain_id.to_string()),
                Value::String(role.as_str().to_string()),
            ],
        )?;
        self.global.execute(
            "UPDATE brains SET role = ?1 WHERE brain_id = ?2",
            &[
                Value::String(role.as_str().to_string()),
                Value::String(brain_id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Detach a brain from a project.
    pub fn detach(&self, project_id: &str, brain_id: &str) -> Result<(), BrainError> {
        self.global.execute(
            "DELETE FROM project_brains WHERE project_id = ?1 AND brain_id = ?2",
            &[
                Value::String(project_id.to_string()),
                Value::String(brain_id.to_string()),
            ],
        )?;
        self.global.execute(
            "UPDATE brains SET role = NULL WHERE brain_id = ?1",
            &[Value::String(brain_id.to_string())],
        )?;
        Ok(())
    }

    /// The role of a brain in a project, if attached.
    pub fn role_for(
        &self,
        project_id: &str,
        brain_id: &str,
    ) -> Result<Option<BrainRole>, BrainError> {
        let rows = self.global.query(
            "SELECT role FROM project_brains WHERE project_id = ?1 AND brain_id = ?2",
            &[
                Value::String(project_id.to_string()),
                Value::String(brain_id.to_string()),
            ],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("role"))
            .and_then(Value::as_str)
            .and_then(BrainRole::parse_opt))
    }

    /// Reject a write to a brain attached to `project_id` as `read-only`.
    pub fn require_writable(&self, project_id: &str, brain_id: &str) -> Result<(), BrainError> {
        match self.role_for(project_id, brain_id)? {
            Some(role) => roles::require_writable(role),
            None => Ok(()),
        }
    }

    /// Save a brain as a template under the templates root (`BRAIN.md` §7.2).
    /// Regenerable files are excluded.
    pub fn save_template(&self, brain_id: &str, name: &str) -> Result<PathBuf, BrainError> {
        let record = self.get(brain_id)?;
        if name.trim().is_empty() {
            return Err(BrainError::invalid("template name must not be empty"));
        }
        let dest = self.templates_root.join(name);
        if dest.exists() {
            return Err(BrainError::invalid(format!(
                "template {:?} already exists",
                name
            )));
        }
        std::fs::create_dir_all(&self.templates_root)
            .map_err(|e| BrainError::io(&self.templates_root, e))?;
        copy_tree(&record.path, &dest)?;
        Ok(dest)
    }

    /// Export a brain to a `.fergbrain` archive (`BRAIN.md` §14.3). Emits
    /// `brain.exported`.
    pub fn export(
        &self,
        brain_id: &str,
        archive_path: &Path,
    ) -> Result<portable::Manifest, BrainError> {
        let record = self.get(brain_id)?;
        let manifest = portable::export(&record.path, archive_path)?;
        self.emit(
            "brain.exported",
            &record,
            json!({ "path": archive_path.to_string_lossy(), "files": manifest.files.len() }),
        );
        Ok(manifest)
    }

    /// Import a `.fergbrain` archive into `target`, register it, index it, and
    /// emit `brain.imported` (`BRAIN.md` §14.2).
    pub fn import(
        &self,
        indexer: &dyn BrainIndexer,
        archive_path: &Path,
        target: &Path,
    ) -> Result<BrainRecord, BrainError> {
        require_empty_target(target)?;
        let summary = portable::import(archive_path, target)?;
        let config = BrainConfig::load(target)?;
        let layout = VaultLayout::from_config(target.to_path_buf(), &config);
        let record = BrainRecord {
            brain_id: summary.brain_id,
            name: summary.name,
            path: target.to_path_buf(),
            role: None,
            schema_version: available_schema_version(&self.brain_migrations_dir)?,
            created_at: now_ms(),
            last_opened_at: None,
            archived_at: None,
        };
        self.insert_registry(&record)?;
        let report = indexer.index_brain(&record.brain_id, &record.path, &layout.vault_root)?;
        self.emit(
            "brain.imported",
            &record,
            json!({ "documents": report.documents }),
        );
        Ok(record)
    }

    fn optional_row(&self, brain_id: &str) -> Result<Option<BrainRecord>, BrainError> {
        let rows = self.global.query(
            "SELECT * FROM brains WHERE brain_id = ?1",
            &[Value::String(brain_id.to_string())],
        )?;
        match rows.first() {
            Some(row) => Ok(Some(BrainRecord::from_row(row)?)),
            None => Ok(None),
        }
    }

    fn insert_registry(&self, record: &BrainRecord) -> Result<(), BrainError> {
        self.global.execute(
            "INSERT INTO brains \
             (brain_id, name, path, role, schema_version, created_at, last_opened_at, archived_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                Value::String(record.brain_id.clone()),
                Value::String(record.name.clone()),
                Value::String(record.path.to_string_lossy().into_owned()),
                record
                    .role
                    .map_or(Value::Null, |r| Value::String(r.as_str().to_string())),
                Value::from(record.schema_version),
                Value::from(record.created_at),
                record.last_opened_at.map_or(Value::Null, Value::from),
                record.archived_at.map_or(Value::Null, Value::from),
            ],
        )?;
        Ok(())
    }

    fn emit(&self, kind: &str, record: &BrainRecord, extra: serde_json::Value) {
        let mut payload = json!({
            "brain_id": record.brain_id,
            "name": record.name,
            "path": record.path.to_string_lossy(),
        });
        if let (Some(map), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                map.insert(key.clone(), value.clone());
            }
        }
        self.sink.emit(kind, payload);
    }
}

/// Refuse a target directory that already exists and is non-empty.
fn require_empty_target(path: &Path) -> Result<(), BrainError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Err(BrainError::invalid(format!(
            "target is not a directory: {}",
            path.display()
        )));
    }
    let mut entries = std::fs::read_dir(path).map_err(|e| BrainError::io(path, e))?;
    if entries.next().is_some() {
        return Err(BrainError::invalid(format!(
            "target directory is not empty: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Write a template's seed files into a freshly created vault (additively).
fn seed_template(layout: &VaultLayout, template: BrainTemplate) -> Result<(), BrainError> {
    for (rel, content) in template.seed_files() {
        let out = layout.vault_root.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BrainError::io(parent, e))?;
        }
        if !out.exists() {
            std::fs::write(&out, content).map_err(|e| BrainError::io(&out, e))?;
        }
    }
    Ok(())
}

/// Recursively copy `src` into `dst`, excluding regenerable files and
/// `.fergents/` (`portable::is_excluded`).
fn copy_tree(src: &Path, dst: &Path) -> Result<(), BrainError> {
    std::fs::create_dir_all(dst).map_err(|e| BrainError::io(dst, e))?;
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| BrainError::io(&dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| BrainError::io(&dir, e))?;
            let path = entry.path();
            let rel = path
                .strip_prefix(src)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if portable::is_excluded(&rel) {
                continue;
            }
            let out = dst.join(path.strip_prefix(src).unwrap_or(&path));
            if path.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| BrainError::io(&out, e))?;
                stack.push(path);
            } else {
                std::fs::copy(&path, &out).map_err(|e| BrainError::io(&out, e))?;
            }
        }
    }
    Ok(())
}

/// A filesystem/URL-friendly slug derived from `name`.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    slug
}

/// The current time as an RFC 3339 timestamp (`brain.created_at`).
fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
