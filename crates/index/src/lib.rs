//! Fergents per-brain index.
//!
//! Owns the per-brain SQLite schema (`migrations/brain/`), content chunking,
//! embeddings behind a `VectorStore` trait (`sqlite-vec`), the FTS5 + vector +
//! sparse hybrid search pipeline with RRF fusion and reranking, filters, and
//! backlinks (`SEARCH.md`).
