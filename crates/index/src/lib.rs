//! Fergents per-brain index.
//!
//! Owns content chunking, embeddings behind the [`Embedder`] and
//! [`VectorStore`] traits (`sqlite-vec`), the content-hash embedding cache and
//! batched worker, and (later) the FTS5 + vector + sparse hybrid search
//! pipeline with RRF fusion, reranking, filters, and backlinks (`SEARCH.md`).

mod chunk;
mod embed;
mod embedding;
mod error;
mod vector;

pub use chunk::{
    chunk_attachment_text, chunk_markdown, chunk_plain_text, count_tokens, Chunk, ChunkConfig,
    DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_CHUNK_SIZE, DEFAULT_MIN_CHUNK_SIZE,
};
pub use embed::{Embedder, StubEmbedder, DEFAULT_EMBED_BATCH, DEFAULT_EMBED_DIM, STUB_MODEL};
#[cfg(feature = "embed-real")]
pub use embed::{FastEmbedder, REAL_MODEL};
pub use embedding::{
    content_hash, EmbedInput, EmbedReport, EmbeddingCache, EmbeddingMeta, EmbeddingPipeline,
    EmbeddingWorker,
};
pub use error::IndexError;
pub use vector::{SqliteVecStore, VectorHit, VectorStore};
