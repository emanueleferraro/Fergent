//! Fergents brain model.
//!
//! A brain is a folder on disk with a `brain.toml` (identity + configuration)
//! and a `vault/` of markdown files, plus a regenerable `index.sqlite` and an
//! optional `graph.db` (`BRAIN.md` §2, `ARCHITECTURE.md` §8). This crate owns
//! `brain.toml`, the vault layout, the brain lifecycle, and multi-brain
//! operations.
//!
//! Phase 1 (Task 3) covers the `brain.toml` model, the vault layout, and
//! `.brainignore` matching; Task 14 adds the [`BrainManager`] lifecycle and
//! multi-brain operations.

mod config;
mod crossbrain;
mod error;
mod ignore;
mod indexing;
mod manager;
mod portable;
mod record;
mod roles;
mod store;
mod tmpl;
mod vault;

pub use config::{
    BrainConfig, BrainIdentity, GitConfig, GraphConfig, IndexConfig, MarkdownConfig, SearchConfig,
    SyncConfig, VaultConfig, BRAIN_TOML, SEARCH_MODES, SYNC_SCOPES,
};
pub use crossbrain::{parse_target, resolve, search_scope, Attachment, LinkTarget, Resolution};
pub use error::BrainError;
pub use ignore::{brainignore_path, BrainIgnore, BRAINIGNORE, BUILTIN_IGNORES};
pub use indexing::{BrainIndexer, IndexReport, VaultIndexer};
pub use manager::{BrainManager, BrainPatch, CreateBrainSpec};
pub use portable::{ImportSummary, Manifest, ManifestEntry, MANIFEST_NAME};
pub use record::BrainRecord;
pub use roles::{require_writable, BrainRole};
pub use store::{
    available_schema_version, register_sqlite_vec, BrainStore, BRAIN_MIGRATIONS_SUBDIR, INDEX_DB,
};
pub use tmpl::BrainTemplate;
pub use vault::{VaultLayout, BRAIN_META_DIR};
