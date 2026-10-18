//! Fergents per-brain index.
//!
//! Owns content chunking, embeddings behind the [`Embedder`] and
//! [`VectorStore`] traits (`sqlite-vec`), the content-hash embedding cache and
//! batched worker, the indexing pipeline (vault scan, persisted job queue,
//! `notify` watcher, incremental reindex), and (later) the FTS5 + vector +
//! sparse hybrid search pipeline with RRF fusion, reranking, filters, and
//! backlinks (`SEARCH.md`).

mod chunk;
mod daily;
mod embed;
mod embedding;
mod engine;
mod error;
mod events;
mod fusion;
mod notes;
mod pipeline;
mod queue;
mod rerank;
mod scan;
mod search;
mod templates;
mod vector;
mod watcher;

pub use chunk::{
    chunk_attachment_text, chunk_markdown, chunk_plain_text, count_tokens, Chunk, ChunkConfig,
    DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_CHUNK_SIZE, DEFAULT_MIN_CHUNK_SIZE,
};
pub use daily::{
    now_local, validate_format, DailyNote, DailyNotes, NoteIndexer, DEFAULT_DAILY_FORMAT,
    DEFAULT_DAILY_TEMPLATE,
};
pub use embed::{Embedder, StubEmbedder, DEFAULT_EMBED_BATCH, DEFAULT_EMBED_DIM, STUB_MODEL};
#[cfg(feature = "embed-real")]
pub use embed::{FastEmbedder, REAL_MODEL};
pub use embedding::{
    content_hash, EmbedInput, EmbedReport, EmbeddingCache, EmbeddingMeta, EmbeddingPipeline,
    EmbeddingWorker,
};
pub use engine::SearchEngine;
pub use error::IndexError;
pub use events::{EventSink, NullSink, RecordingSink};
pub use fusion::{reciprocal_rank_fusion, FusedHit, RankedList, Stage, StageScore, RRF_K};
pub use notes::{
    backlinks, broken_links, classify_link, clear_note_data, dead_ends, delete_tag,
    frontmatter_tags, link_kinds, list_tags, orphans, outline, persist_links, persist_tags,
    rename_tag, resolve_incoming_links, resolve_target, unlinked_mentions, upsert_notes_meta,
    LinkRecord, NoteRef, OutlineEntry, Tag, UnlinkedMention,
};
pub use pipeline::{
    IndexOutcome, Indexer, IndexerConfig, ProcessReport, ScanReport, DEFAULT_MAX_EMBED_TOKENS,
    DEFAULT_MAX_FILE_BYTES,
};
pub use queue::{IndexJob, IndexJobOp, IndexQueue, QueueStats};
#[cfg(feature = "rerank-real")]
pub use rerank::{CrossEncoderReranker, REAL_RERANK_MODEL};
pub use rerank::{Reranker, StubReranker, RERANK_MIN_TOKENS, RERANK_TOP_K, STUB_RERANK_MODEL};
pub use scan::{classify, DocumentKind, ScannedFile, VaultScanner, BUILTIN_IGNORES};
pub use search::{
    analyze_query, AnalyzedQuery, CandidateCounts, HitMetadata, SearchFilters, SearchHit,
    SearchMode, SearchRequest, SearchResponse, StageScores, CANDIDATE_K, DEFAULT_TOP_K,
};
pub use templates::{
    create_template, list_templates, render, template_file_name, RenderedTemplate, TemplateInfo,
    TemplateVariables, DATETIME_FORMAT, DATE_FORMAT, TIME_FORMAT,
};
pub use vector::{SqliteVecStore, VectorHit, VectorStore};
pub use watcher::VaultWatcher;
