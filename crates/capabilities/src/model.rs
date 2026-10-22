//! Capability model types.

use serde::{Deserialize, Serialize};

/// The default `source` for capabilities registered by the kernel itself.
pub const DEFAULT_SOURCE: &str = "kernel";

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_source() -> String {
    DEFAULT_SOURCE.to_string()
}

/// A capability definition (the static, declarative part).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    /// Namespaced, stable id, e.g. `capability.charts`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semver-ish version string.
    #[serde(default = "default_version")]
    pub version: String,
    /// Origin: `kernel` or `plugin:<id>`.
    #[serde(default = "default_source")]
    pub source: String,
    /// Short description for the Settings → Features page.
    #[serde(default)]
    pub description: Option<String>,
    /// Ids this capability depends on (a DAG).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Declared resource labels this capability allocates when enabled.
    #[serde(default)]
    pub resources: Vec<String>,
    /// Whether the capability is enabled when first seeded.
    #[serde(default)]
    pub default_enabled: bool,
    /// Whether a toggle can be applied without a kernel restart.
    #[serde(default)]
    pub hot_toggle: bool,
    /// Optional free-form configuration.
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

/// A capability with its current runtime state, as returned to clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityView {
    /// Namespaced id.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Origin.
    pub source: String,
    /// Short description.
    pub description: Option<String>,
    /// Dependency ids.
    pub dependencies: Vec<String>,
    /// Declared resource labels.
    pub resources: Vec<String>,
    /// Whether it is currently enabled.
    pub enabled: bool,
    /// Whether it is enabled by default.
    pub default_enabled: bool,
    /// Whether a toggle is hot.
    pub hot_toggle: bool,
    /// Set when a toggle was applied but needs a restart to take effect.
    pub restart_required: bool,
    /// Optional configuration.
    pub config: Option<serde_json::Value>,
}

impl CapabilityView {
    /// Build a view from a definition plus its runtime state.
    pub(crate) fn from_parts(
        capability: &Capability,
        enabled: bool,
        restart_required: bool,
    ) -> Self {
        Self {
            id: capability.id.clone(),
            name: capability.name.clone(),
            version: capability.version.clone(),
            source: capability.source.clone(),
            description: capability.description.clone(),
            dependencies: capability.dependencies.clone(),
            resources: capability.resources.clone(),
            enabled,
            default_enabled: capability.default_enabled,
            hot_toggle: capability.hot_toggle,
            restart_required,
            config: capability.config.clone(),
        }
    }
}
