//! The Composer backend (Phase 3 spec §7, Appendix A).
//!
//! A Composer run is a model-authored multi-file proposal. The kernel is
//! authoritative: the run stores the exact original and proposed text of every
//! file, `composer.apply` recomputes the accepted subset from those hunks, and
//! every write goes through [`FileService`] so etag conflicts, snapshots, and
//! `file.saved` events behave exactly like a manual save.
//!
//! The model is only ever asked for full file contents; the kernel diffs them
//! itself (`similar`), which is far more reliable than parsing model-authored
//! unified diffs.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Component, Path};
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use fergents_events::{now_ms, EventBus};
use fergents_ide::{language_id, FileService, ProjectError, ProjectService};
use fergents_llm::{ChatRequest, Message};
use fergents_protocol::{
    ComposerAppliedFile, ComposerFile, ComposerRun, ComposerState, GitDiffHunk, GitDiffLine,
    GitDiffLineKind,
};
use similar::{ChangeTag, TextDiff};

/// The most files one proposal may touch.
pub const MAX_PROPOSED_FILES: usize = 20;
/// The most context files one proposal may read.
pub const MAX_CONTEXT_FILES: usize = 8;
/// The total context budget, in characters.
pub const MAX_CONTEXT_CHARS: usize = 120_000;
/// The largest single proposed file, in characters.
pub const MAX_FILE_CHARS: usize = 400_000;

const COMPOSER_SYSTEM: &str = "\
You are the Fergents Composer, a precise senior software engineer.

You receive an instruction and the current contents of the relevant files.
Reply with JSON only, exactly this shape:

{\"message\": \"one short sentence describing the change\", \"files\": [{\"path\": \"relative/path.ext\", \"content\": \"the complete new file content\"}]}

Rules:
- `path` is project-relative, uses forward slashes, and never starts with `/` or `..`.
- `content` is the complete file after the change, never a fragment or a diff.
- Only include files you actually change. Keep unrelated code byte-identical.
- Return `\"files\": []` when no edit is needed (for example, an explanation).
- No markdown fences, no commentary outside the JSON object.";

/// The model surface the Composer and Inline AI need.
pub trait CompletionClient: Send + Sync {
    /// Run a completion and return the assistant text.
    fn complete<'a>(
        &'a self,
        model: &'a str,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;
}

/// Resolves the preferred model for a task (route first, then provider order).
pub type ModelResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Errors raised by the Composer.
#[derive(Debug, thiserror::Error)]
pub enum ComposerError {
    /// A project or file operation failed.
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// The event bus rejected a publication.
    #[error(transparent)]
    Event(#[from] fergents_events::EventError),
    /// The run id is unknown (or was evicted).
    #[error("unknown composer run: {0}")]
    UnknownRun(String),
    /// Only a proposed run can be applied, rejected, or cancelled.
    #[error("composer run {0} is {1}; only proposed runs can change")]
    NotProposed(String, ComposerState),
    /// No provider advertises a model for the task.
    #[error("no model configured for the {0} task")]
    NoModel(String),
    /// The provider call failed.
    #[error("model call failed: {0}")]
    Model(String),
    /// The model did not return a usable proposal.
    #[error("invalid proposal: {0}")]
    InvalidProposal(String),
    /// The caller accepted no hunks.
    #[error("no hunks accepted")]
    NothingAccepted,
    /// One or more writes were refused; nothing else is written.
    #[error("apply failed: {0}")]
    Apply(String),
}

/// One proposal request.
#[derive(Debug, Clone, Default)]
pub struct ProposalRequest {
    /// The project the proposal belongs to.
    pub project_id: String,
    /// The user's instruction.
    pub prompt: String,
    /// Files to include as context (in addition to `active_path`).
    pub paths: Vec<String>,
    /// The file the user is looking at.
    pub active_path: Option<String>,
    /// The selected text, when the request came from a selection.
    pub selection: Option<String>,
    /// Explicit model override.
    pub model: Option<String>,
}

/// A parsed model reply.
#[derive(Debug, Clone, serde::Deserialize)]
struct RawProposal {
    #[serde(default)]
    message: String,
    #[serde(default)]
    files: Vec<RawFile>,
}

/// One file in a parsed model reply.
#[derive(Debug, Clone, serde::Deserialize)]
struct RawFile {
    path: String,
    content: String,
}

/// Multi-file proposals, their diffs, and their approval state.
pub struct ComposerService {
    projects: Arc<ProjectService>,
    files: Arc<FileService>,
    bus: Arc<EventBus>,
    client: Arc<dyn CompletionClient>,
    model_for: ModelResolver,
    runs: RwLock<HashMap<String, ComposerRun>>,
}

impl ComposerService {
    /// Create the service.
    pub fn new(
        projects: Arc<ProjectService>,
        files: Arc<FileService>,
        bus: Arc<EventBus>,
        client: Arc<dyn CompletionClient>,
        model_for: ModelResolver,
    ) -> Self {
        Self {
            projects,
            files,
            bus,
            client,
            model_for,
            runs: RwLock::new(HashMap::new()),
        }
    }

    /// Ask the model for a proposal, diff it, and store the run.
    pub async fn propose(&self, request: ProposalRequest) -> Result<ComposerRun, ComposerError> {
        self.projects.require_trusted(&request.project_id)?;
        let model = request
            .model
            .clone()
            .or_else(|| (self.model_for)("composer"))
            .ok_or_else(|| ComposerError::NoModel("composer".to_string()))?;
        let context = self.context_files(&request);
        let prompt = build_prompt(&request, &context);
        let chat = ChatRequest {
            model: model.clone(),
            messages: vec![Message::system(COMPOSER_SYSTEM), Message::user(prompt)],
            tools: Vec::new(),
            temperature: Some(0.2),
            max_tokens: Some(4096),
            stream: false,
        };
        let raw = self
            .client
            .complete(&model, chat)
            .await
            .map_err(ComposerError::Model)?;
        let parsed = parse_proposal(&raw)?;
        let files = self.build_files(&request.project_id, parsed.files)?;
        let additions = files.iter().map(|file| file.additions).sum();
        let deletions = files.iter().map(|file| file.deletions).sum();
        let now = now_ms();
        let run = ComposerRun {
            id: ulid::Ulid::new().to_string(),
            project_id: request.project_id.clone(),
            prompt: request.prompt.clone(),
            message: parsed.message.trim().to_string(),
            state: ComposerState::Proposed,
            model,
            files,
            additions,
            deletions,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.store(&run)?;
        self.bus.publish(
            "composer.proposed",
            "composer",
            serde_json::json!({
                "run_id": run.id,
                "project_id": run.project_id,
                "files": run.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
                "additions": run.additions,
                "deletions": run.deletions,
            }),
        )?;
        Ok(run)
    }

    /// Fetch one run.
    pub fn get(&self, run_id: &str) -> Result<ComposerRun, ComposerError> {
        self.runs
            .read()
            .expect("composer runs lock")
            .get(run_id)
            .cloned()
            .ok_or_else(|| ComposerError::UnknownRun(run_id.to_string()))
    }

    /// Write exactly the accepted hunks of every file, then mark the run applied.
    pub fn apply(
        &self,
        run_id: &str,
        accepted: &[String],
    ) -> Result<(ComposerRun, Vec<ComposerAppliedFile>), ComposerError> {
        let mut run = self.get(run_id)?;
        if run.state != ComposerState::Proposed {
            return Err(ComposerError::NotProposed(run.id.clone(), run.state));
        }
        let accepted: HashSet<String> = accepted.iter().cloned().collect();
        let mut applied = Vec::new();
        let mut failures = Vec::new();
        for file in &run.files {
            let ids: Vec<String> = file
                .hunks
                .iter()
                .enumerate()
                .map(|(index, _)| format!("{}:{index}", file.path))
                .collect();
            if !ids.iter().any(|id| accepted.contains(id)) {
                continue;
            }
            let content = apply_hunks(
                &file.path,
                &file.original,
                &file.hunks,
                &accepted,
                file.proposed.ends_with('\n'),
            );
            match self.write(&run.project_id, file, &content) {
                Ok(etag) => applied.push(ComposerAppliedFile {
                    path: file.path.clone(),
                    etag,
                }),
                Err(reason) => failures.push(format!("{}: {reason}", file.path)),
            }
        }
        if !failures.is_empty() {
            run.state = ComposerState::Failed;
            run.error = Some(failures.join("; "));
            run.updated_at = now_ms();
            self.store(&run)?;
            self.bus.publish(
                "composer.failed",
                "composer",
                serde_json::json!({ "run_id": run.id, "error": run.error }),
            )?;
            return Err(ComposerError::Apply(failures.join("; ")));
        }
        if applied.is_empty() {
            return Err(ComposerError::NothingAccepted);
        }
        run.state = ComposerState::Applied;
        run.error = None;
        run.updated_at = now_ms();
        self.store(&run)?;
        self.bus.publish(
            "composer.applied",
            "composer",
            serde_json::json!({
                "run_id": run.id,
                "project_id": run.project_id,
                "paths": applied.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
            }),
        )?;
        Ok((run, applied))
    }

    /// Discard a proposal; nothing is written.
    pub fn reject(&self, run_id: &str) -> Result<ComposerRun, ComposerError> {
        let mut run = self.get(run_id)?;
        if run.state == ComposerState::Proposed {
            run.state = ComposerState::Rejected;
            run.updated_at = now_ms();
            self.store(&run)?;
            self.bus.publish(
                "composer.rejected",
                "composer",
                serde_json::json!({ "run_id": run.id, "project_id": run.project_id }),
            )?;
        }
        Ok(run)
    }

    /// Cancel a proposal; returns whether anything changed.
    pub fn cancel(&self, run_id: &str) -> Result<bool, ComposerError> {
        let run = self.get(run_id)?;
        if run.state != ComposerState::Proposed {
            return Ok(false);
        }
        self.reject(run_id)?;
        Ok(true)
    }

    /// Everything a prompt needs: the reading order and the file contents.
    fn context_files(&self, request: &ProposalRequest) -> Vec<(String, String)> {
        let mut paths: Vec<String> = Vec::new();
        if let Some(active) = &request.active_path {
            paths.push(active.clone());
        }
        for path in &request.paths {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        let mut files = Vec::new();
        let mut total = 0usize;
        for path in paths {
            if files.len() >= MAX_CONTEXT_FILES {
                break;
            }
            let Ok(snapshot) = self.files.read(&request.project_id, &path) else {
                continue;
            };
            if snapshot.binary {
                continue;
            }
            total += snapshot.content.len();
            if total > MAX_CONTEXT_CHARS {
                break;
            }
            files.push((snapshot.path, snapshot.content));
        }
        files
    }

    /// Diff every proposed file against what is on disk right now.
    fn build_files(
        &self,
        project_id: &str,
        proposed: Vec<RawFile>,
    ) -> Result<Vec<ComposerFile>, ComposerError> {
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        for file in proposed {
            if files.len() >= MAX_PROPOSED_FILES {
                break;
            }
            let path = relative_path(&file.path)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            if file.content.len() > MAX_FILE_CHARS {
                return Err(ComposerError::InvalidProposal(format!(
                    "{} is larger than {} characters",
                    path, MAX_FILE_CHARS
                )));
            }
            let (original, language) = match self.files.read(project_id, &path) {
                Ok(snapshot) if !snapshot.binary => (snapshot.content, snapshot.language_id),
                Ok(_) => return Err(ComposerError::InvalidProposal(format!("{path} is binary"))),
                Err(ProjectError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                    (String::new(), language_id(Path::new(&path)).to_string())
                }
                Err(err) => return Err(ComposerError::Project(err)),
            };
            if original == file.content {
                continue;
            }
            let hunks = diff_hunks(&original, &file.content);
            let additions = hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .filter(|line| matches!(line.kind, GitDiffLineKind::Add))
                .count() as u64;
            let deletions = hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .filter(|line| matches!(line.kind, GitDiffLineKind::Del))
                .count() as u64;
            files.push(ComposerFile {
                path,
                language_id: language,
                original,
                proposed: file.content,
                binary: false,
                hunks,
                additions,
                deletions,
            });
        }
        Ok(files)
    }

    /// Write one file, guarding against changes made after the proposal.
    fn write(
        &self,
        project_id: &str,
        file: &ComposerFile,
        content: &str,
    ) -> Result<String, String> {
        match self.files.read(project_id, &file.path) {
            Ok(snapshot) if snapshot.content == file.original => self
                .files
                .write(project_id, &file.path, content, Some(&snapshot.etag))
                .map(|saved| saved.etag)
                .map_err(|err| err.to_string()),
            Ok(_) => Err("the file changed after the proposal".to_string()),
            Err(ProjectError::Io(err))
                if err.kind() == std::io::ErrorKind::NotFound && file.original.is_empty() =>
            {
                self.files
                    .create(project_id, &file.path, content)
                    .map(|saved| saved.etag)
                    .map_err(|err| err.to_string())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn store(&self, run: &ComposerRun) -> Result<(), ComposerError> {
        self.runs
            .write()
            .expect("composer runs lock")
            .insert(run.id.clone(), run.clone());
        Ok(())
    }
}

/// The stable hunk id used by the UI: `<path>:<index>`.
pub fn hunk_id(path: &str, index: usize) -> String {
    format!("{path}:{index}")
}

/// Apply the accepted hunks to `original` (mirror of the UI model).
///
/// `trailing_newline` restores the final newline the hunk text cannot carry
/// (lines are stored without their terminator), so a fully accepted proposal
/// writes exactly the text the model returned.
pub fn apply_hunks(
    path: &str,
    original: &str,
    hunks: &[GitDiffHunk],
    accepted: &HashSet<String>,
    trailing_newline: bool,
) -> String {
    let lines: Vec<&str> = if original.is_empty() {
        Vec::new()
    } else {
        original.split('\n').collect()
    };
    let mut ordered: Vec<(usize, &GitDiffHunk)> = hunks.iter().enumerate().collect();
    ordered.sort_by_key(|(_, hunk)| hunk.old_start);
    let mut output: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for (index, hunk) in ordered {
        // `old_lines == 0` inserts at `old_start`; otherwise the first old line
        // is 1-based.
        let start = if hunk.old_lines == 0 {
            hunk.old_start as usize
        } else {
            hunk.old_start.saturating_sub(1) as usize
        };
        if start < cursor || start > lines.len() {
            continue;
        }
        if start > cursor {
            output.extend(lines[cursor..start].iter().map(|line| (*line).to_string()));
        }
        if accepted.contains(&hunk_id(path, index)) {
            output.extend(
                hunk.lines
                    .iter()
                    .filter(|line| !matches!(line.kind, GitDiffLineKind::Del))
                    .map(|line| line.content.clone()),
            );
        } else {
            let end = (start + hunk.old_lines as usize).min(lines.len());
            output.extend(lines[start..end].iter().map(|line| (*line).to_string()));
        }
        cursor = start + hunk.old_lines as usize;
    }
    output.extend(
        lines[cursor.min(lines.len())..]
            .iter()
            .map(|line| (*line).to_string()),
    );
    let mut text = output.join("\n");
    if trailing_newline && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Turn two texts into unified hunks with three lines of context.
pub fn diff_hunks(original: &str, proposed: &str) -> Vec<GitDiffHunk> {
    let diff = TextDiff::from_lines(original, proposed);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        let mut lines = Vec::new();
        let mut old_lines = 0u64;
        let mut new_lines = 0u64;
        for op in &group {
            for change in op.iter_changes(diff.old_slices(), diff.new_slices()) {
                let (kind, old_line, new_line) = match change.tag() {
                    ChangeTag::Equal => {
                        old_lines += 1;
                        new_lines += 1;
                        (
                            GitDiffLineKind::Context,
                            change.old_index().map(|i| i as u64 + 1),
                            change.new_index().map(|i| i as u64 + 1),
                        )
                    }
                    ChangeTag::Delete => {
                        old_lines += 1;
                        (
                            GitDiffLineKind::Del,
                            change.old_index().map(|i| i as u64 + 1),
                            None,
                        )
                    }
                    ChangeTag::Insert => {
                        new_lines += 1;
                        (
                            GitDiffLineKind::Add,
                            None,
                            change.new_index().map(|i| i as u64 + 1),
                        )
                    }
                };
                lines.push(GitDiffLine {
                    kind,
                    old_line,
                    new_line,
                    content: change.value().trim_end_matches('\n').to_string(),
                });
            }
        }
        let Some(first) = group.first() else {
            continue;
        };
        let old_start = if old_lines == 0 {
            first.old_range().start as u64
        } else {
            first.old_range().start as u64 + 1
        };
        let new_start = first.new_range().start as u64 + 1;
        hunks.push(GitDiffHunk {
            header: format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
            lines,
            old_start,
            old_lines,
            new_start,
            new_lines,
        });
    }
    hunks
}

/// Reject absolute paths and `..` escapes; normalise `\` and `./`.
fn relative_path(raw: &str) -> Result<String, ComposerError> {
    let cleaned = raw.trim().replace('\\', "/");
    let cleaned = cleaned.strip_prefix("./").unwrap_or(&cleaned);
    let invalid =
        |reason: &str| ComposerError::InvalidProposal(format!("invalid path {raw:?}: {reason}"));
    if cleaned.is_empty() {
        return Err(invalid("empty"));
    }
    if cleaned.starts_with('/') {
        return Err(invalid("absolute paths are not allowed"));
    }
    for component in Path::new(cleaned).components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(invalid("only normal segments are allowed")),
        }
    }
    Ok(cleaned.to_string())
}

/// Extract the JSON object from a reply that may be fenced or padded.
fn parse_proposal(raw: &str) -> Result<RawProposal, ComposerError> {
    let trimmed = raw.trim();
    let body = if let Some(start) = trimmed.find("```") {
        let rest = &trimmed[start + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        match rest.rfind("```") {
            Some(end) => &rest[..end],
            None => rest,
        }
    } else {
        trimmed
    };
    let start = body
        .find('{')
        .ok_or_else(|| ComposerError::InvalidProposal("no JSON object".to_string()))?;
    let end = body
        .rfind('}')
        .ok_or_else(|| ComposerError::InvalidProposal("no JSON object".to_string()))?;
    if end < start {
        return Err(ComposerError::InvalidProposal("no JSON object".to_string()));
    }
    serde_json::from_str(&body[start..=end])
        .map_err(|err| ComposerError::InvalidProposal(err.to_string()))
}

/// The user message: instruction, selection, and context files.
fn build_prompt(request: &ProposalRequest, context: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str("Instruction:\n");
    out.push_str(request.prompt.trim());
    out.push('\n');
    if let Some(selection) = &request.selection {
        if !selection.trim().is_empty() {
            out.push_str("\nSelected text:\n```\n");
            out.push_str(selection);
            out.push_str("\n```\n");
        }
    }
    if !context.is_empty() {
        out.push_str("\nCurrent files:\n");
        for (path, content) in context {
            out.push_str(&format!("\n<file path=\"{path}\">\n{content}\n</file>\n"));
        }
    }
    out.push_str("\nReturn the JSON object now.");
    out
}
