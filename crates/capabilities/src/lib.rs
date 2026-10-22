//! Fergents capability registry.
//!
//! Every product feature is a registered [`Capability`]. The kernel is the
//! authority for capability state: [`CapabilityRegistry`] tracks which
//! capabilities are enabled, enforces the dependency graph, persists state to
//! the global `capabilities` table, and emits `capability.*` events.
//!
//! A disabled capability is expected to allocate nothing: the subsystem that
//! owns it must not start tasks, register tools/commands/events, or open
//! pools. The registry is how the kernel decides — see `ARCHITECTURE.md` and
//! the Phase 2 spec (`capability registry`, decisions D1–D3).

mod catalogue;
mod error;
mod model;
mod registry;

pub use catalogue::default_catalogue;
pub use error::CapabilityError;
pub use model::{Capability, CapabilityView, DEFAULT_SOURCE};
pub use registry::CapabilityRegistry;
