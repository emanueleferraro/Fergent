//! The Fergents workflow engine (spec `.trae/specs/workflow-phase-5/spec.md`,
//! `WORKFLOW.md`).
//!
//! This crate owns everything a workflow document needs before it runs:
//!
//! - [`document`] — the YAML/JSON document model and the typed node configs.
//! - [`schema`] — the shipped JSON Schema for editor completion and validation.
//! - [`template`] — the `{{ … }}` interpolation language (Appendix D).
//! - [`expr`] — the closed condition and transform language (§1.6).
//! - [`graph`] — topological ranks, scopes, and cycle detection (§1.1, §3.1).
//! - [`validate`] — the semantic rules of §1.1, §1.9, §1.8, and §5.1.
//! - [`context`] — the run context, spill-to-disk, and output mapping (§1.5).
//! - [`store`] — the persisted runs, steps, and checkpoints (§2).
//! - [`checkpoint`] — the versioned checkpoint envelope (§3.3).
//! - [`executor`] — one pure function per node kind (§1.2).
//! - [`engine`] — the driver that walks the graph and records the run (§3).
//! - [`trigger`] — the six trigger kinds and the schedule/watch/subscribe
//!   runtime that turns them into runs (§2).
//!
//! Everything except [`engine`] and [`trigger`] is pure: no I/O beyond
//! parsing, no clock unless one is passed in, and no ambient state. Those two
//! are the only modules that touch the store, the event bus, the filesystem,
//! a socket, or the host.

pub mod checkpoint;
pub mod context;
pub mod document;
pub mod engine;
pub mod error;
pub mod executor;
pub mod expr;
pub mod graph;
pub mod scheduler;
pub mod schema;
pub mod store;
pub mod template;
pub mod trigger;
pub mod validate;

pub use checkpoint::{Checkpointer, RestoredCheckpoint};
pub use context::RunContext;
pub use document::{
    AppManifest, Edge, LoopBody, Node, NodeKind, PermissionsBlock, TriggerKind, TriggerSpec,
    WorkflowDocument,
};
pub use engine::Engine;
pub use error::WorkflowError;
pub use executor::{
    AllowAll, BudgetSink, Cancellation, Execution, Gates, LoopRunner, NodeContext, NodeHost, Usage,
};
pub use expr::{apply_ops, Condition, TransformOp};
pub use graph::Graph;
pub use scheduler::{Decision, Outcome, Scheduler};
pub use schema::SCHEMA_JSON;
pub use store::{CheckpointRecord, NewRun, RunRecord, RunState, RunStore, StepRecord, StepState};
pub use template::{Expr, RenderContext, SecretResolver, Template};
pub use validate::{validate, Issue, Validation};
