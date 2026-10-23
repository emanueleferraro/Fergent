//! File operations inside a project: reads with etags, conflict-checked
//! writes, reversible mutations, and the external-change watcher
//! (task 3.2, spec decision D2).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fergents_events::EventBus;
use fergents_reversibility::{BackupContext, ReversibilityService};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::ProjectError;
use crate::service::ProjectService;

/// Content is returned in full up to this size; beyond it `truncated` is set.
pub const MAX_READ_BYTES: u64 = 5 * 1024 * 1024;
/// Files above this size are neither hashed nor read (a safety valve).
const MAX_HASH_BYTES: u64 = 64 * 1024 * 1024;
/// Bytes inspected to decide whether a file is binary.
const BINARY_PROBE_BYTES: usize = 8 * 1024;
/// Quiet period before coalesced watcher events are published.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// A file read from a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Project-relative path (forward slashes).
    pub path: String,
    /// The file content (empty when `binary`).
    pub content: String,
    /// `sha256(content)` as lowercase hex; empty when `binary`.
    pub etag: String,
    /// Size in bytes on disk.
    pub size: i64,
    /// The Monaco language id for this path.
    pub language_id: String,
    /// Whether the content was cut at [`MAX_READ_BYTES`].
    pub truncated: bool,
    /// Whether the file looks binary (NUL byte in the probe window).
    pub binary: bool,
}

/// One entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// File name (no path).
    pub name: String,
    /// Project-relative path (forward slashes).
    pub path: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    /// Size in bytes; 0 for directories.
    pub size: i64,
}

/// Metadata for one path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// Project-relative path.
    pub path: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    /// Size in bytes; 0 for directories.
    pub size: i64,
    /// Last modification time (ms since the epoch).
    pub modified_at: i64,
    /// The content etag for files; empty for directories.
    pub etag: String,
}

/// Project-scoped file operations.
pub struct FileService {
    projects: Arc<ProjectService>,
    reversibility: Arc<ReversibilityService>,
    bus: Arc<EventBus>,
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

impl FileService {
    /// Create the service.
    pub fn new(
        projects: Arc<ProjectService>,
        reversibility: Arc<ReversibilityService>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            projects,
            reversibility,
            bus,
            watchers: Mutex::new(HashMap::new()),
        }
    }

    /// Read a file, returning its content and etag.
    ///
    /// Reads are allowed in restricted (untrusted) projects.
    pub fn read(&self, project_id: &str, rel: &str) -> Result<FileSnapshot, ProjectError> {
        let project = self.projects.require(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        let metadata = std::fs::metadata(&path)?;
        if metadata.is_dir() {
            return Err(ProjectError::Invalid(format!("{rel} is a directory")));
        }
        let file = read_text(&path)?;
        Ok(FileSnapshot {
            path: normalise(rel),
            content: file.content,
            etag: file.etag,
            size: metadata.len() as i64,
            language_id: language_id(&path).to_string(),
            truncated: file.truncated,
            binary: file.binary,
        })
    }

    /// Write a file.
    ///
    /// When `base_etag` is given and no longer matches the file on disk, the
    /// write is refused with `file_conflict` and nothing is changed. A
    /// `base_etag` for a missing file is also a conflict. Mutations require a
    /// trusted project.
    pub fn write(
        &self,
        project_id: &str,
        rel: &str,
        content: &str,
        base_etag: Option<&str>,
    ) -> Result<FileSnapshot, ProjectError> {
        let project = self.projects.require_trusted(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        if path.is_dir() {
            return Err(ProjectError::Invalid(format!("{rel} is a directory")));
        }
        let current = etag_of(&path)?;
        if let Some(base) = base_etag {
            match current {
                Some(actual) if actual == base => {}
                _ => return Err(ProjectError::Conflict(rel.to_string())),
            }
        }

        let context = BackupContext::new();
        self.reversibility.snapshot(&path, &context)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&path, content.as_bytes())?;

        let snapshot = FileSnapshot {
            path: normalise(rel),
            content: content.to_string(),
            etag: sha256_hex(content.as_bytes()),
            size: content.len() as i64,
            language_id: language_id(&path).to_string(),
            truncated: false,
            binary: false,
        };
        self.bus.publish(
            "file.saved",
            "ide",
            json!({
                "project_id": project.id,
                "path": snapshot.path,
                "etag": snapshot.etag,
                "size": snapshot.size,
            }),
        )?;
        Ok(snapshot)
    }

    /// Create a new file; an existing path is a conflict.
    pub fn create(
        &self,
        project_id: &str,
        rel: &str,
        content: &str,
    ) -> Result<FileSnapshot, ProjectError> {
        let project = self.projects.require_trusted(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        if path.exists() {
            return Err(ProjectError::Conflict(rel.to_string()));
        }
        self.write(project_id, rel, content, None)
    }

    /// Create a directory (and any missing parents).
    pub fn mkdir(&self, project_id: &str, rel: &str) -> Result<FileStat, ProjectError> {
        let project = self.projects.require_trusted(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        std::fs::create_dir_all(&path)?;
        self.bus.publish(
            "file.created",
            "ide",
            json!({ "project_id": project.id, "path": normalise(rel), "kind": "dir" }),
        )?;
        self.stat(project_id, rel)
    }

    /// Rename or move a path inside the project.
    pub fn rename(&self, project_id: &str, from: &str, to: &str) -> Result<FileStat, ProjectError> {
        let project = self.projects.require_trusted(project_id)?;
        let source = self.projects.resolve(&project, from)?;
        let target = self.projects.resolve(&project, to)?;
        if !source.exists() {
            return Err(ProjectError::Invalid(format!("{from} does not exist")));
        }
        if target.exists() {
            return Err(ProjectError::Conflict(to.to_string()));
        }
        let context = BackupContext::new();
        self.snapshot_all(&source, &context)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&source, &target)?;
        self.bus.publish(
            "file.renamed",
            "ide",
            json!({
                "project_id": project.id,
                "from": normalise(from),
                "to": normalise(to),
            }),
        )?;
        self.stat(project_id, to)
    }

    /// Delete a file or directory tree.
    pub fn delete(&self, project_id: &str, rel: &str) -> Result<bool, ProjectError> {
        let project = self.projects.require_trusted(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        if path == Path::new(&project.path) {
            return Err(ProjectError::Invalid(
                "refusing to delete the project root".to_string(),
            ));
        }
        if !path.exists() {
            return Ok(false);
        }
        let context = BackupContext::new();
        self.snapshot_all(&path, &context)?;
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
        self.bus.publish(
            "file.deleted",
            "ide",
            json!({ "project_id": project.id, "path": normalise(rel) }),
        )?;
        Ok(true)
    }

    /// List one directory. Reads are allowed in restricted projects.
    pub fn list(&self, project_id: &str, rel: &str) -> Result<Vec<FileEntry>, ProjectError> {
        let project = self.projects.require(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".fergents" {
                continue;
            }
            let metadata = entry.metadata()?;
            let is_dir = metadata.is_dir();
            entries.push(FileEntry {
                path: join_rel(rel, &name),
                name,
                kind: if is_dir { "dir" } else { "file" }.to_string(),
                size: if is_dir { 0 } else { metadata.len() as i64 },
            });
        }
        entries.sort_by(|a, b| {
            (a.kind != "dir")
                .cmp(&(b.kind != "dir"))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// Stat one path. Reads are allowed in restricted projects.
    pub fn stat(&self, project_id: &str, rel: &str) -> Result<FileStat, ProjectError> {
        let project = self.projects.require(project_id)?;
        let path = self.projects.resolve(&project, rel)?;
        let metadata = std::fs::metadata(&path)?;
        let is_dir = metadata.is_dir();
        let etag = if is_dir {
            String::new()
        } else {
            etag_of(&path)?.unwrap_or_default()
        };
        Ok(FileStat {
            path: normalise(rel),
            kind: if is_dir { "dir" } else { "file" }.to_string(),
            size: if is_dir { 0 } else { metadata.len() as i64 },
            modified_at: modified_ms(&metadata),
            etag,
        })
    }

    /// Start watching a project for external changes. Idempotent.
    pub fn watch(&self, project_id: &str) -> Result<bool, ProjectError> {
        let project = self.projects.require(project_id)?;
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| ProjectError::Watch("watcher registry poisoned".to_string()))?;
        if watchers.contains_key(project_id) {
            return Ok(false);
        }

        let root = PathBuf::from(&project.path);
        let (tx, rx) = mpsc::channel::<notify::Event>();
        let bus = self.bus.clone();
        let watched_root = root.clone();
        let watched_id = project.id.clone();
        std::thread::Builder::new()
            .name(format!("fergents-watch-{}", project.id))
            .spawn(move || debounce_loop(rx, bus, watched_id, watched_root))?;

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    let _ = tx.send(event);
                }
            })
            .map_err(|err| ProjectError::Watch(err.to_string()))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|err| ProjectError::Watch(err.to_string()))?;
        watchers.insert(project_id.to_string(), watcher);
        Ok(true)
    }

    /// Stop watching a project.
    pub fn unwatch(&self, project_id: &str) -> bool {
        self.watchers
            .lock()
            .map(|mut watchers| watchers.remove(project_id).is_some())
            .unwrap_or(false)
    }

    /// Start watching every known project (called at kernel start).
    pub fn watch_all(&self) -> Result<usize, ProjectError> {
        let mut started = 0;
        for project in self.projects.list()? {
            if self.watch(&project.id)? {
                started += 1;
            }
        }
        Ok(started)
    }

    /// Whether a project is currently watched.
    pub fn is_watching(&self, project_id: &str) -> bool {
        self.watchers
            .lock()
            .map(|watchers| watchers.contains_key(project_id))
            .unwrap_or(false)
    }

    /// Snapshot every file under `path` (recursively for directories).
    fn snapshot_all(&self, path: &Path, context: &BackupContext) -> Result<(), ProjectError> {
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                self.snapshot_all(&entry?.path(), context)?;
            }
        } else if path.exists() {
            self.reversibility.snapshot(path, context)?;
        }
        Ok(())
    }
}

/// The outcome of decoding one file for reading.
struct TextFile {
    content: String,
    etag: String,
    truncated: bool,
    binary: bool,
}

/// Read a file, hashing the whole content but decoding only the first
/// [`MAX_READ_BYTES`].
fn read_text(path: &Path) -> Result<TextFile, ProjectError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_HASH_BYTES {
        return Err(ProjectError::Invalid(
            "file is too large to open".to_string(),
        ));
    }
    let bytes = std::fs::read(path)?;
    let probe = &bytes[..bytes.len().min(BINARY_PROBE_BYTES)];
    if probe.contains(&0) {
        return Ok(TextFile {
            content: String::new(),
            etag: String::new(),
            truncated: false,
            binary: true,
        });
    }

    let limit = bytes.len().min(MAX_READ_BYTES as usize);
    let slice = &bytes[..limit];
    let content = match std::str::from_utf8(slice) {
        Ok(text) => text.to_string(),
        Err(err) => String::from_utf8_lossy(&slice[..err.valid_up_to()]).into_owned(),
    };
    Ok(TextFile {
        content,
        etag: sha256_hex(&bytes),
        truncated: limit < bytes.len(),
        binary: false,
    })
}

/// `sha256(content)` as lowercase hex, or `None` when the file is absent.
fn etag_of(path: &Path) -> Result<Option<String>, ProjectError> {
    if !path.is_file() {
        return Ok(None);
    }
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_HASH_BYTES {
        return Err(ProjectError::Invalid(
            "file is too large to hash".to_string(),
        ));
    }
    Ok(Some(sha256_hex(&std::fs::read(path)?)))
}

/// Write through a sibling temporary file and rename it into place.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let parent = path
        .parent()
        .ok_or_else(|| ProjectError::Invalid("cannot write outside the project".to_string()))?;
    let temp = parent.join(format!(".fergents-{}.tmp", ulid::Ulid::new()));
    std::fs::write(&temp, bytes)?;
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |delta| delta.as_millis() as i64)
}

/// Normalise a project-relative path for the wire (forward slashes, no `./`).
fn normalise(rel: &str) -> String {
    let rel = rel.trim().replace('\\', "/");
    rel.trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

fn join_rel(dir: &str, name: &str) -> String {
    let dir = normalise(dir);
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Publish `file.changed` for each distinct path, coalescing bursts.
fn debounce_loop(
    rx: mpsc::Receiver<notify::Event>,
    bus: Arc<EventBus>,
    project_id: String,
    root: PathBuf,
) {
    loop {
        let mut pending: Vec<String> = Vec::new();
        match rx.recv() {
            Ok(event) => collect_paths(&mut pending, &root, &event),
            Err(_) => return,
        }
        loop {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(event) => collect_paths(&mut pending, &root, &event),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    publish_paths(&bus, &project_id, &pending);
                    return;
                }
            }
        }
        publish_paths(&bus, &project_id, &pending);
    }
}

fn collect_paths(pending: &mut Vec<String>, root: &Path, event: &notify::Event) {
    for path in &event.paths {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if rel.is_empty()
            || rel.starts_with(".git/")
            || rel == ".git"
            || rel.starts_with(".fergents/")
        {
            continue;
        }
        if !pending.iter().any(|seen| seen == &rel) {
            pending.push(rel);
        }
    }
}

fn publish_paths(bus: &EventBus, project_id: &str, paths: &[String]) {
    for path in paths {
        let _ = bus.publish(
            "file.changed",
            "ide",
            json!({ "project_id": project_id, "path": path, "origin": "external" }),
        );
    }
}

/// The Monaco/VS Code language id for a path, by extension.
pub fn language_id(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescript",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascript",
        "json" => "json",
        "jsonc" => "json",
        "md" | "markdown" => "markdown",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "html" | "htm" => "html",
        "py" => "python",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "c" => "c",
        "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "xml" => "xml",
        "svg" => "xml",
        "dockerfile" => "dockerfile",
        "lock" => "plaintext",
        _ => {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Dockerfile"))
            {
                "dockerfile"
            } else {
                "plaintext"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_ids_cover_common_extensions() {
        assert_eq!(language_id(Path::new("src/main.rs")), "rust");
        assert_eq!(language_id(Path::new("app/App.tsx")), "typescript");
        assert_eq!(language_id(Path::new("Dockerfile")), "dockerfile");
        assert_eq!(language_id(Path::new("LICENSE")), "plaintext");
    }

    #[test]
    fn paths_are_normalised() {
        assert_eq!(normalise("./src\\main.rs/"), "src/main.rs");
        assert_eq!(join_rel("", "README.md"), "README.md");
        assert_eq!(join_rel("src", "main.rs"), "src/main.rs");
    }
}
