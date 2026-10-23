//! Project model types.

use serde::{Deserialize, Serialize};

/// Whether a project may run code (LSP servers, terminals, git hooks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustState {
    /// Restricted mode: nothing executable runs.
    Untrusted,
    /// The user trusts this project.
    Trusted,
}

impl TrustState {
    /// The database/wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::Trusted => "trusted",
        }
    }

    /// Parse a stored value; anything unknown is untrusted (fail closed).
    pub fn parse(value: &str) -> Self {
        match value {
            "trusted" => Self::Trusted,
            _ => Self::Untrusted,
        }
    }
}

/// One opened code workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// ULID.
    pub id: String,
    /// Canonical absolute path of the opened folder.
    pub path: String,
    /// Display name (folder name by default).
    pub name: String,
    /// Detected version control (`git` or `None`).
    pub vcs: Option<String>,
    /// An optional brain linked to this project.
    pub brain_id: Option<String>,
    /// Trust state (spec decision D3).
    pub trust: TrustState,
    /// Free-form settings.
    pub settings: Option<serde_json::Value>,
    /// Last time the project was opened (ms since the epoch).
    pub last_opened_at: i64,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
}

impl Project {
    /// Whether the project is trusted.
    pub fn is_trusted(&self) -> bool {
        self.trust == TrustState::Trusted
    }
}

/// One text-search hit inside a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Project-relative path (forward slashes).
    pub path: String,
    /// 1-based line number.
    pub line: u32,
    /// The matching line, trimmed and capped.
    pub text: String,
}
