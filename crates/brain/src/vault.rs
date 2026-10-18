//! Vault layout (`BRAIN.md` §4.1).
//!
//! A brain folder contains `brain.toml`, a `vault/` of markdown files, a
//! `.fergents/` metadata folder, and (once indexed) `index.sqlite` and an
//! optional `graph.db`.
//!
//! The vault has no required structure, but the default template lays out
//! `daily/`, `notes/`, `attachments/`, and `templates/` under the vault root.

use std::path::{Path, PathBuf};

use crate::config::BrainConfig;
use crate::error::BrainError;

/// The brain's metadata folder, inside the brain folder.
pub const BRAIN_META_DIR: &str = ".fergents";

/// Resolved directories for a brain's vault, derived from [`BrainConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultLayout {
    /// The brain folder itself.
    pub brain_dir: PathBuf,
    /// `<brain>/.fergents` — internal metadata.
    pub meta_dir: PathBuf,
    /// `<brain>/<vault.path>` — the vault root.
    pub vault_root: PathBuf,
    /// `<vault>/<default_note_folder>`.
    pub notes_dir: PathBuf,
    /// `<vault>/<daily_folder>`.
    pub daily_dir: PathBuf,
    /// `<vault>/<attachments_folder>`.
    pub attachments_dir: PathBuf,
    /// `<vault>/<templates_folder>`.
    pub templates_dir: PathBuf,
}

impl VaultLayout {
    /// Resolve the layout for `brain_dir` using the given configuration.
    pub fn from_config(brain_dir: impl Into<PathBuf>, config: &BrainConfig) -> Self {
        let brain_dir = brain_dir.into();
        let vault_root = brain_dir.join(&config.vault.path);
        Self {
            meta_dir: brain_dir.join(BRAIN_META_DIR),
            notes_dir: vault_root.join(&config.vault.default_note_folder),
            daily_dir: vault_root.join(&config.vault.daily_folder),
            attachments_dir: vault_root.join(&config.vault.attachments_folder),
            templates_dir: vault_root.join(&config.vault.templates_folder),
            vault_root,
            brain_dir,
        }
    }

    /// Every directory in the layout that should exist on disk.
    pub fn dirs(&self) -> Vec<&Path> {
        vec![
            &self.brain_dir,
            &self.meta_dir,
            &self.vault_root,
            &self.notes_dir,
            &self.daily_dir,
            &self.attachments_dir,
            &self.templates_dir,
        ]
    }

    /// Create the brain folder, the metadata folder, the vault root, and the
    /// default subfolders if they do not exist. Idempotent.
    pub fn ensure(&self) -> Result<(), BrainError> {
        for dir in self.dirs() {
            std::fs::create_dir_all(dir).map_err(|e| BrainError::io(dir, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_directories() {
        let config = BrainConfig::default();
        let layout = VaultLayout::from_config("/brains/personal", &config);
        assert_eq!(layout.brain_dir, PathBuf::from("/brains/personal"));
        assert_eq!(layout.meta_dir, PathBuf::from("/brains/personal/.fergents"));
        assert_eq!(layout.vault_root, PathBuf::from("/brains/personal/vault"));
        assert_eq!(
            layout.notes_dir,
            PathBuf::from("/brains/personal/vault/notes")
        );
        assert_eq!(
            layout.daily_dir,
            PathBuf::from("/brains/personal/vault/daily")
        );
        assert_eq!(
            layout.attachments_dir,
            PathBuf::from("/brains/personal/vault/attachments")
        );
        assert_eq!(
            layout.templates_dir,
            PathBuf::from("/brains/personal/vault/templates")
        );
    }

    #[test]
    fn honours_custom_folder_names() {
        let mut config = BrainConfig::default();
        config.vault.path = "notes-vault".to_string();
        config.vault.daily_folder = "journal".to_string();
        let layout = VaultLayout::from_config("/brains/x", &config);
        assert_eq!(layout.vault_root, PathBuf::from("/brains/x/notes-vault"));
        assert_eq!(
            layout.daily_dir,
            PathBuf::from("/brains/x/notes-vault/journal")
        );
    }

    #[test]
    fn ensure_creates_every_directory_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let config = BrainConfig::default();
        let layout = VaultLayout::from_config(dir.path(), &config);

        layout.ensure().expect("create");
        for path in [
            &layout.brain_dir,
            &layout.meta_dir,
            &layout.vault_root,
            &layout.notes_dir,
            &layout.daily_dir,
            &layout.attachments_dir,
            &layout.templates_dir,
        ] {
            assert!(path.is_dir(), "{} should be a directory", path.display());
        }

        // Idempotent: calling again does not error.
        layout.ensure().expect("create again");
    }
}
