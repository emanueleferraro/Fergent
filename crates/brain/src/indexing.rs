//! The indexing seam for brain lifecycle operations.
//!
//! The manager drives indexing through the [`BrainIndexer`] trait so the
//! lifecycle is testable without the full pipeline. [`VaultIndexer`] is the
//! real implementation: it opens the per-brain store and runs the existing
//! scan + process pipeline (Task 8).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_index::{
    Embedder, EmbeddingCache, EmbeddingPipeline, EventSink, Indexer, IndexerConfig, VectorStore,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};

use crate::error::BrainError;
use crate::store::INDEX_DB;

/// The outcome of indexing a brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexReport {
    /// Documents created or updated by this run.
    pub documents: usize,
}

/// Runs the indexing pipeline for one brain.
///
/// The manager calls this on create, open, and reindex. Tests use a stub; the
/// kernel wires [`VaultIndexer`].
pub trait BrainIndexer: Send + Sync {
    /// Open the per-brain store and run scan + process (full or incremental).
    fn index_brain(
        &self,
        brain_id: &str,
        brain_dir: &Path,
        vault_root: &Path,
    ) -> Result<IndexReport, BrainError>;

    /// Flush pending work and checkpoint the store (called on close).
    fn flush(&self, _brain_dir: &Path) -> Result<(), BrainError> {
        Ok(())
    }
}

/// Convenience alias for the embedder/vector factories.
type EmbedderFactory<E> = Arc<dyn Fn() -> E + Send + Sync>;
type VectorFactory<V> = Arc<dyn Fn(Arc<dyn Storage>) -> V + Send + Sync>;

/// The real [`BrainIndexer`]: drives `fergents-index` over a brain's vault.
pub struct VaultIndexer<E: Embedder, V: VectorStore> {
    migrations_dir: PathBuf,
    sink: Arc<dyn EventSink>,
    embed_dim: usize,
    make_embedder: EmbedderFactory<E>,
    make_vector: VectorFactory<V>,
}

impl<E: Embedder, V: VectorStore> VaultIndexer<E, V> {
    /// Build an indexer over the per-brain migrations in `migrations_dir`.
    pub fn new(
        migrations_dir: impl Into<PathBuf>,
        sink: Arc<dyn EventSink>,
        embed_dim: usize,
        make_embedder: impl Fn() -> E + Send + Sync + 'static,
        make_vector: impl Fn(Arc<dyn Storage>) -> V + Send + Sync + 'static,
    ) -> Self {
        Self {
            migrations_dir: migrations_dir.into(),
            sink,
            embed_dim,
            make_embedder: Arc::new(make_embedder),
            make_vector: Arc::new(make_vector),
        }
    }

    fn open_storage(&self, brain_dir: &Path) -> Result<Arc<dyn Storage>, BrainError> {
        register_sqlite_vec();
        let path = brain_dir.join(INDEX_DB);
        // The per-brain schema lives directly in `migrations_dir` (not under a
        // `global/` subdirectory), so load it explicitly.
        let migrations = load_migrations_from(&self.migrations_dir)?;
        let storage = SqliteStorage::open_with_migrations(&path, &migrations)?;
        Ok(Arc::new(storage))
    }
}

impl<E: Embedder, V: VectorStore> BrainIndexer for VaultIndexer<E, V> {
    fn index_brain(
        &self,
        brain_id: &str,
        brain_dir: &Path,
        vault_root: &Path,
    ) -> Result<IndexReport, BrainError> {
        let storage = self.open_storage(brain_dir)?;
        let vector = (self.make_vector)(storage.clone());
        let pipeline = EmbeddingPipeline::new(
            (self.make_embedder)(),
            vector,
            EmbeddingCache::new(storage.clone()),
        );
        let indexer = Indexer::new(
            storage,
            IndexerConfig {
                embed_dim: self.embed_dim,
                ..IndexerConfig::new(brain_id, vault_root)
            },
            pipeline,
            self.sink.clone(),
        )
        .map_err(|e| BrainError::Index(e.to_string()))?;
        indexer
            .scan_and_enqueue()
            .map_err(|e| BrainError::Index(e.to_string()))?;
        let processed = indexer
            .process_pending()
            .map_err(|e| BrainError::Index(e.to_string()))?;
        Ok(IndexReport {
            documents: processed.processed,
        })
    }

    fn flush(&self, brain_dir: &Path) -> Result<(), BrainError> {
        let path = brain_dir.join(INDEX_DB);
        if !path.is_file() {
            return Ok(());
        }
        let storage = self.open_storage(brain_dir)?;
        storage.query("PRAGMA wal_checkpoint(TRUNCATE)", &[])?;
        Ok(())
    }
}
