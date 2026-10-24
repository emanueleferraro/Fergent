//! Git model types shared with the protocol.

use serde::{Deserialize, Serialize};

/// What happened to one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A new path.
    Added,
    /// Content changed.
    Modified,
    /// The path was removed.
    Deleted,
    /// The path was moved.
    Renamed,
    /// The file type changed (e.g. symlink to file).
    Typechange,
    /// The path has unresolved conflict markers.
    Conflicted,
    /// The path is untracked.
    Untracked,
}

impl ChangeKind {
    /// Wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Typechange => "typechange",
            Self::Conflicted => "conflicted",
            Self::Untracked => "untracked",
        }
    }
}

/// One changed path in the working tree or the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    /// Path relative to the repository root.
    pub path: String,
    /// Previous path, for renames.
    pub original_path: Option<String>,
    /// Kind of change.
    pub kind: ChangeKind,
    /// Whether the change is staged in the index.
    pub staged: bool,
}

/// The repository status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoStatus {
    /// Owning project.
    pub project_id: String,
    /// Current branch, if the HEAD is attached.
    pub branch: Option<String>,
    /// Current HEAD commit id.
    pub head: Option<String>,
    /// Whether HEAD is detached.
    pub detached: bool,
    /// Commits ahead of the upstream branch.
    pub ahead: u32,
    /// Commits behind the upstream branch.
    pub behind: u32,
    /// Whether the status has no entries.
    pub clean: bool,
    /// Changed paths.
    pub entries: Vec<StatusEntry>,
}

/// One line of a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    /// `context`, `add`, or `del`.
    pub kind: String,
    /// 1-based line in the old file, when applicable.
    pub old_line: Option<u32>,
    /// 1-based line in the new file, when applicable.
    pub new_line: Option<u32>,
    /// Line content without the leading origin marker or newline.
    pub content: String,
}

/// One hunk of a file diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    /// First old line.
    pub old_start: u32,
    /// Number of old lines.
    pub old_lines: u32,
    /// First new line.
    pub new_start: u32,
    /// Number of new lines.
    pub new_lines: u32,
    /// The `@@ ... @@` header text.
    pub header: String,
    /// The hunk lines.
    pub lines: Vec<DiffLine>,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// Path relative to the repository root.
    pub path: String,
    /// Previous path, for renames.
    pub original_path: Option<String>,
    /// Whether the diff is binary.
    pub binary: bool,
    /// Hunks (empty for binary files).
    pub hunks: Vec<DiffHunk>,
}

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    /// Branch name without `refs/heads/`.
    pub name: String,
    /// Whether this branch is checked out.
    pub is_head: bool,
    /// Configured upstream, e.g. `origin/main`.
    pub upstream: Option<String>,
    /// Commits ahead of the upstream.
    pub ahead: u32,
    /// Commits behind the upstream.
    pub behind: u32,
}

/// One commit in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    /// Full commit SHA.
    pub id: String,
    /// Abbreviated SHA.
    pub short_id: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_email: String,
    /// Author timestamp (seconds since the epoch).
    pub time: i64,
    /// First line of the commit message.
    pub summary: String,
}

/// A commit with its full message and diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitDetail {
    /// Commit metadata.
    pub commit: CommitInfo,
    /// Full commit message.
    pub message: String,
    /// Files changed by the commit.
    pub files: Vec<DiffFileSummary>,
}

/// A changed file in a commit, without line-level hunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFileSummary {
    /// Path relative to the repository root.
    pub path: String,
    /// Previous path, for renames.
    pub original_path: Option<String>,
    /// Lines added.
    pub additions: u32,
    /// Lines deleted.
    pub deletions: u32,
}

/// One blamed line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlameLine {
    /// 1-based line number.
    pub line: u32,
    /// Commit SHA that introduced the line.
    pub commit: String,
    /// Abbreviated SHA.
    pub short_commit: String,
    /// Author name.
    pub author: String,
    /// Author timestamp (seconds since the epoch).
    pub time: i64,
    /// Line content without the newline.
    pub content: String,
}

/// One conflicted path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFile {
    /// Path relative to the repository root.
    pub path: String,
    /// Whether a base (common ancestor) version exists.
    pub has_base: bool,
    /// Whether "ours" exists.
    pub has_ours: bool,
    /// Whether "theirs" exists.
    pub has_theirs: bool,
}

/// How to resolve a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveStrategy {
    /// Keep the current branch's version.
    Ours,
    /// Take the incoming branch's version.
    Theirs,
    /// Keep the working-tree file as-is and mark it resolved.
    MarkResolved,
}

impl ResolveStrategy {
    /// Wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ours => "ours",
            Self::Theirs => "theirs",
            Self::MarkResolved => "mark_resolved",
        }
    }

    /// Parse a wire value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ours" => Some(Self::Ours),
            "theirs" => Some(Self::Theirs),
            "mark_resolved" | "mark" => Some(Self::MarkResolved),
            _ => None,
        }
    }
}

/// The result of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitResult {
    /// Full commit SHA.
    pub id: String,
    /// Abbreviated SHA.
    pub short_id: String,
    /// First line of the commit message.
    pub summary: String,
    /// Trailers added for agent or Composer commits.
    pub trailers: Vec<String>,
}

/// The result of a remote operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteResult {
    /// The operation (`fetch`, `pull`, `push`).
    pub operation: String,
    /// Combined command output, trimmed.
    pub output: String,
}
