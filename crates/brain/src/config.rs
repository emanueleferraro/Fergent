//! The `brain.toml` model (`BRAIN.md` §6.1).
//!
//! A brain's `brain.toml` holds its identity (`[brain]`) and its
//! configuration (`[vault]`, `[markdown]`, `[index]`, `[graph]`, `[search]`,
//! `[sync]`, `[git]`). In Phase 1 the `[sync]` and `[git]` sections are
//! parsed and preserved but have no runtime effect.
//!
//! Loading is permissive: unknown sections/keys are ignored, and any missing
//! section or key falls back to the documented default, so a partial file
//! (e.g. written by a newer client) still loads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::BrainError;

/// The `brain.toml` file name inside a brain folder.
pub const BRAIN_TOML: &str = "brain.toml";

/// Accepted values for `search.default_mode` (`SEARCH.md` §2).
pub const SEARCH_MODES: [&str; 4] = ["hybrid", "bm25", "vector", "sparse"];

/// Accepted values for `sync.scope` (`BRAIN.md` §6.1).
pub const SYNC_SCOPES: [&str; 3] = ["full", "notes-only", "custom"];

/// The full `brain.toml` configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrainConfig {
    /// `[brain]` — identity.
    pub brain: BrainIdentity,
    /// `[vault]` — vault structure.
    pub vault: VaultConfig,
    /// `[markdown]` — markdown flavor toggles.
    pub markdown: MarkdownConfig,
    /// `[index]` — indexing and embeddings.
    pub index: IndexConfig,
    /// `[graph]` — knowledge-graph extraction.
    pub graph: GraphConfig,
    /// `[search]` — search defaults.
    pub search: SearchConfig,
    /// `[sync]` — parsed only in Phase 1.
    pub sync: SyncConfig,
    /// `[git]` — parsed only in Phase 1.
    pub git: GitConfig,
}

/// `[brain]` — the brain's identity (`BRAIN.md` §6.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrainIdentity {
    /// Stable brain id (ULID/UUID). Not the path (`BRAIN.md` §10.4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// URL/filesystem-friendly slug.
    pub slug: String,
    /// Template the brain was created from.
    pub template: Option<String>,
    /// Accent color.
    pub color: Option<String>,
    /// Icon (emoji or path).
    pub icon: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

/// `[vault]` — vault structure (`BRAIN.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    /// Vault path, relative to the brain folder.
    pub path: String,
    /// Default folder for new notes.
    pub default_note_folder: String,
    /// Folder for daily notes.
    pub daily_folder: String,
    /// `strftime`-style format for daily-note filenames.
    pub daily_format: String,
    /// Folder for attachments.
    pub attachments_folder: String,
    /// Folder for note templates.
    pub templates_folder: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            path: "vault".to_string(),
            default_note_folder: "notes".to_string(),
            daily_folder: "daily".to_string(),
            daily_format: "%Y-%m-%d".to_string(),
            attachments_folder: "attachments".to_string(),
            templates_folder: "templates".to_string(),
        }
    }
}

/// `[markdown]` — markdown flavor toggles (`BRAIN.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    /// Support `[[wiki links]]`.
    pub wiki_links: bool,
    /// Support `![[embeds]]`.
    pub embeds: bool,
    /// Support `^block-id` references.
    pub block_refs: bool,
    /// Support Obsidian callouts.
    pub callouts: bool,
    /// Support `$inline$` and `$$block$$` math.
    pub math: bool,
    /// Support Mermaid fenced blocks.
    pub mermaid: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            wiki_links: true,
            embeds: true,
            block_refs: true,
            callouts: true,
            math: true,
            mermaid: true,
        }
    }
}

/// `[index]` — indexing and embeddings (`BRAIN.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexConfig {
    /// Embedding model id.
    pub embedding_model: String,
    /// Target chunk size, in tokens.
    pub chunk_size: u32,
    /// Chunk overlap, in tokens.
    pub chunk_overlap: u32,
    /// Reindex automatically on change.
    pub auto_index: bool,
    /// Watch the vault for changes.
    pub watch: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            embedding_model: "local:bge-m3".to_string(),
            chunk_size: 512,
            chunk_overlap: 64,
            auto_index: true,
            watch: true,
        }
    }
}

/// `[graph]` — knowledge-graph extraction (`BRAIN.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphConfig {
    /// Enable the entity graph.
    pub enabled: bool,
    /// Extract entities.
    pub extract_entities: bool,
    /// Extract relations.
    pub extract_relations: bool,
    /// Use the LLM for extraction (off by default; `GRAPH.md` §4).
    pub llm_extraction: bool,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extract_entities: true,
            extract_relations: true,
            llm_extraction: false,
        }
    }
}

/// `[search]` — search defaults (`BRAIN.md` §6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Default search mode (`hybrid`, `bm25`, `vector`, `sparse`).
    pub default_mode: String,
    /// Default number of results.
    pub default_top_k: u32,
    /// Rerank results by default.
    pub rerank: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_mode: "hybrid".to_string(),
            default_top_k: 8,
            rerank: true,
        }
    }
}

/// `[sync]` — parsed and preserved in Phase 1, no runtime effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Syncing enabled.
    pub enabled: bool,
    /// Sync scope (`full`, `notes-only`, `custom`).
    pub scope: String,
    /// Peer ids.
    pub peers: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: "full".to_string(),
            peers: Vec::new(),
        }
    }
}

/// `[git]` — parsed and preserved in Phase 1, no runtime effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GitConfig {
    /// Shadow-git versioning enabled.
    pub enabled: bool,
    /// Commit automatically on change.
    pub auto_commit: bool,
    /// Template for commit messages.
    pub commit_message_template: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_commit: false,
            commit_message_template: "fergents: {{action}} {{path}}".to_string(),
        }
    }
}

impl BrainConfig {
    /// The `brain.toml` path inside `brain_dir`.
    pub fn path(brain_dir: &Path) -> PathBuf {
        brain_dir.join(BRAIN_TOML)
    }

    /// Parse a `brain.toml` from a string.
    pub fn from_toml_str(text: &str) -> Result<Self, BrainError> {
        toml::from_str(text).map_err(BrainError::parse)
    }

    /// Serialize this configuration as TOML.
    pub fn to_toml_string(&self) -> Result<String, BrainError> {
        toml::to_string_pretty(self).map_err(BrainError::serialize)
    }

    /// Load `brain.toml` from `brain_dir`.
    pub fn load(brain_dir: &Path) -> Result<Self, BrainError> {
        let path = Self::path(brain_dir);
        let text = std::fs::read_to_string(&path).map_err(|e| BrainError::io(&path, e))?;
        Self::from_toml_str(&text)
    }

    /// Validate the configuration and write it to `brain_dir/brain.toml`.
    ///
    /// The file is written to a temporary path and renamed, so readers never
    /// observe a partially written file.
    pub fn save(&self, brain_dir: &Path) -> Result<(), BrainError> {
        self.validate()?;
        let path = Self::path(brain_dir);
        let text = self.to_toml_string()?;
        std::fs::create_dir_all(brain_dir).map_err(|e| BrainError::io(brain_dir, e))?;
        let tmp = path.with_file_name(format!("{BRAIN_TOML}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| BrainError::io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| BrainError::io(&path, e))?;
        Ok(())
    }

    /// Validate the configuration, returning a clear error on the first problem.
    pub fn validate(&self) -> Result<(), BrainError> {
        if self.brain.id.trim().is_empty() {
            return Err(BrainError::invalid("brain.id must not be empty"));
        }
        if self.brain.name.trim().is_empty() {
            return Err(BrainError::invalid("brain.name must not be empty"));
        }
        if self.brain.created_at.trim().is_empty() {
            return Err(BrainError::invalid("brain.created_at must not be empty"));
        }
        if self.vault.path.trim().is_empty() {
            return Err(BrainError::invalid("vault.path must not be empty"));
        }
        if self.index.chunk_size == 0 {
            return Err(BrainError::invalid(
                "index.chunk_size must be greater than 0",
            ));
        }
        if self.index.chunk_overlap >= self.index.chunk_size {
            return Err(BrainError::invalid(
                "index.chunk_overlap must be smaller than index.chunk_size",
            ));
        }
        if !SEARCH_MODES.contains(&self.search.default_mode.as_str()) {
            return Err(BrainError::invalid(format!(
                "search.default_mode must be one of {SEARCH_MODES:?}, got {:?}",
                self.search.default_mode
            )));
        }
        if !SYNC_SCOPES.contains(&self.sync.scope.as_str()) {
            return Err(BrainError::invalid(format!(
                "sync.scope must be one of {SYNC_SCOPES:?}, got {:?}",
                self.sync.scope
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BrainConfig {
        BrainConfig {
            brain: BrainIdentity {
                id: "01J8K2X9".to_string(),
                name: "Personal".to_string(),
                slug: "personal".to_string(),
                template: Some("personal".to_string()),
                color: Some("#6B94D9".to_string()),
                icon: Some("brain".to_string()),
                created_at: "2025-10-14T09:00:00Z".to_string(),
            },
            ..BrainConfig::default()
        }
    }

    #[test]
    fn section_defaults_match_brain_md() {
        assert_eq!(VaultConfig::default().path, "vault");
        assert_eq!(VaultConfig::default().default_note_folder, "notes");
        assert_eq!(VaultConfig::default().daily_folder, "daily");
        assert_eq!(VaultConfig::default().daily_format, "%Y-%m-%d");
        assert_eq!(VaultConfig::default().attachments_folder, "attachments");
        assert_eq!(VaultConfig::default().templates_folder, "templates");

        assert!(MarkdownConfig::default().wiki_links);
        assert!(MarkdownConfig::default().mermaid);

        let index = IndexConfig::default();
        assert_eq!(index.embedding_model, "local:bge-m3");
        assert_eq!(index.chunk_size, 512);
        assert_eq!(index.chunk_overlap, 64);
        assert!(index.auto_index);
        assert!(index.watch);

        assert!(GraphConfig::default().enabled);
        assert!(!GraphConfig::default().llm_extraction);

        assert_eq!(SearchConfig::default().default_mode, "hybrid");
        assert_eq!(SearchConfig::default().default_top_k, 8);
        assert!(SearchConfig::default().rerank);

        assert!(!SyncConfig::default().enabled);
        assert_eq!(SyncConfig::default().scope, "full");
        assert!(SyncConfig::default().peers.is_empty());

        assert!(!GitConfig::default().enabled);
        assert!(!GitConfig::default().auto_commit);
        assert_eq!(
            GitConfig::default().commit_message_template,
            "fergents: {{action}} {{path}}"
        );
    }

    #[test]
    fn round_trip_through_toml() {
        let config = sample();
        let toml = config.to_toml_string().expect("serialize");
        let back = BrainConfig::from_toml_str(&toml).expect("deserialize");
        assert_eq!(config, back);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let partial = "[brain]\nid = \"01J8K\"\nname = \"Personal\"\ncreated_at = \"2025-10-14T09:00:00Z\"\n\n[index]\nchunk_size = 256\n";
        let config = BrainConfig::from_toml_str(partial).expect("deserialize");
        assert_eq!(config.index.chunk_size, 256);
        assert_eq!(config.index.chunk_overlap, 64); // default
        assert_eq!(config.vault.daily_folder, "daily"); // default section
        assert_eq!(config.search.default_mode, "hybrid"); // default section
    }

    #[test]
    fn blank_brain_identity_is_valid_but_empty_is_rejected() {
        // Default identity has empty id/name/created_at -> invalid.
        assert!(BrainConfig::default().validate().is_err());

        let mut config = sample();
        config.brain.name = String::new();
        assert!(matches!(config.validate(), Err(BrainError::Invalid(_))));
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("brain.name"));
    }

    #[test]
    fn invalid_fields_are_rejected_with_clear_errors() {
        let mut config = sample();

        config.index.chunk_size = 0;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("chunk_size"));

        let mut config = sample();
        config.index.chunk_size = 64;
        config.index.chunk_overlap = 64;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("chunk_overlap"));

        let mut config = sample();
        config.search.default_mode = "fuzzy".to_string();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("default_mode"));

        let mut config = sample();
        config.sync.scope = "everything".to_string();
        assert!(config.validate().unwrap_err().to_string().contains("scope"));
    }

    #[test]
    fn malformed_toml_is_an_error_not_a_panic() {
        let err = BrainConfig::from_toml_str("this is not = = toml").unwrap_err();
        assert!(matches!(err, BrainError::Parse { .. }));
    }

    #[test]
    fn load_missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = BrainConfig::load(dir.path()).unwrap_err();
        assert!(matches!(err, BrainError::Io { .. }));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample();
        config.save(dir.path()).expect("save");
        assert!(BrainConfig::path(dir.path()).is_file());
        let loaded = BrainConfig::load(dir.path()).expect("load");
        assert_eq!(config, loaded);
    }

    #[test]
    fn save_rejects_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let err = BrainConfig::default().save(dir.path()).unwrap_err();
        assert!(matches!(err, BrainError::Invalid(_)));
        assert!(!BrainConfig::path(dir.path()).exists());
    }
}
