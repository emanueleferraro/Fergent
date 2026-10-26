//! `plugin.toml`: the manifest model and validation (`PLUGINS.md` §3).

use std::collections::BTreeMap;
use std::path::Path;

use fergents_packages::api_compatible;
use serde::{Deserialize, Serialize};

use crate::error::PluginError;

/// The 16 UI slots of `UI.md` §22 (phase 4 API v1).
pub const SLOTS: [&str; 16] = [
    "rail.item",
    "sidebar.section",
    "main.tab",
    "editor.toolbar",
    "editor.gutter",
    "chat.message.actions",
    "chat.composer.extras",
    "chat.permission.card",
    "statusbar.item",
    "rightpanel.section",
    "dashboard.widget",
    "command.palette",
    "settings.section",
    "graph.overlay",
    "workflow.node",
    "theme.tokens",
];

/// Permission categories a plugin may request.
pub const PERMISSIONS: [&str; 12] = [
    "fs.read",
    "fs.write",
    "net.http",
    "llm.call",
    "mcp.connect",
    "events.subscribe",
    "events.emit",
    "storage.local",
    "secrets.get",
    "brain.read",
    "brain.write",
    "ui.notify",
];

/// A parsed permission scope: `true` (unscoped) or a list of scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionScope {
    /// Unscoped grant.
    Enabled(bool),
    /// Scoped grant.
    Scopes(Vec<String>),
}

impl PermissionScope {
    /// Parse a raw TOML permission value.
    pub fn from_toml(value: &toml::Value) -> Result<Self, String> {
        match value {
            toml::Value::Boolean(enabled) => Ok(PermissionScope::Enabled(*enabled)),
            toml::Value::Array(items) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "scopes must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()
                .map(PermissionScope::Scopes),
            _ => Err("permission must be `true` or a list of scopes".to_string()),
        }
    }

    /// The declared scopes, when the permission is scoped.
    pub fn scopes(&self) -> Option<&[String]> {
        match self {
            PermissionScope::Enabled(true) => None,
            PermissionScope::Enabled(false) => Some(&[]),
            PermissionScope::Scopes(scopes) => Some(scopes),
        }
    }
}

/// The `[plugin]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginMeta {
    /// Stable id, e.g. `com.example.rss-reader`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semver.
    pub version: String,
    /// Kernel API version (integer or numeric string).
    #[serde(default = "default_api_version", deserialize_with = "de_api_version")]
    pub api_version: u32,
    /// One-line description.
    #[serde(default)]
    pub description: Option<String>,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
    /// SPDX license.
    #[serde(default)]
    pub license: Option<String>,
    /// Homepage URL.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Repository URL.
    #[serde(default)]
    pub repository: Option<String>,
}

fn default_api_version() -> u32 {
    1
}

fn deserialize_permissions<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, toml::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    let mut out = BTreeMap::new();
    flatten_permissions("", &value, &mut out);
    Ok(out)
}

fn flatten_permissions(prefix: &str, value: &toml::Value, out: &mut BTreeMap<String, toml::Value>) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_permissions(&key, value, out);
            }
        }
        value => {
            if !prefix.is_empty() {
                out.insert(prefix.to_string(), value.clone());
            }
        }
    }
}

fn de_api_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    match value {
        toml::Value::Integer(value) => u32::try_from(value)
            .map_err(|_| serde::de::Error::custom("api_version must be a positive integer")),
        toml::Value::String(value) => value
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom("api_version must be an integer")),
        _ => Err(serde::de::Error::custom(
            "api_version must be an integer or numeric string",
        )),
    }
}

/// The `[kernel]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelMeta {
    /// Relative path to the WASM component.
    pub module: String,
    /// Optional entry label.
    #[serde(default)]
    pub entry: Option<String>,
}

/// The `[ui]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiMeta {
    /// Relative path to the UI module.
    pub entry: String,
    /// Optional stylesheet.
    #[serde(default)]
    pub stylesheet: Option<String>,
    /// Kernel commands this UI may call (besides its own tools).
    #[serde(default)]
    pub kernel_calls: Vec<String>,
    /// Event topics the UI may subscribe to.
    #[serde(default)]
    pub events: Vec<String>,
}

/// A `[[commands]]` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandMeta {
    /// Command id, unique within the plugin.
    pub id: String,
    /// Palette label.
    pub label: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional shortcut.
    #[serde(default)]
    pub shortcut: Option<String>,
    /// Declarative availability predicate.
    #[serde(default)]
    pub when: Option<serde_json::Value>,
}

/// A `[[capabilities]]` entry (spec §0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityMeta {
    /// Capability id, e.g. `capability.rss-digest`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
}

/// A `[[settings]]` entry rendered by Settings → Plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingMeta {
    /// Config key.
    pub key: String,
    /// Label.
    pub label: String,
    /// `string`, `number`, `boolean`, `select`, or `secret`.
    #[serde(default = "default_setting_kind")]
    pub kind: String,
    /// Default value.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Options for `select`.
    #[serde(default)]
    pub options: Vec<String>,
    /// Store the value in the secret vault.
    #[serde(default)]
    pub secret: bool,
}

fn default_setting_kind() -> String {
    "string".to_string()
}

/// The full manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// `[plugin]`
    pub plugin: PluginMeta,
    /// `[kernel]`
    #[serde(default)]
    pub kernel: Option<KernelMeta>,
    /// `[ui]`
    #[serde(default)]
    pub ui: Option<UiMeta>,
    /// `[permissions]` — raw TOML values (`true` or a list of scopes).
    ///
    /// Dotted keys are flattened, so `fs.write = [...]`, `[permissions.fs]
    /// write = [...]`, and `"fs.write" = [...]` are all accepted.
    #[serde(default, deserialize_with = "deserialize_permissions")]
    pub permissions: BTreeMap<String, toml::Value>,
    /// `slots = ["rail.item", ...]`
    #[serde(default)]
    pub slots: Vec<String>,
    /// `[[commands]]`
    #[serde(default)]
    pub commands: Vec<CommandMeta>,
    /// `[[capabilities]]`
    #[serde(default)]
    pub capabilities: Vec<CapabilityMeta>,
    /// `[[settings]]`
    #[serde(default)]
    pub settings: Vec<SettingMeta>,
    /// `[[workflow_nodes]]` (accepted, executed in Phase 5).
    #[serde(default)]
    pub workflow_nodes: Vec<toml::Value>,
}

impl PluginManifest {
    /// Parse and validate a manifest document.
    pub fn parse(text: &str) -> Result<Self, PluginError> {
        let manifest: PluginManifest =
            toml::from_str(text).map_err(|err| PluginError::InvalidManifest(err.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The plugin kind implied by its tables.
    pub fn kind(&self) -> &'static str {
        match (&self.kernel, &self.ui) {
            (Some(_), Some(_)) => "hybrid",
            (Some(_), None) => "kernel",
            (None, Some(_)) => "ui",
            (None, None) => "kernel",
        }
    }

    /// Validate ids, versions, permissions, slots, and paths.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.plugin.id.is_empty()
            || !self
                .plugin
                .id
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ".-_".contains(ch))
            || matches!(
                self.plugin.id.chars().next(),
                Some('.') | Some('-') | Some('_')
            )
        {
            return Err(PluginError::InvalidManifest(format!(
                "invalid plugin id: {:?}",
                self.plugin.id
            )));
        }
        fergents_packages::parse_version(&self.plugin.version)?;
        if !api_compatible(
            self.plugin.api_version,
            fergents_packages::KERNEL_API_VERSION,
        ) {
            return Err(PluginError::InvalidManifest(format!(
                "api_version {} is not supported by this kernel",
                self.plugin.api_version
            )));
        }
        if self.kernel.is_none() && self.ui.is_none() {
            return Err(PluginError::InvalidManifest(
                "a plugin must declare [kernel], [ui], or both".to_string(),
            ));
        }
        for (permission, scope) in &self.permissions {
            if !PERMISSIONS.contains(&permission.as_str()) {
                return Err(PluginError::InvalidManifest(format!(
                    "unknown permission: {permission}"
                )));
            }
            match PermissionScope::from_toml(scope) {
                Ok(PermissionScope::Enabled(false)) => {
                    return Err(PluginError::InvalidManifest(format!(
                        "permission {permission} is declared disabled"
                    )))
                }
                Ok(_) => {}
                Err(message) => {
                    return Err(PluginError::InvalidManifest(format!(
                        "permission {permission}: {message}"
                    )))
                }
            }
        }
        for slot in &self.slots {
            if !SLOTS.contains(&slot.as_str()) {
                return Err(PluginError::InvalidManifest(format!(
                    "unknown UI slot: {slot}"
                )));
            }
        }
        let mut commands = std::collections::HashSet::new();
        for command in &self.commands {
            if command.id.is_empty() || !commands.insert(command.id.clone()) {
                return Err(PluginError::InvalidManifest(format!(
                    "duplicate or empty command id: {}",
                    command.id
                )));
            }
        }
        let mut capabilities = std::collections::HashSet::new();
        for capability in &self.capabilities {
            if !capability.id.starts_with("capability.") || !capabilities.insert(&capability.id) {
                return Err(PluginError::InvalidManifest(format!(
                    "invalid or duplicate capability id: {}",
                    capability.id
                )));
            }
        }
        let mut settings = std::collections::HashSet::new();
        for setting in &self.settings {
            if !matches!(
                setting.kind.as_str(),
                "string" | "number" | "boolean" | "select" | "secret"
            ) {
                return Err(PluginError::InvalidManifest(format!(
                    "unknown setting kind: {}",
                    setting.kind
                )));
            }
            if !settings.insert(&setting.key) {
                return Err(PluginError::InvalidManifest(format!(
                    "duplicate setting key: {}",
                    setting.key
                )));
            }
        }
        Ok(())
    }

    /// Check that every declared file exists and stays inside `root`.
    pub fn validate_files(&self, root: &Path) -> Result<(), PluginError> {
        if let Some(kernel) = &self.kernel {
            require_file(root, &kernel.module)?;
        }
        if let Some(ui) = &self.ui {
            require_file(root, &ui.entry)?;
            if let Some(stylesheet) = &ui.stylesheet {
                require_file(root, stylesheet)?;
            }
        }
        Ok(())
    }
}

fn require_file(root: &Path, relative: &str) -> Result<(), PluginError> {
    if relative.is_empty() || Path::new(relative).is_absolute() {
        return Err(PluginError::InvalidManifest(format!(
            "missing or unsafe file: {relative}"
        )));
    }
    let canonical_root = root.canonicalize()?;
    let path = root.join(relative);
    let canonical = path
        .canonicalize()
        .map_err(|_| PluginError::InvalidManifest(format!("missing file: {relative}")))?;
    if canonical.is_file() && canonical.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(PluginError::InvalidManifest(format!(
            "missing or unsafe file: {relative}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
slots = ["rail.item", "sidebar.section"]

[plugin]
id = "com.example.rss-reader"
name = "RSS Reader"
version = "1.0.0"
api_version = "1"
description = "Fetch and read feeds."

[kernel]
module = "plugin.wasm"

[permissions]
fs.read = ["$PLUGIN_DATA/**"]
net.http = ["https://example.com/**"]
storage.local = true

[[commands]]
id = "refresh"
label = "Refresh feeds"

[[capabilities]]
id = "capability.rss-digest"
name = "RSS Digest"
"#;

    #[test]
    fn parses_a_valid_manifest() {
        let manifest = PluginManifest::parse(MINIMAL).unwrap();
        assert_eq!(manifest.plugin.id, "com.example.rss-reader");
        assert_eq!(manifest.kind(), "kernel");
        assert_eq!(manifest.permissions.len(), 3);
        assert_eq!(manifest.slots.len(), 2);
        assert_eq!(manifest.commands.len(), 1);
        assert_eq!(manifest.capabilities.len(), 1);
    }

    #[test]
    fn rejects_unknown_permissions() {
        let text = r#"
[plugin]
id = "x"
name = "X"
version = "1.0.0"

[kernel]
module = "x.wasm"

[permissions]
root = true
"#;
        assert!(PluginManifest::parse(text).is_err());
    }

    #[test]
    fn rejects_unknown_slots() {
        let text = r#"
slots = ["sidebar.rail"]

[plugin]
id = "x"
name = "X"
version = "1.0.0"

[ui]
entry = "ui.js"
"#;
        assert!(PluginManifest::parse(text).is_err());
    }

    #[test]
    fn rejects_incompatible_api() {
        let text = r#"
[plugin]
id = "x"
name = "X"
version = "1.0.0"
api_version = 9

[kernel]
module = "x.wasm"
"#;
        assert!(PluginManifest::parse(text).is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        let text = r#"
[plugin]
id = "x"
name = "X"
version = "1.0.0"

[kernel]
module = "../x.wasm"
"#;
        let manifest = PluginManifest::parse(text).unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(manifest.validate_files(dir.path()).is_err());
    }
}
