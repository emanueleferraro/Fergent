//! Fergents reversibility (`ARCHITECTURE.md` §10.3, §15).
//!
//! Every agent file mutation is preceded by a timestamped copy under
//! `<data_dir>/backups/<id>/`. Restores are audited, and every backup is
//! referenced from the tamper-evident `audit_log` (`revert_ref`), so a restore
//! can be replayed per file, per session, or per run.

mod error;
mod service;

pub use error::ReversibilityError;
pub use service::{Backup, BackupContext, ReversibilityService};
