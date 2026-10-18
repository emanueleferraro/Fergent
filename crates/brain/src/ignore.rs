//! `.brainignore` support (`BRAIN.md` §3.4, `SEARCH.md` §13).
//!
//! The indexer skips `.fergents/`, `index.sqlite*`, `graph.db*`, `.DS_Store`,
//! `Thumbs.db`, and any path matched by the brain's `.brainignore` file
//! (gitignore syntax).
//!
//! Paths are matched relative to the brain folder (where `.brainignore`
//! lives): e.g. `vault/notes/a.md`.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::error::BrainError;

/// The ignore file name inside a brain folder.
pub const BRAINIGNORE: &str = ".brainignore";

/// Paths the indexer always ignores, independently of `.brainignore`.
pub const BUILTIN_IGNORES: [&str; 5] = [
    ".fergents/",
    "index.sqlite*",
    "graph.db*",
    ".DS_Store",
    "Thumbs.db",
];

/// A compiled ignore matcher: the built-in patterns plus the brain's
/// `.brainignore` (if present).
#[derive(Debug, Clone)]
pub struct BrainIgnore {
    matcher: Gitignore,
}

impl BrainIgnore {
    /// Compile a matcher from the built-in patterns plus the given
    /// gitignore-syntax patterns.
    pub fn from_patterns<I, S>(patterns: I) -> Result<Self, BrainError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = GitignoreBuilder::new("");
        for pattern in BUILTIN_IGNORES {
            builder
                .add_line(None, pattern)
                .map_err(|e| BrainError::invalid(format!("built-in ignore {pattern:?}: {e}")))?;
        }
        for pattern in patterns {
            let pattern = pattern.as_ref();
            builder.add_line(None, pattern).map_err(|e| {
                BrainError::invalid(format!(".brainignore pattern {pattern:?}: {e}"))
            })?;
        }
        let matcher = builder.build().map_err(|e| {
            BrainError::invalid(format!("could not build .brainignore matcher: {e}"))
        })?;
        Ok(Self { matcher })
    }

    /// Load the brain's `.brainignore` from `brain_dir`, if it exists.
    ///
    /// A missing file is not an error; the built-in patterns still apply.
    pub fn load(brain_dir: &Path) -> Result<Self, BrainError> {
        let path = brain_dir.join(BRAINIGNORE);
        let patterns = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(BrainError::io(&path, e)),
        };
        Self::from_patterns(patterns.lines())
    }

    /// Whether `path` (relative to the brain folder) is ignored.
    ///
    /// Parent directories are taken into account, so a path inside an ignored
    /// directory is itself ignored.
    pub fn is_ignored(&self, path: impl AsRef<Path>, is_dir: bool) -> bool {
        self.matcher
            .matched_path_or_any_parents(path.as_ref(), is_dir)
            .is_ignore()
    }

    /// The default matcher (built-in patterns only), for brains with no
    /// `.brainignore`.
    pub fn empty() -> Self {
        Self::from_patterns(std::iter::empty::<&str>()).expect("built-in ignore patterns are valid")
    }

    /// Quick helper: check a path against the built-in patterns only.
    pub fn is_builtin_ignored(path: impl AsRef<Path>, is_dir: bool) -> bool {
        Self::empty().is_ignored(path, is_dir)
    }
}

impl Default for BrainIgnore {
    fn default() -> Self {
        Self::empty()
    }
}

/// The resolved `.brainignore` path inside a brain folder.
pub fn brainignore_path(brain_dir: &Path) -> PathBuf {
    brain_dir.join(BRAINIGNORE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_ignored() {
        let ignore = BrainIgnore::empty();
        assert!(ignore.is_ignored(".fergents/journal.md", false));
        assert!(ignore.is_ignored(".fergents", true));
        assert!(ignore.is_ignored("index.sqlite", false));
        assert!(ignore.is_ignored("index.sqlite-wal", false));
        assert!(ignore.is_ignored("graph.db", false));
        assert!(ignore.is_ignored("graph.db-shm", false));
        assert!(ignore.is_ignored(".DS_Store", false));
        assert!(ignore.is_ignored("vault/notes/.DS_Store", false));
        assert!(ignore.is_ignored("Thumbs.db", false));
    }

    #[test]
    fn normal_vault_paths_are_not_ignored() {
        let ignore = BrainIgnore::empty();
        assert!(!ignore.is_ignored("vault/notes/a.md", false));
        assert!(!ignore.is_ignored("vault", true));
        assert!(!ignore.is_ignored("brain.toml", false));
    }

    #[test]
    fn custom_patterns_apply() {
        let ignore = BrainIgnore::from_patterns(["*.tmp", "build/", "!keep.tmp"]).unwrap();
        assert!(ignore.is_ignored("vault/notes/draft.tmp", false));
        assert!(ignore.is_ignored("vault/build/x.md", false));
        assert!(ignore.is_ignored("build", true));
        assert!(!ignore.is_ignored("vault/notes/keep.tmp", false));
        assert!(!ignore.is_ignored("vault/notes/a.md", false));
    }

    #[test]
    fn load_reads_the_brainignore_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(BRAINIGNORE), "*.log\nscratch/\n").unwrap();
        let ignore = BrainIgnore::load(dir.path()).unwrap();
        assert!(ignore.is_ignored("vault/a.log", false));
        assert!(ignore.is_ignored("scratch/todo.md", false));
        // Built-ins still apply alongside the file.
        assert!(ignore.is_ignored("index.sqlite", false));
        assert!(!ignore.is_ignored("vault/notes/a.md", false));
    }

    #[test]
    fn load_without_file_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let ignore = BrainIgnore::load(dir.path()).unwrap();
        assert!(ignore.is_ignored("index.sqlite", false));
        assert!(!ignore.is_ignored("vault/a.md", false));
    }
}
