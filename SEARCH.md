# Search

> Hybrid semantic search: chunking, embeddings, BM25, vector, fusion,
> reranking, filters, and query understanding.

**Audience:** developers, AI developers.
**Companion docs:** `BRAIN.md`, `GRAPH.md`, `LLM.md`.

---

## Table of contents

1. [Principles](#1-principles)
2. [The pipeline](#2-the-pipeline)
3. [Chunking](#3-chunking)
4. [Embeddings](#4-embeddings)
5. [BM25](#5-bm25)
6. [Vector search](#6-vector-search)
7. [Fusion](#7-fusion)
8. [Reranking](#8-reranking)
9. [Query understanding](#9-query-understanding)
10. [Filters](#10-filters)
11. [Results](#11-results)
12. [Caching](#12-caching)
13. [Indexing pipeline](#13-indexing-pipeline)
14. [Multimodal search](#14-multimodal-search)
15. [Code search](#15-code-search)
16. [Performance](#16-performance)
17. [Evaluation](#17-evaluation)
18. [AI operations](#18-ai-operations)
19. [Observability](#19-observability)
20. [Appendix: search API](#20-appendix-search-api)

---

## 1. Principles

- **Hybrid by default**: BM25 + vector, fused, reranked.
- **Per-brain**: each brain has its own index.
- **Incremental**: only changed content is re-indexed.
- **Multilingual**: the embedding model handles 100+ languages.
- **Local-first**: embeddings and reranker run on-device by default.
- **Explainable**: results carry scores, sources, and match reasons.

---

## 2. The pipeline

```
                ┌──────────────┐
   query ──────▶│   QUERY      │
                │   ANALYSIS   │
                └──────┬───────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
   ┌─────────┐   ┌─────────┐   ┌─────────┐
   │  BM25   │   │ VECTOR  │   │ FILTERS │
   │ (FTS5)  │   │(sqlite- │   │         │
   │         │   │  vec)   │   │         │
   └────┬────┘   └────┬────┘   └────┬────┘
        │              │              │
        └──────────────┼──────────────┘
                       ▼
                ┌──────────────┐
                │    FUSION    │
                │     (RRF)    │
                └──────┬───────┘
                       ▼
                ┌──────────────┐
                │   RERANK     │
                │ (cross-enc.) │
                └──────┬───────┘
                       ▼
                ┌──────────────┐
                │   RESULTS    │
                └──────────────┘
```

---

## 3. Chunking

### 3.1 Why chunking matters

RAG quality is dominated by chunking. A good chunk is:

- Self-contained (understandable without neighbors).
- Focused (one topic).
- Short enough to embed well (target 512 tokens).
- Long enough to carry meaning (minimum 100 tokens).

### 3.2 Chunkers by content type

#### Markdown

1. Split by headings (H1, H2, H3).
2. Within each section, split by paragraphs.
3. If a paragraph is too long, split by sentences.
4. Merge adjacent chunks until the target size is reached.
5. Add overlap (64 tokens) between consecutive chunks.

Each chunk keeps a `heading_path` (e.g. "Title > Section > Sub") for context.

#### Code

1. Parse with tree-sitter.
2. Split by top-level declarations (functions, classes, modules).
3. If a declaration is too long, split by nested declarations.
4. Include the surrounding context (imports, class header) in each chunk.

Chunks keep the file path, language, and symbol name.

#### PDF

1. Extract text with layout (using a layout-aware extractor).
2. Split by headings (detected via font size, boldness).
3. Within sections, split by paragraphs.
4. Merge to target size.

#### HTML

1. Extract main content (readability).
2. Convert to markdown.
3. Use the markdown chunker.

#### Plain text

1. Split by paragraphs.
2. Split by sentences if needed.
3. Merge to target size.

### 3.3 Parameters

| Parameter | Default | Notes |
|---|---|---|
| `chunk_size` | 512 | target tokens |
| `chunk_overlap` | 64 | tokens |
| `min_chunk_size` | 100 | below this, merge with neighbor |
| `max_chunk_size` | 1024 | above this, force split |

Configurable per brain.

### 3.4 Chunk metadata

Every chunk stores:

- `id`, `document_id`, `ordinal`.
- `content`, `token_count`.
- `start_offset`, `end_offset`.
- `heading_path`.
- `language` (for code).
- `symbol` (for code).

This metadata is used for display, filtering, and re-ranking.

---

## 4. Embeddings

### 4.1 Model

**Default**: `BAAI/bge-m3` via fastembed-rs.

- Multilingual (100+ languages).
- 1024 dimensions.
- Dense + sparse output (we use both).
- 8k token context.

### 4.2 Generation

- Batched (default 64).
- Cached by content hash.
- Generated in a background worker.
- GPU-accelerated if available.

### 4.3 Storage

- Dense vectors: `vec_chunks` (sqlite-vec).
- Sparse vectors: stored as JSON in `embedding_meta` or a separate table.
- Metadata: `embedding_meta` (model, dim, created_at).

### 4.4 Change of model

Changing the embedding model requires a full re-index. The UI warns and
offers to re-index in the background.

---

## 5. BM25

### 5.1 Index

FTS5 with `unicode61` tokenizer, `remove_diacritics 2`, and Porter stemming
for English. CJK uses a custom tokenizer.

### 5.2 Query

The query is tokenized and expanded (stemming, synonyms if configured).

### 5.3 Scoring

FTS5 returns BM25 scores. We keep the top-K (default 100) for fusion.

### 5.4 Highlighting

FTS5 supports snippet highlighting. We use it to show match context in
results.

---

## 6. Vector search

### 6.1 Approximate vs exact

For brains < 1M chunks: exact search with SIMD (fast enough, ~10 ms for
top-10 over 100k chunks).

For larger brains: ANN via `sqlite-vec`'s IVF index (built periodically).

### 6.2 Query

The query is embedded with the same model as the chunks. The top-K (default
100) nearest chunks are retrieved.

### 6.3 Distance

Cosine similarity by default. Vectors are normalized on insert, so cosine =
dot product.

---

## 7. Fusion

### 7.1 Reciprocal Rank Fusion (RRF)

RRF combines ranked lists without needing score normalization:

```
score(d) = Σ over lists L: 1 / (k + rank_L(d))
```

Where `k` = 60 (default). Each list contributes proportionally to its rank.

### 7.2 Why RRF

- **Simple**: no tuning of weights.
- **Robust**: works across heterogeneous scorers.
- **Effective**: consistently strong in benchmarks.

### 7.3 Alternative: weighted sum

Optionally, weights can be configured per list:

```toml
[search.fusion]
method = "rrf"   # or "weighted"
k = 60
weights = { bm25 = 1.0, vector = 1.0, sparse = 0.5 }
```

---

## 8. Reranking

### 8.1 Model

**Default**: `BAAI/bge-reranker-v2-m3` via fastembed-rs.

- Cross-encoder (query + document jointly encoded).
- Multilingual.
- Local.

### 8.2 Process

1. Take the top-K from fusion (default 20).
2. Rerank them.
3. Return the top-N (default 8).

### 8.3 When to rerank

- Always by default.
- Skipped for trivial queries (< 3 tokens) or when latency is critical.
- Configurable per brain (`search.rerank: true | false | auto`).

### 8.4 Alternatives

- Cohere Rerank (API).
- Voyage Rerank (API).
- Cross-encoder models via fastembed.

---

## 9. Query understanding

### 9.1 Light analysis

- Tokenize.
- Detect language.
- Strip stopwords for BM25.
- Detect quoted phrases (exact match).
- Detect filters (`tag:foo`, `path:bar`, `type:note`).
- Detect date ranges (`last week`, `2025-10`).

### 9.2 Heavy analysis (optional)

For complex queries, an LLM can:

- Rewrite the query for better retrieval.
- Generate multiple query variants (multi-query).
- Extract entities and relations.
- Produce a structured query.

This is opt-in (costs tokens).

### 9.3 Multi-query

For ambiguous queries, generate 3-5 variants, search each, and merge. This
improves recall significantly.

### 9.4 HyDE

Optionally, generate a hypothetical answer and search for it. Improves
recall for question-like queries.

---

## 10. Filters

### 10.1 Syntax

Filters are supported in the query and as structured parameters:

```
MCP security tag:research path:notes/ -tag:archive after:2025-01-01
```

Or:

```json
{
  "query": "MCP security",
  "filters": {
    "tags": ["research"],
    "path": "notes/",
    "exclude_tags": ["archive"],
    "after": "2025-01-01",
    "type": "note"
  }
}
```

### 10.2 Supported filters

- `tag` / `-tag`
- `path` / `-path` (prefix match)
- `type` (note, attachment, code)
- `language` (for code)
- `after` / `before` (date)
- `has` (link, backlink, attachment)
- `frontmatter.key` (any frontmatter field)
- `brain` (which brain)

### 10.3 Application

Filters are applied:

- **Before** BM25 and vector search (for indexed fields).
- **After** (for computed fields).

Filtered-out chunks don't reach fusion.

---

## 11. Results

### 11.1 Result shape

```json
{
  "chunk_id": "...",
  "document_id": "...",
  "path": "notes/mcp-security.md",
  "title": "MCP Security",
  "heading_path": "MCP Security > Threats > Prompt Injection",
  "content": "...",
  "snippet": "...matched <mark>text</mark>...",
  "score": 0.87,
  "scores": {
    "bm25": 0.42,
    "vector": 0.91,
    "sparse": 0.55,
    "fusion": 0.78,
    "rerank": 0.87
  },
  "metadata": {
    "tags": ["research"],
    "created": "2025-10-01",
    "brain": "personal"
  }
}
```

### 11.2 Grouping

Results can be grouped by:

- Document (default): one result per document, with the best chunk.
- Chunk: one result per chunk.
- Section: one result per heading section.

### 11.3 Highlighting

Matched terms are highlighted in the snippet. For vector matches (no exact
terms), the snippet is the chunk's most relevant sentence (via a
sentence-similarity heuristic).

### 11.4 Citation

Every result carries a citation string:

```
[1] notes/mcp-security.md#threats (score 0.87)
```

Agents use these citations in their responses.

---

## 12. Caching

### 12.1 Query cache

Identical queries (with the same filters) return cached results. TTL: 5 min
by default (configurable). Invalidated when the underlying brain changes.

### 12.2 Embedding cache

Identical text is never re-embedded. Cached by content hash.

### 12.3 Rerank cache

Identical (query, chunk) pairs are never re-ranked.

### 12.4 Semantic cache

For repeated queries with similar meaning, an optional semantic cache
returns the cached result set if the query embedding similarity exceeds a
threshold.

---

## 13. Indexing pipeline

### 13.1 Trigger

- File created, changed, deleted.
- Manual reindex.
- Brain opened (incremental scan).

### 13.2 Steps

```
detect → parse → chunk → embed → store → extract entities → update graph
```

Each step is a job in the indexer queue. Jobs are:

- **Prioritized**: user-visible first.
- **Retried**: on transient failures.
- **Observable**: progress shown in the UI.

### 13.3 Concurrency

- Multiple files indexed in parallel (bounded).
- Embeddings batched.
- Entity extraction parallelized.

### 13.4 Recovery

If indexing is interrupted (crash, shutdown), the queue is persisted. On
restart, pending jobs resume.

---

## 14. Multimodal search

### 14.1 Images

- Images are captioned (via a vision model) or described (via the user).
- Captions are embedded and searchable.
- Optionally, CLIP-style image embeddings for image-to-image search.

### 14.2 Audio

- Audio is transcribed (Whisper).
- The transcript is chunked and embedded.

### 14.3 Video

- Key frames extracted.
- Audio transcribed.
- Frames captioned.
- All text embedded.

### 14.4 PDFs

- Text extracted and embedded.
- Page images indexed for visual search (optional).

---

## 15. Code search

### 15.1 Differences from prose

- Chunks are AST-based.
- Symbols (function names, classes) are indexed.
- Search supports:
  - Natural language ("where do we handle auth?").
  - Symbol search ("parse_config").
  - Regex search (on content).
  - Structural search (tree-sitter queries).

### 15.2 Language support

Built-in: Rust, TypeScript, JavaScript, Python, Go, Java, C/C++, Ruby, PHP,
Lua, Bash, SQL, HTML, CSS, JSON, YAML, TOML, Markdown.

### 15.3 Cross-reference

Code chunks are linked to:
- Their file.
- Their symbol.
- Their callers (via static analysis, optional).
- Their tests (if detectable).

---

## 16. Performance

### 16.1 Targets

| Operation | Target |
|---|---|
| BM25 (100k chunks) | < 20 ms |
| Vector (100k chunks) | < 50 ms |
| Fusion | < 5 ms |
| Rerank (20 candidates) | < 100 ms |
| Total search | < 200 ms |

### 16.2 Techniques

- **Index warm-up**: keep hot indexes in memory.
- **Parallel search**: BM25 and vector run in parallel.
- **Early termination**: stop when enough results are found.
- **Caching**: query, embedding, rerank caches.

---

## 17. Evaluation

### 17.1 Metrics

- **Recall@k**: how many relevant chunks are in the top-k.
- **MRR**: mean reciprocal rank of the first relevant result.
- **NDCG**: normalized discounted cumulative gain.
- **Latency**: p50, p95, p99.

### 17.2 Benchmarks

- A curated set of queries per brain (built over time).
- A synthetic set generated by the AI.
- Public benchmarks for the embedding model.

### 17.3 Continuous evaluation

The indexer logs queries and (optionally) user feedback. Periodic jobs
compute metrics and surface regressions.

---

## 18. AI operations

The AI can:

- **Search** any brain with any mode.
- **Explain** why a result was returned.
- **Suggest** better queries.
- **Re-index** a brain.
- **Tune** parameters (chunk size, top-k).
- **Evaluate** search quality on a set of queries.

---

## 19. Observability

Per search:

- Query, mode, filters.
- Latency (per stage).
- Number of candidates.
- Top score.
- Cache hit/miss.

Aggregated:

- Query volume over time.
- Latency distribution.
- Cache hit rate.
- Zero-result rate.
- Click-through (if UI feedback is enabled).

---

## 20. Appendix: search API

```json
{
  "command": "search.query",
  "params": {
    "query": "MCP security",
    "brains": ["personal"],
    "mode": "hybrid",
    "top_k": 8,
    "filters": {
      "tags": ["research"],
      "after": "2025-01-01"
    },
    "group_by": "document",
    "rerank": true,
    "explain": false
  }
}
```

Response:

```json
{
  "results": [ ... ],
  "meta": {
    "latency_ms": 142,
    "candidates": { "bm25": 100, "vector": 100 },
    "cache": { "query": false, "embedding": true, "rerank": false }
  }
}
```

---

*End of `SEARCH.md`.*