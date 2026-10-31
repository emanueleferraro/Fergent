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
//!
//! The modules are pure: no I/O beyond parsing, no clock unless one is passed
//! in, and no ambient state. Execution lives in the sibling modules added by
//! task 5.2.

pub mod document;
pub mod error;
pub mod expr;
pub mod graph;
pub mod schema;
pub mod template;
pub mod validate;

pub use document::{
    AppManifest, Edge, LoopBody, Node, NodeKind, PermissionsBlock, TriggerKind, TriggerSpec,
    WorkflowDocument,
};
pub use error::WorkflowError;
pub use expr::{apply_ops, Condition, TransformOp};
pub use graph::Graph;
pub use schema::SCHEMA_JSON;
pub use template::{Expr, RenderContext, SecretResolver, Template};
pub use validate::{validate, Issue, Validation};
