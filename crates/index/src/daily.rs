//! Daily notes (`BRAIN.md` §4.4; spec Task 13.1).
//!
//! A daily note is created once per day, on first open, at
//! `<daily_folder>/<strftime>.md` (default `daily/YYYY-MM-DD.md`, decision D10).
//! It is seeded from the configured daily template (if any) and immediately
//! indexed through the existing pipeline via the [`NoteIndexer`] hook.
//!
//! Only the file creation and rendering live here; the caller supplies the
//! resolved [`VaultLayout`](fergents_brain::VaultLayout) directories and the
//! wall-clock instant, so the logic is pure and testable without a brain.

use std::path::PathBuf;

use chrono::NaiveDateTime;

use crate::embed::Embedder;
use crate::error::IndexError;
use crate::pipeline::{IndexOutcome, Indexer};
use crate::templates::{self, TemplateVariables};
use crate::vector::VectorStore;

/// The default daily-note file name format (decision D10).
pub const DEFAULT_DAILY_FORMAT: &str = "%Y-%m-%d";
/// The default daily template file name inside the templates folder.
pub const DEFAULT_DAILY_TEMPLATE: &str = "daily.md";

/// A hook that indexes a vault-relative file.
///
/// Implemented by [`Indexer`], so `daily.open` reuses the existing pipeline
/// (Task 8) instead of reimplementing detection, chunking, or embedding.
pub trait NoteIndexer {
    /// Index (create or update) the vault-relative file `rel_path`.
    fn index_note(&self, rel_path: &str) -> Result<IndexOutcome, IndexError>;
}

impl<E: Embedder, V: VectorStore> NoteIndexer for Indexer<E, V> {
    fn index_note(&self, rel_path: &str) -> Result<IndexOutcome, IndexError> {
        self.index_file(rel_path)
    }
}

/// The outcome of opening a daily note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyNote {
    /// The note's absolute path on disk.
    pub path: PathBuf,
    /// The note's vault-relative path (used for indexing).
    pub rel_path: String,
    /// Whether the note was created by this call (`false` if it already existed).
    pub created: bool,
    /// The note's content (the freshly rendered template when created).
    pub content: String,
    /// The byte offset of the daily template's `{{cursor}}`, if present.
    pub cursor: Option<usize>,
}

/// Creates and opens daily notes for one vault.
#[derive(Debug, Clone)]
pub struct DailyNotes {
    vault_root: PathBuf,
    daily_dir: PathBuf,
    templates_dir: PathBuf,
    daily_format: String,
    daily_template: String,
}

impl DailyNotes {
    /// Build from the resolved vault directories.
    pub fn new(
        vault_root: impl Into<PathBuf>,
        daily_dir: impl Into<PathBuf>,
        templates_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            vault_root: vault_root.into(),
            daily_dir: daily_dir.into(),
            templates_dir: templates_dir.into(),
            daily_format: DEFAULT_DAILY_FORMAT.to_string(),
            daily_template: DEFAULT_DAILY_TEMPLATE.to_string(),
        }
    }

    /// Override the `strftime` filename format (`brain.toml` `vault.daily_format`).
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.daily_format = format.into();
        self
    }

    /// Override the daily template file name inside the templates folder.
    pub fn with_template(mut self, name: impl Into<String>) -> Self {
        self.daily_template = name.into();
        self
    }

    /// The configured filename format.
    pub fn format(&self) -> &str {
        &self.daily_format
    }

    /// The absolute path and vault-relative path of the daily note for `now`.
    pub fn path_for(&self, now: NaiveDateTime) -> Result<(PathBuf, String), IndexError> {
        validate_format(&self.daily_format)?;
        let file_name = format!("{}.md", now.format(&self.daily_format));
        let path = self.daily_dir.join(&file_name);
        let rel_dir = self
            .daily_dir
            .strip_prefix(&self.vault_root)
            .map_err(|_| IndexError::Invalid("the daily folder must be inside the vault".into()))?;
        // Normalize to `/` so the indexed path is stable across platforms.
        let rel_path = rel_dir
            .join(&file_name)
            .to_string_lossy()
            .replace('\\', "/");
        Ok((path, rel_path))
    }

    /// Create the daily note for `now` if absent, then index it, and return it.
    ///
    /// Idempotent within a day: a second call finds the existing file, does not
    /// rewrite it, and does not re-index it.
    pub fn open(
        &self,
        indexer: &dyn NoteIndexer,
        now: NaiveDateTime,
    ) -> Result<DailyNote, IndexError> {
        let (path, rel_path) = self.path_for(now)?;

        if path.is_file() {
            let content =
                std::fs::read_to_string(&path).map_err(|e| IndexError::Io(e.to_string()))?;
            return Ok(DailyNote {
                path,
                rel_path,
                created: false,
                content,
                cursor: None,
            });
        }

        let mut vars = TemplateVariables::from_now(now);
        vars.title = now.format(&self.daily_format).to_string();
        let template = self.load_template()?;
        let rendered = templates::render(&template, &vars);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| IndexError::Io(e.to_string()))?;
        }
        std::fs::write(&path, &rendered.content).map_err(|e| IndexError::Io(e.to_string()))?;

        indexer.index_note(&rel_path)?;

        Ok(DailyNote {
            path,
            rel_path,
            created: true,
            content: rendered.content,
            cursor: rendered.cursor,
        })
    }

    /// Read the daily template, or an empty string when it does not exist.
    fn load_template(&self) -> Result<String, IndexError> {
        let path = self.templates_dir.join(&self.daily_template);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(IndexError::Io(e.to_string())),
        }
    }
}

/// The current local wall-clock instant, for convenience.
pub fn now_local() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// Validate an `strftime` format string, rejecting empty or malformed formats.
pub fn validate_format(format: &str) -> Result<(), IndexError> {
    use chrono::format::{Item, StrftimeItems};
    if format.trim().is_empty() {
        return Err(IndexError::Invalid("daily_format must not be empty".into()));
    }
    if StrftimeItems::new(format).any(|item| matches!(item, Item::Error)) {
        return Err(IndexError::Invalid(format!(
            "invalid daily_format: {format:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 10, 18)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
    }

    #[test]
    fn default_path_is_daily_dated() {
        let notes = DailyNotes::new("b/v", "b/v/daily", "b/v/templates");
        let (path, rel) = notes.path_for(at()).unwrap();
        assert_eq!(path, PathBuf::from("b/v/daily").join("2025-10-18.md"));
        assert_eq!(rel, "daily/2025-10-18.md");
    }

    #[test]
    fn custom_format_and_folder_are_honoured() {
        let notes = DailyNotes::new("b/v", "b/v/journal", "b/v/templates").with_format("%Y/%m/%d");
        let (path, rel) = notes.path_for(at()).unwrap();
        assert_eq!(path, PathBuf::from("b/v/journal").join("2025/10/18.md"));
        assert_eq!(rel, "journal/2025/10/18.md");
    }

    #[test]
    fn invalid_format_is_rejected() {
        let notes = DailyNotes::new("b/v", "b/v/daily", "b/v/templates").with_format("%Q");
        assert!(matches!(notes.path_for(at()), Err(IndexError::Invalid(_))));
    }

    #[test]
    fn daily_folder_outside_vault_is_rejected() {
        let notes = DailyNotes::new("b/vault", "elsewhere", "b/vault/templates");
        assert!(matches!(notes.path_for(at()), Err(IndexError::Invalid(_))));
    }
}
