//! Fergents source control: git2 repositories, status, diffs, commits, history,
//! conflicts, remote operations, and shadow checkpoints (task 3.5).

mod cli;
mod error;
mod model;
mod repo;
mod service;
mod shadow;

pub use error::GitError;
pub use model::{
    BlameLine, Branch, ChangeKind, CommitDetail, CommitInfo, CommitResult, ConflictFile,
    DiffFileSummary, DiffHunk, DiffLine, FileDiff, RemoteResult, RepoStatus, ResolveStrategy,
    StatusEntry,
};
pub use service::{trailers_for, GitService, DEFAULT_LOG_LIMIT, MAX_LOG_LIMIT};
pub use shadow::ShadowRepo;
