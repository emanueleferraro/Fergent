//! Skill errors.

use fergents_packages::PackageError;

/// Errors raised by the skill runtime.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] fergents_storage::StorageError),
    /// A package failure.
    #[error("package error: {0}")]
    Package(#[from] PackageError),
    /// A sandbox failure.
    #[error("sandbox error: {0}")]
    Sandbox(#[from] fergents_wasm::SandboxError),
    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A malformed skill frontmatter or folder.
    #[error("invalid skill: {0}")]
    Invalid(String),
    /// The skill does not exist.
    #[error("skill not found: {0}")]
    NotFound(String),
    /// The skill is disabled.
    #[error("skill disabled: {0}")]
    Disabled(String),
    /// Input does not match the declared schema.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Output does not match the declared schema.
    #[error("invalid output: {0}")]
    InvalidOutput(String),
    /// The skill lacks a declared permission.
    #[error("skill permission denied: {0}")]
    PermissionDenied(String),
    /// Chaining exceeded the depth limit or formed a cycle.
    #[error("skill chain rejected: {0}")]
    Depth(String),
    /// A model or tool call failed.
    #[error("skill execution failed: {0}")]
    Failed(String),
    /// A JSON document is malformed.
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
}

impl SkillError {
    /// A stable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            SkillError::Storage(_) => "storage_error",
            SkillError::Package(err) => err.code(),
            SkillError::Sandbox(err) => err.code(),
            SkillError::Io(_) => "skill_io",
            SkillError::Invalid(_) => "skill_invalid",
            SkillError::NotFound(_) => "skill_not_found",
            SkillError::Disabled(_) => "skill_disabled",
            SkillError::InvalidInput(_) => "skill_invalid_input",
            SkillError::InvalidOutput(_) => "skill_invalid_output",
            SkillError::PermissionDenied(_) => "skill_permission_denied",
            SkillError::Depth(_) => "skill_depth_exceeded",
            SkillError::Failed(_) => "skill_failed",
            SkillError::Json(_) => "skill_invalid_json",
        }
    }
}
