//! Workflow errors.
//!
//! Every variant maps to a stable wire code (spec Appendix A). Codes are part
//! of the protocol contract: log messages may change, codes may not.

/// Errors raised by the workflow engine.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON document is malformed.
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    /// A YAML document is malformed.
    #[error("invalid yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    /// The document is structurally or semantically invalid.
    #[error("invalid workflow: {0}")]
    Invalid(String),
    /// The workflow does not exist.
    #[error("workflow not found: {0}")]
    NotFound(String),
    /// The operation conflicts with the workflow's current state.
    #[error("workflow conflict: {0}")]
    Conflict(String),
    /// The stored version does not match the document digest.
    #[error("workflow version conflict: {0}")]
    VersionConflict(String),
    /// The per-workflow run queue is full.
    #[error("workflow queue full: {0}")]
    QueueFull(String),
    /// A node type is not available (for example, its plugin is disabled).
    #[error("workflow node unavailable: {0}")]
    NodeUnavailable(String),
    /// A template references a context path that does not exist.
    #[error("template missing path: {0}")]
    TemplateMissing(String),
    /// A template is syntactically invalid.
    #[error("invalid template: {0}")]
    TemplateInvalid(String),
    /// The graph contains a cycle outside a loop body.
    #[error("workflow cycle: {0}")]
    Cycle(String),
    /// A loop exceeded its iteration limit.
    #[error("workflow iteration limit: {0}")]
    IterationLimit(String),
    /// A run exceeded one of its budgets.
    #[error("workflow budget exceeded: {0}")]
    BudgetExceeded(String),
    /// A run was interrupted and cannot continue without a resume.
    #[error("workflow interrupted: {0}")]
    Interrupted(String),
    /// A trigger rejected the caller's credentials.
    #[error("workflow trigger auth: {0}")]
    TriggerAuth(String),
    /// The run has no usable checkpoint and cannot be resumed.
    #[error("workflow run not resumable: {0}")]
    RunNotResumable(String),
    /// An expression failed to parse or evaluate.
    #[error("invalid expression: {0}")]
    ExpressionInvalid(String),
}

impl WorkflowError {
    /// A stable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            WorkflowError::Io(_) => "workflow_io",
            WorkflowError::Json(_) => "workflow_invalid_json",
            WorkflowError::Yaml(_) => "workflow_invalid_yaml",
            WorkflowError::Invalid(_) => "workflow_invalid",
            WorkflowError::NotFound(_) => "workflow_not_found",
            WorkflowError::Conflict(_) => "workflow_conflict",
            WorkflowError::VersionConflict(_) => "workflow_version_conflict",
            WorkflowError::QueueFull(_) => "workflow_queue_full",
            WorkflowError::NodeUnavailable(_) => "workflow_node_unavailable",
            WorkflowError::TemplateMissing(_) => "workflow_template_missing",
            WorkflowError::TemplateInvalid(_) => "workflow_template_invalid",
            WorkflowError::Cycle(_) => "workflow_cycle",
            WorkflowError::IterationLimit(_) => "workflow_iteration_limit",
            WorkflowError::BudgetExceeded(_) => "workflow_budget_exceeded",
            WorkflowError::Interrupted(_) => "workflow_interrupted",
            WorkflowError::TriggerAuth(_) => "workflow_trigger_auth",
            WorkflowError::RunNotResumable(_) => "workflow_run_not_resumable",
            WorkflowError::ExpressionInvalid(_) => "workflow_invalid",
        }
    }
}

impl From<WorkflowError> for String {
    fn from(err: WorkflowError) -> String {
        err.to_string()
    }
}
