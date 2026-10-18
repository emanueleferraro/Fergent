//! Integration tests for the vector store, embedder, and embedding pipeline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use fergents_index::{
    content_hash, EmbedInput, Embedder, EmbeddingCache, EmbeddingPipeline, EmbeddingWorker,
    IndexError, SqliteVecStore, StubEmbedder, VectorHit, VectorStore, DEFAULT_EMBED_DIM,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};
use serde_json::Value;

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

/// A fresh in-memory per-brain store with sqlite-vec registered.
fn storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    let migrations = load_migrations_from(&brain_migrations_dir()).unwrap();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations).unwrap())
}

/// Insert a document and one chunk so foreign keys from `embedding_meta` hold.
fn seed_chunk(storage: &Arc<dyn Storage>, document_id: &str, chunk_id: &str) {
    storage
        .execute(
            "INSERT OR IGNORE INTO documents \
             (id, path, kind, title, mime, size, hash, created_at, updated_at, indexed_at) \
             VALUES (?1, ?2, 'note', ?3, 'text/markdown', 0, 'h', 0, 0, 0)",
            &[
                Value::String(document_id.to_string()),
                Value::String(format!("{document_id}.md")),
                Value::String(document_id.to_string()),
            ],
        )
        .unwrap();
    storage
        .execute(
            "INSERT INTO chunks \
             (id, document_id, ordinal, content, token_count, start_offset, end_offset) \
             VALUES (?1, ?2, 0, 'x', 1, 0, 1)",
            &[
                Value::String(chunk_id.to_string()),
                Value::String(document_id.to_string()),
            ],
        )
        .unwrap();
}

/// A 1024-dim unit basis vector.
fn unit(axis: usize) -> Vec<f32> {
    let mut vector = vec![0f32; DEFAULT_EMBED_DIM];
    vector[axis % DEFAULT_EMBED_DIM] = 1.0;
    vector
}

#[test]
fn vector_store_inserts_retrieves_and_searches() {
    let store = SqliteVecStore::new(storage(), DEFAULT_EMBED_DIM);
    assert_eq!(store.count().unwrap(), 0);
    assert!(store.is_empty().unwrap());

    store.upsert("c1", &unit(0)).unwrap();
    store.upsert("c2", &unit(1)).unwrap();
    assert_eq!(store.count().unwrap(), 2);

    let got = store.get("c1").unwrap().expect("c1 vector");
    assert_eq!(got.len(), DEFAULT_EMBED_DIM);
    assert!((got[0] - 1.0).abs() < 1e-6);

    let hits = store.search(&unit(0), 2).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].chunk_id, "c1");

    store.delete("c1").unwrap();
    assert_eq!(store.count().unwrap(), 1);
    assert!(store.get("c1").unwrap().is_none());
}

#[test]
fn vector_store_replaces_existing_vector() {
    let store = SqliteVecStore::new(storage(), DEFAULT_EMBED_DIM);
    store.upsert("c1", &unit(0)).unwrap();
    store.upsert("c1", &unit(5)).unwrap();

    assert_eq!(store.count().unwrap(), 1, "replace must not duplicate");
    let got = store.get("c1").unwrap().unwrap();
    assert!((got[5] - 1.0).abs() < 1e-6);
    assert!(got[0].abs() < 1e-6);
}

#[test]
fn vector_store_rejects_wrong_dimension() {
    let store = SqliteVecStore::new(storage(), DEFAULT_EMBED_DIM);
    let err = store.upsert("c1", &[1.0, 2.0]).unwrap_err();
    assert!(matches!(
        err,
        IndexError::DimensionMismatch {
            expected: 1024,
            got: 2
        }
    ));
}

/// An embedder that counts how many texts and batches it was asked to embed.
#[derive(Clone, Default)]
struct Counter {
    batches: Arc<AtomicUsize>,
    texts: Arc<AtomicUsize>,
}

struct CountingEmbedder {
    inner: StubEmbedder,
    counter: Counter,
}

impl Embedder for CountingEmbedder {
    fn model(&self) -> &str {
        self.inner.model()
    }

    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
        self.counter.batches.fetch_add(1, Ordering::SeqCst);
        self.counter.texts.fetch_add(texts.len(), Ordering::SeqCst);
        self.inner.embed(texts)
    }
}

#[test]
fn identical_text_is_never_re_embedded() {
    let storage = storage();
    seed_chunk(&storage, "doc", "ch1");
    seed_chunk(&storage, "doc", "ch2");

    let counter = Counter::default();
    let pipeline = EmbeddingPipeline::new(
        CountingEmbedder {
            inner: StubEmbedder::default(),
            counter: counter.clone(),
        },
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );

    let items = vec![
        EmbedInput {
            chunk_id: "ch1".into(),
            content: "alpha beta".into(),
        },
        EmbedInput {
            chunk_id: "ch2".into(),
            content: "gamma delta".into(),
        },
    ];

    let first = pipeline.embed(&items).unwrap();
    assert_eq!(first.embedded, 2);
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.batches, 1);

    // Second run: both chunks are cache hits, nothing is embedded.
    let second = pipeline.embed(&items).unwrap();
    assert_eq!(second.embedded, 0);
    assert_eq!(second.cache_hits, 2);
    assert_eq!(second.batches, 0);
    assert_eq!(counter.texts.load(Ordering::SeqCst), 2);
    assert_eq!(counter.batches.load(Ordering::SeqCst), 1);

    // Changing the content invalidates the cache for that chunk only.
    let changed = vec![EmbedInput {
        chunk_id: "ch1".into(),
        content: "alpha beta changed".into(),
    }];
    let third = pipeline.embed(&changed).unwrap();
    assert_eq!(third.embedded, 1);
    assert_eq!(third.cache_hits, 0);
}

#[test]
fn the_cache_records_the_content_hash() {
    let storage = storage();
    seed_chunk(&storage, "doc", "ch1");
    let cache = EmbeddingCache::new(storage.clone());

    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    pipeline
        .embed(&[EmbedInput {
            chunk_id: "ch1".into(),
            content: "hello".into(),
        }])
        .unwrap();

    let meta = cache.get("ch1").unwrap().expect("cache row");
    assert_eq!(meta.model, "stub");
    assert_eq!(meta.dim, DEFAULT_EMBED_DIM);
    assert_eq!(meta.content_hash, content_hash("hello"));
}

#[test]
fn batching_splits_into_the_expected_number_of_batches() {
    let storage = storage();
    let mut items = Vec::new();
    for i in 0..5 {
        let chunk_id = format!("b{i}");
        seed_chunk(&storage, "doc", &chunk_id);
        items.push(EmbedInput {
            chunk_id,
            content: format!("text number {i}"),
        });
    }

    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    )
    .with_batch_size(2);

    let report = pipeline.embed(&items).unwrap();
    assert_eq!(report.embedded, 5);
    assert_eq!(report.batches, 3, "5 items at batch size 2 = 3 batches");
}

/// A minimal in-memory [`VectorStore`] proving the pipeline is swappable.
struct FakeStore {
    dim: usize,
    data: Mutex<HashMap<String, Vec<f32>>>,
}

impl FakeStore {
    fn new(dim: usize) -> Self {
        Self {
            dim,
            data: Mutex::new(HashMap::new()),
        }
    }
}

impl VectorStore for FakeStore {
    fn dim(&self) -> usize {
        self.dim
    }

    fn upsert(&self, chunk_id: &str, vector: &[f32]) -> Result<(), IndexError> {
        self.data
            .lock()
            .unwrap()
            .insert(chunk_id.to_string(), vector.to_vec());
        Ok(())
    }

    fn get(&self, chunk_id: &str) -> Result<Option<Vec<f32>>, IndexError> {
        Ok(self.data.lock().unwrap().get(chunk_id).cloned())
    }

    fn delete(&self, chunk_id: &str) -> Result<(), IndexError> {
        self.data.lock().unwrap().remove(chunk_id);
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<VectorHit>, IndexError> {
        let data = self.data.lock().unwrap();
        let mut hits: Vec<VectorHit> = data
            .iter()
            .map(|(id, vector)| {
                let dot: f32 = query.iter().zip(vector).map(|(a, b)| a * b).sum();
                VectorHit {
                    chunk_id: id.clone(),
                    distance: 1.0 - dot,
                }
            })
            .collect();
        hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        hits.truncate(k);
        Ok(hits)
    }

    fn count(&self) -> Result<usize, IndexError> {
        Ok(self.data.lock().unwrap().len())
    }
}

#[test]
fn pipeline_swaps_the_backend_behind_the_trait() {
    let storage = storage();
    seed_chunk(&storage, "doc", "ch1");

    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        FakeStore::new(DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );

    let report = pipeline
        .embed(&[EmbedInput {
            chunk_id: "ch1".into(),
            content: "swappable".into(),
        }])
        .unwrap();

    assert_eq!(report.embedded, 1);
    assert_eq!(pipeline.embedder().dim(), DEFAULT_EMBED_DIM);
}

#[test]
fn worker_embeds_in_the_background() {
    let storage = storage();
    seed_chunk(&storage, "doc", "ch1");
    seed_chunk(&storage, "doc", "ch2");

    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let worker = EmbeddingWorker::spawn(pipeline);

    let report = worker
        .submit(vec![
            EmbedInput {
                chunk_id: "ch1".into(),
                content: "one".into(),
            },
            EmbedInput {
                chunk_id: "ch2".into(),
                content: "two".into(),
            },
        ])
        .unwrap();
    assert_eq!(report.embedded, 2);

    let store = SqliteVecStore::new(storage, DEFAULT_EMBED_DIM);
    assert_eq!(store.count().unwrap(), 2);
    worker.shutdown();
}
