//! The capability registry: definitions, state, dependency enforcement, and
//! persistence.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use fergents_events::{now_ms, EventBus};
use fergents_storage::{Storage, Value};
use serde_json::json;

use crate::error::CapabilityError;
use crate::model::{Capability, CapabilityView};

/// The kernel's registry of capabilities.
///
/// Definitions and their enabled state are held in memory (for the hot path)
/// and written through to the global `capabilities` table.
pub struct CapabilityRegistry {
    store: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    defs: RwLock<HashMap<String, Capability>>,
    enabled: RwLock<HashMap<String, bool>>,
    restart: RwLock<HashSet<String>>,
}

impl CapabilityRegistry {
    /// Open the registry over `store` and load any persisted state.
    pub fn new(store: Arc<dyn Storage>, bus: Arc<EventBus>) -> Result<Self, CapabilityError> {
        let registry = Self {
            store,
            bus,
            defs: RwLock::new(HashMap::new()),
            enabled: RwLock::new(HashMap::new()),
            restart: RwLock::new(HashSet::new()),
        };
        registry.load()?;
        Ok(registry)
    }

    /// Load every persisted capability into memory.
    fn load(&self) -> Result<(), CapabilityError> {
        let rows = self
            .store
            .query("SELECT * FROM capabilities ORDER BY id", &[])?;
        let mut defs: HashMap<String, Capability> = HashMap::new();
        let mut enabled: HashMap<String, bool> = HashMap::new();
        for row in rows {
            let Some(id) = row.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            let capability = Capability {
                id: id.clone(),
                name: row
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string(),
                version: row
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("0.1.0")
                    .to_string(),
                source: row
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or(crate::model::DEFAULT_SOURCE)
                    .to_string(),
                description: row
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                dependencies: parse_string_array(row.get("dependencies")),
                resources: parse_string_array(row.get("resources")),
                default_enabled: row
                    .get("default_enabled")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    != 0,
                hot_toggle: row.get("hot_toggle").and_then(Value::as_i64).unwrap_or(0) != 0,
                config: row.get("config").and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_str().and_then(|s| serde_json::from_str(s).ok())
                    }
                }),
            };
            let is_enabled = row.get("enabled").and_then(Value::as_i64).unwrap_or(0) != 0;
            defs.insert(id.clone(), capability);
            enabled.insert(id, is_enabled);
        }
        *self.defs.write().expect("capability defs poisoned") = defs;
        *self.enabled.write().expect("capability state poisoned") = enabled;
        Ok(())
    }

    /// Register a new capability. Dependencies must already be registered.
    pub fn register(&self, capability: Capability) -> Result<CapabilityView, CapabilityError> {
        validate_id(&capability.id)?;
        {
            let defs = self.defs.read().expect("capability defs poisoned");
            if defs.contains_key(&capability.id) {
                return Err(CapabilityError::DuplicateId(capability.id.clone()));
            }
            for dependency in &capability.dependencies {
                if dependency == &capability.id {
                    return Err(CapabilityError::InvalidId(format!(
                        "self-dependency: {}",
                        capability.id
                    )));
                }
                if !defs.contains_key(dependency) {
                    return Err(CapabilityError::UnknownDependency {
                        id: capability.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        let enabled = capability.default_enabled;
        self.persist(&capability, enabled)?;
        self.enabled
            .write()
            .expect("capability state poisoned")
            .insert(capability.id.clone(), enabled);
        self.emit(
            "capability.registered",
            json!({
                "id": capability.id,
                "name": capability.name,
                "version": capability.version,
                "source": capability.source,
                "enabled": enabled,
            }),
        );
        let id = capability.id.clone();
        self.defs
            .write()
            .expect("capability defs poisoned")
            .insert(id.clone(), capability);
        self.view(&id)
    }

    /// Register every capability in `catalogue` that is not already present,
    /// using each definition's `default_enabled` for its initial state.
    ///
    /// The catalogue must be ordered so dependencies precede dependents.
    /// Returns the number of capabilities added.
    pub fn seed(&self, catalogue: &[Capability]) -> Result<usize, CapabilityError> {
        let mut added = 0;
        for capability in catalogue {
            if self.definition(&capability.id).is_ok() {
                continue;
            }
            self.register(capability.clone())?;
            added += 1;
        }
        Ok(added)
    }

    /// The definition of `id`, if registered.
    pub fn definition(&self, id: &str) -> Result<Capability, CapabilityError> {
        self.defs
            .read()
            .expect("capability defs poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| CapabilityError::UnknownCapability(id.to_string()))
    }

    /// Whether `id` is enabled. Unknown capabilities are treated as enabled so
    /// gating stays opt-in for commands that predate the registry.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled
            .read()
            .expect("capability state poisoned")
            .get(id)
            .copied()
            .unwrap_or(true)
    }

    /// Whether a toggle for `id` is pending a kernel restart.
    pub fn restart_required(&self, id: &str) -> bool {
        self.restart
            .read()
            .expect("capability restart set poisoned")
            .contains(id)
    }

    /// The guard used by command dispatch: refuse when `id` is disabled.
    pub fn require(&self, id: &str) -> Result<(), CapabilityError> {
        if self.is_enabled(id) {
            Ok(())
        } else {
            Err(CapabilityError::Disabled(id.to_string()))
        }
    }

    /// Every registered capability with its runtime state, ordered by id.
    pub fn list(&self) -> Vec<CapabilityView> {
        let mut views: Vec<CapabilityView> = self
            .defs
            .read()
            .expect("capability defs poisoned")
            .values()
            .map(|capability| self.view_of(capability))
            .collect();
        views.sort_by(|a, b| a.id.cmp(&b.id));
        views
    }

    /// The current view of `id`.
    pub fn view(&self, id: &str) -> Result<CapabilityView, CapabilityError> {
        let capability = self.definition(id)?;
        Ok(self.view_of(&capability))
    }

    /// The number of registered capabilities.
    pub fn count(&self) -> usize {
        self.defs.read().expect("capability defs poisoned").len()
    }

    /// Enable or disable `id`, enforcing the dependency graph.
    ///
    /// When enabling with `auto_enable_deps`, disabled dependencies are enabled
    /// first (recursively). When disabling, an enabled dependent blocks the
    /// change ([`CapabilityError::HasEnabledDependents`]).
    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        auto_enable_deps: bool,
    ) -> Result<CapabilityView, CapabilityError> {
        let capability = self.definition(id)?;
        if self.is_enabled(id) == enabled {
            return self.view(id);
        }
        if enabled {
            for dependency in capability.dependencies.clone() {
                if !self.is_enabled(&dependency) {
                    if auto_enable_deps {
                        self.set_enabled(&dependency, true, true)?;
                    } else {
                        return Err(CapabilityError::DependencyDisabled {
                            id: id.to_string(),
                            dependency,
                        });
                    }
                }
            }
        } else if let Some(dependent) = self.first_enabled_dependent(id) {
            return Err(CapabilityError::HasEnabledDependents {
                id: id.to_string(),
                dependent,
            });
        }

        self.write_state(id, enabled)?;
        self.emit(
            if enabled {
                "capability.enabled"
            } else {
                "capability.disabled"
            },
            json!({ "id": id }),
        );
        self.finish_toggle(id, enabled)
    }

    /// Mark `id` as pending a restart (used when its subsystem is not hot).
    pub fn clear_restart_required(&self, id: &str) {
        self.restart
            .write()
            .expect("capability restart set poisoned")
            .remove(id);
    }

    fn first_enabled_dependent(&self, id: &str) -> Option<String> {
        let defs = self.defs.read().expect("capability defs poisoned");
        let enabled = self.enabled.read().expect("capability state poisoned");
        let mut dependents: Vec<&String> = defs
            .values()
            .filter(|capability| {
                capability
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == id)
                    && enabled.get(&capability.id).copied().unwrap_or(false)
            })
            .map(|capability| &capability.id)
            .collect();
        dependents.sort();
        dependents.first().map(|id| (*id).clone())
    }

    fn finish_toggle(&self, id: &str, enabled: bool) -> Result<CapabilityView, CapabilityError> {
        let hot = self
            .definition(id)
            .map(|capability| capability.hot_toggle)
            .unwrap_or(false);
        let restart_required = if hot {
            self.clear_restart_required(id);
            false
        } else {
            self.restart
                .write()
                .expect("capability restart set poisoned")
                .insert(id.to_string());
            true
        };
        let view = self.view(id)?;
        self.emit(
            "capability.changed",
            json!({ "id": id, "enabled": enabled, "restart_required": restart_required }),
        );
        Ok(view)
    }

    fn view_of(&self, capability: &Capability) -> CapabilityView {
        let enabled = self.is_enabled(&capability.id);
        let restart_required = self.restart_required(&capability.id);
        CapabilityView::from_parts(capability, enabled, restart_required)
    }

    fn write_state(&self, id: &str, enabled: bool) -> Result<(), CapabilityError> {
        self.store.execute(
            "UPDATE capabilities SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::from(i64::from(enabled)),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        self.enabled
            .write()
            .expect("capability state poisoned")
            .insert(id.to_string(), enabled);
        Ok(())
    }

    fn persist(&self, capability: &Capability, enabled: bool) -> Result<(), CapabilityError> {
        let now = now_ms();
        self.store.execute(
            "INSERT INTO capabilities \
             (id, name, version, source, description, dependencies, resources, \
              default_enabled, enabled, hot_toggle, config, registered_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            &[
                Value::String(capability.id.clone()),
                Value::String(capability.name.clone()),
                Value::String(capability.version.clone()),
                Value::String(capability.source.clone()),
                capability
                    .description
                    .clone()
                    .map_or(Value::Null, Value::String),
                Value::String(serde_json::to_string(&capability.dependencies)?),
                Value::String(serde_json::to_string(&capability.resources)?),
                Value::from(i64::from(capability.default_enabled)),
                Value::from(i64::from(enabled)),
                Value::from(i64::from(capability.hot_toggle)),
                match &capability.config {
                    Some(config) => Value::String(serde_json::to_string(config)?),
                    None => Value::Null,
                },
                Value::from(now),
                Value::from(now),
            ],
        )?;
        Ok(())
    }

    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if let Err(err) = self
            .bus
            .publish(kind.to_string(), "kernel".to_string(), payload)
        {
            tracing::warn!(error = %err, kind, "failed to publish capability event");
        }
    }
}

fn parse_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

/// Validate a capability id: non-empty, `[a-z0-9.:-]`, not starting with a
/// separator.
fn validate_id(id: &str) -> Result<(), CapabilityError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | ':' | '-'))
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(CapabilityError::InvalidId(id.to_string()))
    }
}
