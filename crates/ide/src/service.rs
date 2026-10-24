//! Project records, trust state, and the file index.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use fergents_events::{now_ms, EventBus};
use fergents_storage::{Row, Storage, Value};
use serde_json::json;

use crate::error::ProjectError;
use crate::index;
use crate::model::{Project, SearchHit, TrustState};
use crate::path;

/// Upper bound on the in-memory file index per project.
const MAX_INDEX_FILES: usize = 50_000;
/// Default page size for file and search queries.
const DEFAULT_LIMIT: usize = 50;
/// Hard cap so a client cannot ask for an unbounded page.
const MAX_LIMIT: usize = 500;

/// The project layer: open, list, trust, index, and search code workspaces.
pub struct ProjectService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    indexes: RwLock<HashMap<String, Vec<String>>>,
}

impl ProjectService {
    /// Create the service.
    pub fn new(storage: Arc<dyn Storage>, bus: Arc<EventBus>) -> Self {
        Self {
            storage,
            bus,
            indexes: RwLock::new(HashMap::new()),
        }
    }

    /// List projects, most recently opened first.
    pub fn list(&self) -> Result<Vec<Project>, ProjectError> {
        let rows = self
            .storage
            .query("SELECT * FROM projects ORDER BY last_opened_at DESC", &[])?;
        Ok(rows.iter().map(row_to_project).collect())
    }

    /// Fetch one project.
    pub fn get(&self, id: &str) -> Result<Option<Project>, ProjectError> {
        let rows = self.storage.query(
            "SELECT * FROM projects WHERE project_id = ?1",
            &[string(id)],
        )?;
        Ok(rows.first().map(row_to_project))
    }

    /// Fetch one project or fail with `project_unknown`.
    pub fn require(&self, id: &str) -> Result<Project, ProjectError> {
        self.get(id)?
            .ok_or_else(|| ProjectError::Unknown(id.to_string()))
    }

    /// Open a folder as a project. Re-opening the same path is idempotent.
    ///
    /// The project is recorded as **untrusted** (spec decision D3).
    pub fn open(&self, raw_path: &str) -> Result<Project, ProjectError> {
        let input = raw_path.trim();
        if input.is_empty() {
            return Err(ProjectError::Invalid("empty path".to_string()));
        }
        let canonical = std::fs::canonicalize(input)
            .map_err(|_| ProjectError::InvalidPath(input.to_string()))?;
        if !canonical.is_dir() {
            return Err(ProjectError::InvalidPath(input.to_string()));
        }
        let canonical_str = canonical.to_string_lossy().into_owned();

        if let Some(existing) = self.find_by_path(&canonical_str)? {
            self.storage.execute(
                "UPDATE projects SET last_opened_at = ?1, updated_at = ?1 WHERE project_id = ?2",
                &[Value::from(now_ms()), string(&existing)],
            )?;
            let project = self.require(&existing)?;
            self.refresh_index(&project)?;
            return Ok(project);
        }

        let id = ulid::Ulid::new().to_string();
        let name = canonical
            .file_name()
            .map(|part| part.to_string_lossy().into_owned())
            .filter(|part| !part.is_empty())
            .unwrap_or_else(|| canonical_str.clone());
        let vcs = detect_vcs(&canonical);
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO projects \
             (project_id, path, name, vcs, brain_id, trust, settings, last_opened_at, \
              created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, NULL, ?6, ?6, ?6)",
            &[
                string(&id),
                string(&canonical_str),
                string(&name),
                vcs.as_ref().map_or(Value::Null, |vcs| string(vcs)),
                string(TrustState::Untrusted.as_str()),
                Value::from(now),
            ],
        )?;

        let project = self.require(&id)?;
        self.bus.publish(
            "project.opened",
            "project",
            json!({
                "project_id": project.id,
                "path": project.path,
                "name": project.name,
                "trust": project.trust.as_str(),
            }),
        )?;
        self.refresh_index(&project)?;
        Ok(project)
    }

    /// Close a project: remove its record and its index.
    pub fn close(&self, id: &str) -> Result<bool, ProjectError> {
        self.require(id)?;
        let affected = self
            .storage
            .execute("DELETE FROM projects WHERE project_id = ?1", &[string(id)])?;
        self.storage.execute(
            "DELETE FROM editor_layouts WHERE project_id = ?1",
            &[string(id)],
        )?;
        if let Ok(mut indexes) = self.indexes.write() {
            indexes.remove(id);
        }
        self.bus
            .publish("project.closed", "project", json!({ "project_id": id }))?;
        Ok(affected > 0)
    }

    /// Rename a project, link/unlink a brain, or replace its settings.
    ///
    /// `brain_id`: `None` leaves it unchanged, `Some(None)` clears it.
    pub fn update(
        &self,
        id: &str,
        name: Option<&str>,
        brain_id: Option<Option<String>>,
        settings: Option<Value>,
    ) -> Result<Project, ProjectError> {
        let current = self.require(id)?;
        let name = name.map(str::to_string).unwrap_or(current.name);
        if name.trim().is_empty() {
            return Err(ProjectError::Invalid("empty project name".to_string()));
        }
        let brain_id = match brain_id {
            None => current.brain_id,
            Some(value) => value,
        };
        let settings = settings.or(current.settings);
        self.storage.execute(
            "UPDATE projects SET name = ?1, brain_id = ?2, settings = ?3, updated_at = ?4 \
             WHERE project_id = ?5",
            &[
                string(&name),
                brain_id.map_or(Value::Null, |value| string(&value)),
                settings.map_or(Value::Null, Value::from),
                Value::from(now_ms()),
                string(id),
            ],
        )?;
        self.require(id)
    }

    /// Grant or revoke trust. Revoking stops running subsystems (task 3.3+).
    pub fn set_trust(&self, id: &str, trusted: bool) -> Result<Project, ProjectError> {
        self.require(id)?;
        let trust = if trusted {
            TrustState::Trusted
        } else {
            TrustState::Untrusted
        };
        self.storage.execute(
            "UPDATE projects SET trust = ?1, updated_at = ?2 WHERE project_id = ?3",
            &[string(trust.as_str()), Value::from(now_ms()), string(id)],
        )?;
        let project = self.require(id)?;
        let kind = if trusted {
            "project.trusted"
        } else {
            "project.untrusted"
        };
        self.bus.publish(
            kind,
            "project",
            json!({ "project_id": project.id, "trust": trust.as_str() }),
        )?;
        Ok(project)
    }

    /// Return the project when trusted, else `project_untrusted`.
    pub fn require_trusted(&self, id: &str) -> Result<Project, ProjectError> {
        let project = self.require(id)?;
        if project.is_trusted() {
            Ok(project)
        } else {
            Err(ProjectError::Untrusted(id.to_string()))
        }
    }

    /// Fuzzy file list for quick-open.
    pub fn files(
        &self,
        id: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<String>, ProjectError> {
        let project = self.require(id)?;
        let limit = clamp_limit(limit);
        let files = self.indexed_files(&project)?;
        Ok(index::fuzzy(query, &files, limit))
    }

    /// Bounded, ignore-aware text search over the project.
    pub fn search(
        &self,
        id: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SearchHit>, ProjectError> {
        let project = self.require(id)?;
        let limit = clamp_limit(limit);
        let root = Path::new(&project.path);
        Ok(index::search(root, query, limit, MAX_INDEX_FILES))
    }

    /// Resolve a project-relative path, rejecting escapes.
    pub fn resolve(&self, project: &Project, rel: &str) -> Result<PathBuf, ProjectError> {
        let root = Path::new(&project.path);
        path::contained(root, rel)
    }

    /// The persisted layout tree for a project, if one was ever saved
    /// (spec decision D14).
    pub fn layout(&self, id: &str) -> Result<Option<serde_json::Value>, ProjectError> {
        self.require(id)?;
        let rows = self.storage.query(
            "SELECT layout FROM editor_layouts WHERE project_id = ?1",
            &[string(id)],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("layout"))
            .and_then(json_value))
    }

    /// Persist the layout tree, replacing any previous one.
    pub fn set_layout(
        &self,
        id: &str,
        layout: serde_json::Value,
    ) -> Result<serde_json::Value, ProjectError> {
        self.require(id)?;
        if !layout.is_object() {
            return Err(ProjectError::Invalid(
                "layout must be an object".to_string(),
            ));
        }
        let raw = serde_json::to_string(&layout)
            .map_err(|error| ProjectError::Invalid(error.to_string()))?;
        self.storage.execute(
            "INSERT INTO editor_layouts (project_id, layout, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(project_id) DO UPDATE SET layout = excluded.layout, \
             updated_at = excluded.updated_at",
            &[string(id), string(&raw), Value::from(now_ms())],
        )?;
        self.bus
            .publish("layout.updated", "project", json!({ "project_id": id }))?;
        Ok(layout)
    }

    /// Build (or reuse) and return the cached file index.
    fn indexed_files(&self, project: &Project) -> Result<Vec<String>, ProjectError> {
        if let Ok(indexes) = self.indexes.read() {
            if let Some(files) = indexes.get(&project.id) {
                return Ok(files.clone());
            }
        }
        let files = self.build_index(project);
        if let Ok(mut indexes) = self.indexes.write() {
            indexes.insert(project.id.clone(), files.clone());
        }
        Ok(files)
    }

    /// Rebuild the index for `project` and publish `project.index.updated`.
    pub fn refresh_index(&self, project: &Project) -> Result<Vec<String>, ProjectError> {
        let files = self.build_index(project);
        if let Ok(mut indexes) = self.indexes.write() {
            indexes.insert(project.id.clone(), files.clone());
        }
        self.bus.publish(
            "project.index.updated",
            "project",
            json!({ "project_id": project.id, "files": files.len() }),
        )?;
        Ok(files)
    }

    fn build_index(&self, project: &Project) -> Vec<String> {
        index::build_index(Path::new(&project.path), MAX_INDEX_FILES)
    }

    fn find_by_path(&self, path: &str) -> Result<Option<String>, ProjectError> {
        let rows = self.storage.query(
            "SELECT project_id FROM projects WHERE path = ?1",
            &[string(path)],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("project_id"))
            .and_then(Value::as_str)
            .map(str::to_string))
    }
}

fn string(value: &str) -> Value {
    Value::String(value.to_string())
}

/// Decode a JSON column that may hold a string, an inline value, or `NULL`.
fn json_value(value: &Value) -> Option<serde_json::Value> {
    match value {
        Value::Null => None,
        Value::String(raw) => serde_json::from_str(raw).ok(),
        other => Some(other.clone()),
    }
}

fn clamp_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Detect `git` at the project root or any ancestor directory.
fn detect_vcs(root: &Path) -> Option<String> {
    let mut current = Some(root);
    while let Some(dir) = current {
        if index::has_git_dir(dir) {
            return Some("git".to_string());
        }
        current = dir.parent();
    }
    None
}

fn row_to_project(row: &Row) -> Project {
    let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
    Project {
        id: text("project_id").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        vcs: text("vcs"),
        brain_id: text("brain_id"),
        trust: row
            .get("trust")
            .and_then(Value::as_str)
            .map(TrustState::parse)
            .unwrap_or(TrustState::Untrusted),
        settings: row.get("settings").and_then(json_value),
        last_opened_at: row
            .get("last_opened_at")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
        updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
    }
}
