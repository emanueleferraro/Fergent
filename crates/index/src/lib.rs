//! Fergents per-brain index.
//!
//! Owns the per-brain SQLite schema (`migrations/brain/`), content chunking,
//! embeddings behind a `VectorStore` trait (`sqlite-vec`), the FTS5 + vector +
//! sparse hybrid search pipeline with RRF fusion and reranking, filters, and
//! backlinks (`SEARCH.md`).

mod chunk;

pub use chunk::{
    chunk_attachment_text, chunk_markdown, chunk_plain_text, count_tokens, Chunk, ChunkConfig,
    DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_CHUNK_SIZE, DEFAULT_MIN_CHUNK_SIZE,
};
