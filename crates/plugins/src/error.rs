//! Plugin errors with stable codes (`spec.md` Appendix A).

use fergents_capabilities::CapabilityError;
use fergents_events::EventError;
use fergents_packages::PackageError;
use fergents_storage::StorageError;
use fergents_wasm::SandboxError;

/// Errors raised by the plugin service.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// A capability registry failure.
    #[error("capability error: {0}")]
    Capability(#[from] CapabilityError),
    /// A package validation failure.
    #[error("package error: {0}")]
    Package(#[from] PackageError),
    /// A sandbox failure.
    #[error("sandbox error: {0}")]
    Sandbox(#[from] SandboxError),
    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A malformed manifest.
    #[error("invalid plugin manifest: {0}")]
    InvalidManifest(String),
    /// The plugin does not exist.
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// The plugin is already installed.
    #[error("plugin already installed: {0}")]
    AlreadyInstalled(String),
    /// An install review expired or is unknown.
    #[error("install review not found or expired: {0}")]
    ReviewNotFound(String),
    /// The user did not approve a declared permission.
    #[error("permission not approved: {0}")]
    PermissionDenied(String),
    /// The plugin could not be loaded.
    #[error("plugin unavailable: {0}")]
    Unavailable(String),
    /// A tool id collides with another plugin or kernel tool.
    #[error("tool conflict: {0}")]
    ToolConflict(String),
    /// The requested source is not supported.
    #[error("unsupported source: {0}")]
    UnsupportedSource(String),
    /// A JSON document is malformed.
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
}

impl PluginError {
    /// A stable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            PluginError::Storage(_) => "storage_error",
            PluginError::Event(_) => "event_error",
            PluginError::Capability(_) => "capability_error",
            PluginError::Package(err) => err.code(),
            PluginError::Sandbox(err) => err.code(),
            PluginError::Io(_) => "plugin_io",
            PluginError::InvalidManifest(_) => "plugin_invalid_manifest",
            PluginError::NotFound(_) => "plugin_not_found",
            PluginError::AlreadyInstalled(_) => "plugin_already_installed",
            PluginError::ReviewNotFound(_) => "plugin_review_not_found",
            PluginError::PermissionDenied(_) => "plugin_permissions_denied",
            PluginError::Unavailable(_) => "plugin_unavailable",
            PluginError::ToolConflict(_) => "plugin_tool_conflict",
            PluginError::UnsupportedSource(_) => "plugin_source_unsupported",
            PluginError::Json(_) => "plugin_invalid_json",
        }
    }

    /// Wrap a host-side failure.
    pub fn host(message: impl Into<String>) -> PluginError {
        PluginError::Unavailable(message.into())
    }
}
