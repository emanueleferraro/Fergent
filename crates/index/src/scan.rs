//! Vault traversal and file classification (`SEARCH.md` §13.1).
//!
//! The scanner walks the vault, skipping anything matched by `.brainignore`
//! (gitignore syntax, via the `ignore` crate) or by the built-in ignores, and
//! classifies each file into a [`DocumentKind`].

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use walkdir::WalkDir;

use crate::error::IndexError;

/// Files always ignored, mirroring the brain's built-ins: the metadata dir and
/// the generated indexes are never vault content.
pub const BUILTIN_IGNORES: &[&str] = &[
    ".fergents/",
    "index.sqlite*",
    "graph.db*",
    ".DS_Store",
    "Thumbs.db",
];

/// The kind of a vault file (`documents.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// Markdown note.
    Note,
    /// Attachment (image, PDF, media, archive).
    Attachment,
    /// Source code or structured config.
    Code,
    /// Anything else.
    Other,
}

impl DocumentKind {
    /// The `documents.kind` string.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Note => "note",
            DocumentKind::Attachment => "attachment",
            DocumentKind::Code => "code",
            DocumentKind::Other => "other",
        }
    }
}

/// A file discovered by the vault scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Vault-relative path, using `/` separators.
    pub rel_path: String,
    /// Absolute path on disk.
    pub abs_path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Classification.
    pub kind: DocumentKind,
    /// Best-effort MIME type.
    pub mime: String,
    /// Whether the file can be read as UTF-8 text and chunked.
    pub is_text: bool,
}

/// Walks a vault and classifies its files.
pub struct VaultScanner {
    root: PathBuf,
    matcher: Gitignore,
}

impl VaultScanner {
    /// Create a scanner for `root`, loading `<root>/.brainignore` (if present)
    /// plus the built-in ignores.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, IndexError> {
        let root = root.into();
        let mut builder = GitignoreBuilder::new(&root);
        let _ = builder.add(root.join(".brainignore"));
        for pattern in BUILTIN_IGNORES {
            let _ = builder.add_line(None, pattern);
        }
        let matcher = builder
            .build()
            .map_err(|err| IndexError::Io(err.to_string()))?;
        Ok(Self { root, matcher })
    }

    /// The vault root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether `rel_path` (vault-relative, `/` separators) is ignored.
    pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool {
        self.matcher
            .matched_path_or_any_parents(Path::new(rel_path), is_dir)
            .is_ignore()
    }

    /// Scan the vault, returning the non-ignored files in a stable order.
    pub fn scan(&self) -> Vec<ScannedFile> {
        let mut files = Vec::new();
        for entry in WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !self.is_ignored(&self.relative(entry.path()), entry.file_type().is_dir())
            })
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let abs_path = entry.path().to_path_buf();
            let rel_path = self.relative(&abs_path);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let (kind, mime, is_text) = classify(&rel_path);
            files.push(ScannedFile {
                rel_path,
                abs_path,
                size,
                kind,
                mime,
                is_text,
            });
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        files
    }

    /// A vault-relative path with `/` separators.
    pub fn relative(&self, abs: &Path) -> String {
        abs.strip_prefix(&self.root)
            .unwrap_or(abs)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Classify a path into `(kind, mime, is_text)`.
pub fn classify(rel_path: &str) -> (DocumentKind, String, bool) {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (kind, mime, is_text) = match ext.as_str() {
        "md" | "markdown" | "mdx" => (DocumentKind::Note, "text/markdown", true),
        "txt" | "log" | "csv" | "tsv" | "ini" | "cfg" | "env" => {
            (DocumentKind::Other, "text/plain", true)
        }
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "kt" | "swift" | "cs" | "c"
        | "h" | "cc" | "cpp" | "hpp" | "rb" | "php" | "lua" | "sh" | "bash" | "sql" | "html"
        | "htm" | "css" | "json" | "yaml" | "yml" | "toml" | "xml" | "vue" | "svelte" => {
            (DocumentKind::Code, mime_for_code(&ext), true)
        }
        "png" => (DocumentKind::Attachment, "image/png", false),
        "jpg" | "jpeg" => (DocumentKind::Attachment, "image/jpeg", false),
        "gif" => (DocumentKind::Attachment, "image/gif", false),
        "webp" => (DocumentKind::Attachment, "image/webp", false),
        "svg" => (DocumentKind::Attachment, "image/svg+xml", true),
        "pdf" => (DocumentKind::Attachment, "application/pdf", false),
        "mp3" => (DocumentKind::Attachment, "audio/mpeg", false),
        "wav" => (DocumentKind::Attachment, "audio/wav", false),
        "mp4" => (DocumentKind::Attachment, "video/mp4", false),
        "mov" => (DocumentKind::Attachment, "video/quicktime", false),
        "zip" => (DocumentKind::Attachment, "application/zip", false),
        _ => (DocumentKind::Other, "application/octet-stream", false),
    };
    (kind, mime.to_string(), is_text)
}

fn mime_for_code(ext: &str) -> &'static str {
    match ext {
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        _ => "text/plain",
    }
}
