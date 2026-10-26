//! Persisted plugin records and API views.

use fergents_storage::{Row, Value};
use serde::{Deserialize, Serialize};

/// A row in the `plugins` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRecord {
    /// Stable plugin id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semver.
    pub version: String,
    /// Kernel API version.
    pub api_version: u32,
    /// `kernel`, `ui`, or `hybrid`.
    pub kind: String,
    /// `file`, `dir`, `git`, `marketplace`, or `dev`.
    pub source: String,
    /// Absolute installation directory.
    pub path: String,
    /// Digest of the manifest and files.
    pub manifest_hash: String,
    /// `verified`, `unknown_key`, `invalid`, `none`, or `dev`.
    pub signature_status: String,
    /// Whether the plugin is loaded.
    pub enabled: bool,
    /// Whether this is a linked development directory.
    pub dev: bool,
    /// Whether the plugin failed its last load.
    pub degraded: bool,
    /// Free-form configuration.
    pub config: Option<serde_json::Value>,
    /// Installed-at timestamp.
    pub installed_at: i64,
    /// Updated-at timestamp.
    pub updated_at: i64,
}

impl PluginRecord {
    /// Decode a row.
    pub fn from_row(row: &Row) -> Option<Self> {
        Some(Self {
            id: row.get("id").and_then(Value::as_str)?.to_string(),
            name: row.get("name").and_then(Value::as_str)?.to_string(),
            version: row.get("version").and_then(Value::as_str)?.to_string(),
            api_version: row.get("api_version").and_then(Value::as_i64).unwrap_or(1) as u32,
            kind: row.get("kind").and_then(Value::as_str)?.to_string(),
            source: row.get("source").and_then(Value::as_str)?.to_string(),
            path: row.get("path").and_then(Value::as_str)?.to_string(),
            manifest_hash: row
                .get("manifest_hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            signature_status: row
                .get("signature_status")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string(),
            enabled: row.get("enabled").and_then(Value::as_i64).unwrap_or(0) != 0,
            dev: row.get("dev").and_then(Value::as_i64).unwrap_or(0) != 0,
            degraded: row.get("degraded").and_then(Value::as_i64).unwrap_or(0) != 0,
            config: row
                .get("config")
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str(text).ok()),
            installed_at: row.get("installed_at").and_then(Value::as_i64).unwrap_or(0),
            updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        })
    }
}

/// A stored permission grant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionGrant {
    /// Category label (`fs.read`, `net.http`, ...).
    pub category: String,
    /// Declared scopes, when the grant is scoped.
    pub scope: Option<serde_json::Value>,
    /// Whether the user approved it.
    pub granted: bool,
}

/// A tool declared by a plugin component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Tool name, unique across plugins.
    pub name: String,
    /// Description shown to the model.
    pub description: String,
    /// JSON Schema for the arguments.
    pub parameters: serde_json::Value,
}

/// The API view of a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginView {
    /// The persisted record.
    #[serde(flatten)]
    pub record: PluginRecord,
    /// Granted permissions.
    pub permissions: Vec<PermissionGrant>,
    /// Declared tools (present when loaded).
    pub tools: Vec<ToolSpec>,
    /// UI slots.
    pub slots: Vec<String>,
    /// Commands.
    pub commands: Vec<crate::manifest::CommandMeta>,
    /// Plugin-provided capabilities.
    pub capabilities: Vec<crate::manifest::CapabilityMeta>,
    /// Last load error, when degraded.
    pub error: Option<String>,
}

/// An install review awaiting approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallReview {
    /// Review id (ULID), valid for ten minutes.
    pub review_id: String,
    /// The parsed manifest.
    pub manifest: crate::manifest::PluginManifest,
    /// Content digest.
    pub digest: String,
    /// Signature verification result.
    pub signature_status: String,
    /// Declared permissions.
    pub permissions: Vec<PermissionGrant>,
    /// Id or tool conflicts.
    pub conflicts: Vec<String>,
    /// Source label for the UI.
    pub source: String,
}

impl InstallReview {
    /// The signature status label.
    pub fn signature_label(&self) -> &str {
        &self.signature_status
    }
}
