//! Sandbox errors with stable codes.

/// Errors raised while loading or executing a component.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The engine could not be configured.
    #[error("sandbox engine: {0}")]
    Engine(String),
    /// The component is not a valid WebAssembly component.
    #[error("invalid component: {0}")]
    Invalid(String),
    /// An export is missing or has the wrong type.
    #[error("missing component export: {0}")]
    MissingExport(String),
    /// The component trapped.
    #[error("component trapped: {0}")]
    Trap(String),
    /// Fuel was exhausted.
    #[error("fuel exhausted")]
    FuelExhausted,
    /// The wall-time deadline was hit.
    #[error("component timed out")]
    Timeout,
    /// The component returned an error.
    #[error("component error: {0}")]
    Component(String),
    /// The input or output exceeded the configured limit.
    #[error("payload too large: {0}")]
    TooLarge(String),
    /// The host bridge denied the call.
    #[error("host call denied: {0}")]
    Denied(String),
    /// A host bridge or I/O failure.
    #[error("host call failed: {0}")]
    Host(String),
}

impl SandboxError {
    /// A stable code for the wire and the audit log.
    pub fn code(&self) -> &'static str {
        match self {
            SandboxError::Engine(_) => "sandbox_engine",
            SandboxError::Invalid(_) => "sandbox_invalid_component",
            SandboxError::MissingExport(_) => "sandbox_missing_export",
            SandboxError::Trap(_) => "plugin_failed",
            SandboxError::FuelExhausted => "plugin_fuel_exhausted",
            SandboxError::Timeout => "plugin_timeout",
            SandboxError::Component(_) => "plugin_failed",
            SandboxError::TooLarge(_) => "plugin_payload_too_large",
            SandboxError::Denied(_) => "plugin_permission_denied",
            SandboxError::Host(_) => "plugin_host_failed",
        }
    }
}
