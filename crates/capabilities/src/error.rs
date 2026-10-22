//! Capability registry errors.

use fergents_events::EventError;
use fergents_storage::StorageError;

/// Errors raised by the capability registry.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// The capability id is empty or malformed.
    #[error("invalid capability id: {0}")]
    InvalidId(String),
    /// A capability with this id is already registered.
    #[error("capability already registered: {0}")]
    DuplicateId(String),
    /// No capability is registered with this id.
    #[error("capability not registered: {0}")]
    UnknownCapability(String),
    /// A dependency is not (yet) registered.
    #[error("capability {id} depends on unknown capability {dependency}")]
    UnknownDependency {
        /// The capability declaring the dependency.
        id: String,
        /// The missing dependency.
        dependency: String,
    },
    /// Enabling was refused because a dependency is disabled.
    #[error("capability {id} requires {dependency}, which is disabled")]
    DependencyDisabled {
        /// The capability being enabled.
        id: String,
        /// The disabled dependency.
        dependency: String,
    },
    /// Disabling was refused because an enabled capability depends on it.
    #[error("capability {id} still has enabled dependents ({dependent})")]
    HasEnabledDependents {
        /// The capability being disabled.
        id: String,
        /// One of the dependents keeping it enabled.
        dependent: String,
    },
    /// The capability is disabled, so its operations are refused.
    #[error("capability disabled: {0}")]
    Disabled(String),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl CapabilityError {
    /// A stable, machine-readable wire code (snake_case), used in error data.
    pub fn code(&self) -> &'static str {
        match self {
            CapabilityError::Storage(_) => "storage_error",
            CapabilityError::Event(_) => "event_error",
            CapabilityError::InvalidId(_) => "capability_invalid_id",
            CapabilityError::DuplicateId(_) => "capability_duplicate_id",
            CapabilityError::UnknownCapability(_) => "capability_unknown",
            CapabilityError::UnknownDependency { .. } => "capability_unknown_dependency",
            CapabilityError::DependencyDisabled { .. } => "capability_dependency_disabled",
            CapabilityError::HasEnabledDependents { .. } => "capability_has_dependents",
            CapabilityError::Disabled(_) => "capability_disabled",
            CapabilityError::Serde(_) => "capability_serde_error",
        }
    }
}
