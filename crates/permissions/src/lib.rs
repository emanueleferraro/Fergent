//! Fergents permission engine (`SECURITY.md` §3–§5, `ARCHITECTURE.md` §9).
//!
//! Three session modes (manual / auto / full), declarative TOML policy rules
//! evaluated in order (first match wins), per-agent overrides that can only
//! narrow, per-tool minimum levels, a permission-card payload, and complete
//! audit logging.

mod engine;
mod error;
mod model;
mod policy;

pub use engine::{PermissionService, PolicyGate};
pub use error::PermissionError;
pub use model::{
    Category, Decision, Mode, PermissionRequest, PermissionResult, Resolved, Strictness,
};
pub use policy::{Policy, PolicyRule};
