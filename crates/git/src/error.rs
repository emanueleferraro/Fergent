//! Git error types.

use fergents_ide::ProjectError;

/// Errors returned by the git service.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] fergents_storage::StorageError),
    /// Event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// Project-layer failure (unknown project, untrusted, escape).
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// Filesystem failure.
    #[error("git io error: {0}")]
    Io(#[from] std::io::Error),
    /// The project folder is not a git repository.
    #[error("not a git repository: {0}")]
    NotRepository(String),
    /// libgit2 failure.
    #[error("git error: {0}")]
    Git(String),
    /// Invalid request parameters.
    #[error("invalid git request: {0}")]
    Invalid(String),
    /// Remote operations are not enabled for this service.
    #[error("remote git operations are disabled")]
    RemoteDisabled,
    /// The `git` CLI failed.
    #[error("git command failed: {0}")]
    Cli(String),
}

impl GitError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "storage_error",
            Self::Event(_) => "event_error",
            Self::Project(error) => error.code(),
            Self::Io(_) => "git_io_error",
            Self::NotRepository(_) => "git_not_repository",
            Self::Git(_) => "git_error",
            Self::Invalid(_) => "git_invalid",
            Self::RemoteDisabled => "git_remote_disabled",
            Self::Cli(_) => "git_cli_error",
        }
    }
}

impl From<git2::Error> for GitError {
    fn from(error: git2::Error) -> Self {
        Self::Git(error.message().to_string())
    }
}
