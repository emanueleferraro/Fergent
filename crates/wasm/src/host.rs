//! The single host-call boundary.
//!
//! A component can only reach the kernel through [`HostBridge::call`]. The
//! implementation lives in `fergents-plugins`, where every operation is
//! checked against the plugin's grants and the kernel permission engine.

/// A host-call failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    /// A stable code (`plugin_permission_denied`, `plugin_host_failed`, ...).
    pub code: String,
    /// A human-readable message safe to show to the guest.
    pub message: String,
}

impl HostError {
    /// A denial.
    pub fn denied(message: impl Into<String>) -> Self {
        Self {
            code: "plugin_permission_denied".to_string(),
            message: message.into(),
        }
    }

    /// A generic host failure.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            code: "plugin_host_failed".to_string(),
            message: message.into(),
        }
    }

    /// An unsupported operation.
    pub fn unsupported(op: &str) -> Self {
        Self {
            code: "plugin_unsupported_op".to_string(),
            message: format!("host operation not supported: {op}"),
        }
    }
}

/// The host surface available to a component.
pub trait HostBridge: Send + Sync {
    /// Dispatch one operation with a JSON payload and return JSON.
    fn call(&self, op: &str, payload: &str) -> Result<String, HostError>;
}

/// A bridge that denies everything; the default for tests and dry runs.
#[derive(Debug, Default)]
pub struct DenyAllBridge;

impl HostBridge for DenyAllBridge {
    fn call(&self, op: &str, _payload: &str) -> Result<String, HostError> {
        Err(HostError::denied(format!("no host access for {op}")))
    }
}
