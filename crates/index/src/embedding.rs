//! Content-hash embedding cache (`embedding_meta`) and the batched embedding
//! worker (`SEARCH.md` §4.2–§4.3).

use std::sync::mpsc::{channel, sync_channel, Sender, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use fergents_storage::{Storage, Value};
use sha2::{Digest, Sha256};

use crate::embed::{Embedder, DEFAULT_EMBED_BATCH};
use crate::error::IndexError;
use crate::vector::VectorStore;

/// A chunk to embed: its id and current text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedInput {
    /// The chunk id (the `chunks.id` primary key).
    pub chunk_id: String,
    /// The chunk text.
    pub content: String,
}

/// The outcome of an embedding run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmbedReport {
    /// Chunks embedded (cache misses).
    pub embedded: usize,
    /// Chunks skipped because a valid cached vector already existed.
    pub cache_hits: usize,
    /// Embedder batches executed.
    pub batches: usize,
}

/// A row of `embedding_meta`: what was embedded for a chunk and from what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingMeta {
    /// The chunk id.
    pub chunk_id: String,
    /// The embedder model id.
    pub model: String,
    /// The vector dimension.
    pub dim: usize,
    /// The hash of the embedded content.
    pub content_hash: String,
    /// The sparse vector as JSON, when the model produces one.
    pub sparse: Option<String>,
    /// Creation time (ms since the Unix epoch).
    pub created_at: i64,
}

/// The SHA-256 content hash used to detect unchanged chunks.
pub fn content_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The `embedding_meta` cache: which chunks have a valid stored vector.
pub struct EmbeddingCache {
    storage: Arc<dyn Storage>,
}

impl EmbeddingCache {
    /// Create a cache over `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Fetch the stored metadata for `chunk_id`, if any.
    pub fn get(&self, chunk_id: &str) -> Result<Option<EmbeddingMeta>, IndexError> {
        let rows = self.storage.query(
            "SELECT chunk_id, model, dim, content_hash, sparse, created_at \
             FROM embedding_meta WHERE chunk_id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        Ok(rows.into_iter().next().map(row_to_meta))
    }

    /// Whether a valid vector already exists for `chunk_id` with the given
    /// model, dimension, and content hash.
    pub fn is_fresh(
        &self,
        chunk_id: &str,
        model: &str,
        dim: usize,
        hash: &str,
    ) -> Result<bool, IndexError> {
        Ok(self.get(chunk_id)?.is_some_and(|meta| {
            meta.model == model && meta.dim == dim && meta.content_hash == hash
        }))
    }

    /// Insert or update the metadata for a chunk.
    pub fn put(&self, meta: &EmbeddingMeta) -> Result<(), IndexError> {
        self.storage.execute(
            "INSERT INTO embedding_meta (chunk_id, model, dim, content_hash, sparse, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(chunk_id) DO UPDATE SET model = excluded.model, dim = excluded.dim, \
             content_hash = excluded.content_hash, sparse = excluded.sparse, \
             created_at = excluded.created_at",
            &[
                Value::String(meta.chunk_id.clone()),
                Value::String(meta.model.clone()),
                Value::from(meta.dim as u64),
                Value::String(meta.content_hash.clone()),
                meta.sparse.clone().map_or(Value::Null, Value::String),
                Value::from(meta.created_at),
            ],
        )?;
        Ok(())
    }

    /// Remove the metadata for `chunk_id`. A missing id is not an error.
    pub fn remove(&self, chunk_id: &str) -> Result<(), IndexError> {
        self.storage.execute(
            "DELETE FROM embedding_meta WHERE chunk_id = ?1",
            &[Value::String(chunk_id.to_string())],
        )?;
        Ok(())
    }
}

fn row_to_meta(row: fergents_storage::Row) -> EmbeddingMeta {
    EmbeddingMeta {
        chunk_id: string_field(&row, "chunk_id"),
        model: string_field(&row, "model"),
        dim: row.get("dim").and_then(Value::as_u64).unwrap_or(0) as usize,
        content_hash: string_field(&row, "content_hash"),
        sparse: row
            .get("sparse")
            .and_then(Value::as_str)
            .map(str::to_string),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn string_field(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Batched, cache-aware embedding: embeds only what changed and stores dense
/// vectors in the [`VectorStore`].
pub struct EmbeddingPipeline<E: Embedder, V: VectorStore> {
    embedder: E,
    store: V,
    cache: EmbeddingCache,
    batch_size: usize,
}

impl<E: Embedder, V: VectorStore> EmbeddingPipeline<E, V> {
    /// Create a pipeline with the default batch size.
    pub fn new(embedder: E, store: V, cache: EmbeddingCache) -> Self {
        Self {
            embedder,
            store,
            cache,
            batch_size: DEFAULT_EMBED_BATCH,
        }
    }

    /// Override the batch size.
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// The embedder used by this pipeline.
    pub fn embedder(&self) -> &E {
        &self.embedder
    }

    /// Embed the given chunks, skipping fresh cache hits, and store the vectors.
    pub fn embed(&self, items: &[EmbedInput]) -> Result<EmbedReport, IndexError> {
        let model = self.embedder.model();
        let dim = self.embedder.dim();

        let mut pending: Vec<&EmbedInput> = Vec::new();
        let mut report = EmbedReport::default();
        for item in items {
            let hash = content_hash(&item.content);
            let fresh = self.cache.is_fresh(&item.chunk_id, model, dim, &hash)?
                && self.store.get(&item.chunk_id)?.is_some();
            if fresh {
                report.cache_hits += 1;
            } else {
                pending.push(item);
            }
        }

        for batch in pending.chunks(self.batch_size) {
            let texts: Vec<String> = batch.iter().map(|item| item.content.clone()).collect();
            let vectors = self.embedder.embed(&texts)?;
            if vectors.len() != batch.len() {
                return Err(IndexError::Embedding(format!(
                    "embedder returned {} vectors for {} inputs",
                    vectors.len(),
                    batch.len()
                )));
            }
            for (item, vector) in batch.iter().zip(vectors) {
                self.store.upsert(&item.chunk_id, &vector)?;
                self.cache.put(&EmbeddingMeta {
                    chunk_id: item.chunk_id.clone(),
                    model: model.to_string(),
                    dim,
                    content_hash: content_hash(&item.content),
                    sparse: None,
                    created_at: now_ms(),
                })?;
                report.embedded += 1;
            }
            report.batches += 1;
        }
        Ok(report)
    }
}

/// A background thread that runs an [`EmbeddingPipeline`] off the caller's
/// thread. Jobs are submitted in batches and reply with their report.
pub struct EmbeddingWorker {
    tx: Sender<Job>,
    handle: Option<JoinHandle<()>>,
}

enum Job {
    Batch(Vec<EmbedInput>, SyncSender<Result<EmbedReport, IndexError>>),
    Shutdown,
}

impl EmbeddingWorker {
    /// Spawn a worker owning `pipeline`.
    pub fn spawn<E, V>(pipeline: EmbeddingPipeline<E, V>) -> Self
    where
        E: Embedder + 'static,
        V: VectorStore + 'static,
    {
        let (tx, rx) = channel::<Job>();
        let handle = std::thread::Builder::new()
            .name("fergents-embedder".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Batch(items, reply) => {
                            let _ = reply.send(pipeline.embed(&items));
                        }
                        Job::Shutdown => break,
                    }
                }
            })
            .expect("spawn the embedding worker thread");
        Self {
            tx,
            handle: Some(handle),
        }
    }

    /// Submit a batch and block until the worker reports back.
    pub fn submit(&self, items: Vec<EmbedInput>) -> Result<EmbedReport, IndexError> {
        let (reply_tx, reply_rx) = sync_channel(1);
        self.tx
            .send(Job::Batch(items, reply_tx))
            .map_err(|_| IndexError::Embedding("embedding worker has stopped".to_string()))?;
        reply_rx
            .recv()
            .map_err(|_| IndexError::Embedding("embedding worker dropped the reply".to_string()))?
    }

    /// Stop the worker and join its thread.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Job::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for EmbeddingWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Wall-clock time in ms since the Unix epoch.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
