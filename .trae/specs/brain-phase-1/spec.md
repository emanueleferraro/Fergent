# Brain Phase 1 Spec

## Why

Phase 0 delivered the skeleton: a kernel daemon, a persisted event bus, a
JSON-RPC protocol over a local socket, a Tauri shell, and a CLI — with an
**empty** `brains` table and no brain creation (Phase 0 spec Appendix B
explicitly reserved `migrations/brain/` and all brain features for Phase 1).

Phase 1 (`ROADMAP.md` §5) turns Fergents into a working **second brain**: a
markdown vault on disk as the source of truth, a per-brain SQLite index, a
file watcher with incremental reindex, full-text + semantic hybrid search,
backlinks and unlinked mentions, a knowledge graph, and multi-brain
management — all fully offline. This is the foundation every later phase
(agents read/write the brain, the IDE edits it, workflows trigger on it)
depends on.

This spec delivers exactly **ROADMAP.md §5 (Phase 1)** and nothing beyond it.

## What Changes

This change is **additive** to the Phase 0 skeleton. Phase 0 code is touched
only where Phase 1 strictly requires it (the brains registry integration, the
protocol surface, and the `Storage` reuse for per-brain databases).

- **New crates**: `crates/markdown` (markdown flavor parser), `crates/brain`
  (brain model, `brain.toml`, vault layout, lifecycle, multi-brain ops,
  `.fergbrain`), `crates/index` (per-brain index schema + migrations, chunker,
  embeddings behind a `VectorStore` trait, FTS5 + `sqlite-vec`, hybrid search
  pipeline, filters, backlinks), `crates/graph` (`GraphStore` trait, CozoDB,
  entity/relation extraction).
- **Per-brain schema** in `migrations/brain/` (new), opened with the existing
  `SqliteStorage` + migration runner, per brain at `<brain>/index.sqlite`.
- **File watcher** (`notify`) → incremental reindex with a **persisted job
  queue** (`index_jobs`), recovering across restarts.
- **Search**: FTS5 (BM25) + `sqlite-vec` dense + BGE-M3 sparse, fused with
  RRF (k=60), reranked with a cross-encoder; filters (tag/path/type/date/
  frontmatter/brain); `search.query` command.
- **Graph**: `graph.db` (CozoDB) behind a `GraphStore` trait, SQLite mirror
  (`entities`, `relations`, `entity_mentions`), entity extraction (regex + NER;
  LLM **off by default**).
- **Multi-brain**: create from template, attach/detach to project, duplicate,
  move, export/import `.fergbrain`, save-as-template, cross-brain search and
  cross-brain links.
- **UI** (desktop): brain sidebar (file tree, tags, backlinks, outline,
  search), a markdown editor with live preview (CodeMirror 6 — **not** Monaco,
  which is Phase 3), a graph view (React Flow in Phase 1), a search panel, and
  a per-brain settings page — built **only** from `style/`.
- **Protocol**: new Phase 1 commands and events, with Rust + TypeScript codegen
  (same pipeline as Phase 0).

No **BREAKING** changes to Phase 0 behavior: `brain.list` keeps working and
now returns real rows once brains exist; all Phase 0 commands are unchanged.

## Impact

- **Affected docs (source of truth)**: `BRAIN.md` §2–§19; `SEARCH.md` §2–§20;
  `GRAPH.md` §2–§15; `ARCHITECTURE.md` §4.4 (topics), §7 (storage), §8
  (multi-brain), (ADR-004, ADR-013); `STACK.md` §4.2, §4.3, §5, §6;
  `ROADMAP.md` §5; `UI.md` (brain layout, sidebar §5.2, editor, graph view);
  `SECURITY.md` §7 (filesystem sandbox — watcher scope); `style/`
  (MANDATORY design system).
- **Affected code (new)**: `crates/{markdown,brain,index,graph}`,
  `migrations/brain/0001_initial.sql`, `protocol/schema/protocol.schema.json`
  (extended), regenerated `crates/protocol/src/generated.rs` and
  `packages/protocol/src/generated.ts`, desktop features under
  `apps/desktop/src/features/{brain,editor,search,graph,settings}`.
- **Affected code (modified, minimal)**: `crates/core` (register the brain
  service; extend the brains registry to real CRUD), `crates/api` (dispatch
  new commands), `crates/fergents` (CLI brain/search commands),
  `migrations/global/` (a new `0002_*.sql` only if strictly required — brains
  columns already exist).
- **Out of scope (later phases)**: LLM router, MCP host/server, agent runtime
  (Phase 2); Monaco/LSP/terminal/IDE (Phase 3); skills, plugins (Phase 4);
  workflows (Phase 5); dashboard (Phase 6); sync, mobile (Phase 7).

## Decisions (resolved)

The following decisions resolve the open questions raised during spec review.
They are normative for Phase 1.

| # | Topic | Decision |
|---|---|---|
| D1 | Markdown editor | **CodeMirror 6** in Phase 1. Monaco + LSP deferred to Phase 3. |
| D2 | Graph renderer | **React Flow** in Phase 1 (not custom WebGL). Migrate to WebGL only if the graph exceeds ~5k nodes and React Flow degrades. |
| D3 | Reranker | Behind a `Reranker` trait. Default: the closest `fastembed` cross-encoder, or `bge-reranker-v2-m3` via `ort` if available. Swappable. |
| D4 | NER | Behind an `EntityExtractor` trait. Regex always on. NER uses an ONNX token-classification model via `ort` when available; degrades to regex-only (never fails). |
| D5 | CI and heavy models | Embeddings and reranker behind traits with a deterministic **stub** used in tests. The real model loads lazily, never in CI. |
| D6 | Templates location | `$FERGENTS_HOME/templates/brains/` (defaults to `~/.fergents/templates/brains/`). |
| D7 | `DATA_MODEL.sql` | Does not exist and will not be created. Extend `DATA_MODEL.md` with the per-brain schema. |
| D8 | Code chunking | tree-sitter AST chunking and code search are deferred to Phase 3. Phase 1 ships markdown, plain text, and attachment-text (PDF) chunkers. |
| D9 | Crate granularity | Four new crates: `markdown`, `brain`, `index`, `graph`. |
| D10 | Daily note format | `strftime` `%Y-%m-%d`, validated per platform. |
| D11 | FTS5 tokenizer | Default `unicode61 remove_diacritics 2` (multilingual-friendly, no `porter`). Configurable per brain via `index.fts_tokenizer`. |
| D12 | Phase 0 gaps | **Fix the 4 Phase 0 gaps first**, as 4 small PRs, before Phase 1 implementation begins. See Appendix F. |

## ADDED Requirements

### Requirement: Brain Model and `brain.toml`

A **brain** SHALL be a folder on disk containing a `brain.toml` (identity +
configuration, `BRAIN.md` §6) and a `vault/` of markdown files, with a
regenerable `index.sqlite` and optional `graph.db`. The brain's stable
identity SHALL be a ULID/UUID `id` in `brain.toml` (`BRAIN.md` §10.4), not its
path.

#### Scenario: `brain.toml` is the single source of identity and config

- **WHEN** a brain folder is read
- **THEN** `brain.toml` is parsed and validated against a schema covering
  `[brain]`, `[vault]`, `[markdown]`, `[index]`, `[graph]`, `[search]`,
  `[sync]`, `[git]` sections with the keys in `BRAIN.md` §6.1
- **AND** a missing/invalid `brain.toml` produces a clear error, never a panic

#### Scenario: The index is regenerable

- **WHEN** `index.sqlite` and `graph.db` are deleted and the brain is reopened
- **THEN** both are rebuilt from `brain.toml` + `vault/` with no data loss

#### Scenario: Ignored files

- **WHEN** the indexer scans the vault
- **THEN** it ignores `.fergents/`, `index.sqlite*`, `graph.db*`, `.DS_Store`,
  `Thumbs.db`, and any path matched by `.brainignore` (gitignore syntax)

### Requirement: Per-Brain Index Schema and Migrations

The system SHALL ship a per-brain schema in `migrations/brain/0001_initial.sql`
applied on brain open via the existing migration runner, recording versions in
a per-brain `schema_version` table, and SHALL refuse to open a brain whose
schema is newer than the kernel understands (`ARCHITECTURE.md` §7.4).

The schema is **normative** (see Appendix B) and covers: `schema_version`,
`documents`, `notes_meta`, `chunks`, `chunks_fts` (FTS5), `vec_chunks`
(`sqlite-vec`), `embedding_meta`, `tags`, `document_tags`, `links`, `entities`,
`relations`, `entity_mentions`, `index_jobs`, `index_meta`.

#### Scenario: Migrations apply on brain open

- **WHEN** a brain is opened for the first time
- **THEN** `migrations/brain/0001_initial.sql` is applied and recorded in the
  per-brain `schema_version`; re-opening is idempotent

#### Scenario: Newer per-brain schema is refused

- **WHEN** a brain's `schema_version` exceeds the kernel's known maximum
- **THEN** the kernel refuses to open it and returns a clear error

#### Scenario: FTS5 and sqlite-vec are available

- **WHEN** the per-brain store is created
- **THEN** `chunks_fts` (FTS5) and `vec_chunks` (`vec0`) exist and are usable,
  and `PRAGMA foreign_keys` is ON with WAL enabled

### Requirement: Vault Indexing and Incremental Reindex

The system SHALL index the vault on open (full scan with hash comparison) and
on change (file watcher), updating documents, chunks, tags, links, mentions,
and the graph incrementally (`BRAIN.md` §5, `SEARCH.md` §13).

#### Scenario: Incremental reindex of one changed file

- **WHEN** a single file changes
- **THEN** only that file is re-chunked, re-embedded, and re-extracted, and
  its backlinks update within 1 s of the save

#### Scenario: Deletion is a soft delete

- **WHEN** a file is deleted from disk
- **THEN** its document is soft-deleted (`deleted_at`), and its chunks/links/
  mentions are removed or marked; the document can be restored

#### Scenario: Persisted, recovering job queue

- **WHEN** indexing is interrupted (crash/shutdown) with jobs pending
- **THEN** `index_jobs` persists them and they resume on the next start

#### Scenario: Size limits

- **WHEN** a file larger than 50 MB (configurable) is encountered
- **THEN** it is skipped; a file over 1M tokens is chunked but not embedded
  (unless opted in)

#### Scenario: Watcher respects the filesystem scope

- **WHEN** the watcher is configured
- **THEN** it watches only the brain vault and ignores the paths in
  "Ignored files", consistent with `SECURITY.md` §7

### Requirement: Markdown Flavor

The system SHALL parse the Fergents markdown flavor (`BRAIN.md` §4.2):
CommonMark + GFM tables/task lists, wiki links `[[note]]` / `[[note|text]]` /
`[[note#heading]]`, embeds `![[...]]`, block refs `^id` and block links
`[[note#^id]]`, tags `#tag` / `#nested/tag`, callouts `> [!note]`, math
`$inline$` / `$$block$$` (KaTeX), Mermaid fenced blocks, and YAML frontmatter.

#### Scenario: Links and tags are extracted

- **WHEN** a note is parsed
- **THEN** outgoing wiki/md links, embeds, tags, block refs, and frontmatter
  are extracted into the `links`, `document_tags`, and `notes_meta` tables

#### Scenario: Unknown frontmatter is preserved

- **WHEN** a note has frontmatter keys beyond the known set
- **THEN** they are preserved in `documents.frontmatter` (the single source
  of truth) and remain queryable

#### Scenario: Rendering is available to the UI

- **WHEN** a note is opened in the editor
- **THEN** the flavor renders (math via KaTeX, Mermaid, callouts) with wiki
  links resolved on the same brain

### Requirement: Chunking

The system SHALL chunk documents with the content-type-specific strategy and
parameters from `SEARCH.md` §3: markdown (heading → paragraph → sentence,
merge to target), plain text (paragraph → sentence), and extracted attachment
text (PDF/plain). Target `chunk_size` 512, `chunk_overlap` 64,
`min_chunk_size` 100, `max_chunk_size` 1024, configurable per brain.

#### Scenario: Heading-aware chunking

- **WHEN** a markdown note is chunked
- **THEN** every chunk stores `heading_path`, `token_count`, and
  `start_offset`/`end_offset`

#### Scenario: Adjacent chunks overlap

- **WHEN** consecutive chunks are produced
- **THEN** they share `chunk_overlap` tokens so context is not lost at
  boundaries

> Code chunking (tree-sitter AST) is **deferred to Phase 3** (decision D8).

### Requirement: Embeddings behind a `VectorStore` Trait

The system SHALL generate embeddings with `BAAI/bge-m3` (1024-dim, dense +
sparse) via `fastembed-rs`, store dense vectors in `vec_chunks`
(`sqlite-vec`) and sparse vectors in `embedding_meta`, keyed by content hash,
batched (default 64), in a background worker, behind a `VectorStore` trait so
the engine can be swapped (e.g. LanceDB) without touching business logic
(`STACK.md` §4.2, ADR-013).

#### Scenario: Identical text is never re-embedded

- **WHEN** the same chunk content is indexed again
- **THEN** its embedding is reused from cache (content-hash keyed)

#### Scenario: Model change requires re-index

- **WHEN** the configured embedding model changes
- **THEN** the system warns and offers a background full re-index

#### Scenario: The vector store is swappable

- **WHEN** the code inserts/queries vectors
- **THEN** it goes through the `VectorStore` trait, never through SQL
  specific to `sqlite-vec`

#### Scenario: CI never downloads the model

- **WHEN** tests run
- **THEN** a deterministic **stub** embedder is used behind the same trait;
  the real `bge-m3` model is loaded lazily only when explicitly enabled
  (decision D5)

### Requirement: Hybrid Search (BM25 + Vector + Sparse, RRF, Rerank)

The system SHALL implement the `SEARCH.md` §2 pipeline: query analysis → BM25
(FTS5) + vector (`sqlite-vec`) + sparse (BGE-M3) + filters → RRF fusion
(k=60) → rerank with a cross-encoder top-20 → top-8, returning results with
per-stage scores and a citation string.

#### Scenario: Hybrid beats BM25 alone

- **WHEN** evaluated on the benchmark set of 50 queries (Appendix D)
- **THEN** hybrid search returns better results than BM25 alone

#### Scenario: Result shape

- **WHEN** a query returns
- **THEN** each result carries `chunk_id`, `document_id`, `path`, `title`,
  `heading_path`, `snippet`, `score`, per-stage `scores`, and `metadata`
  (`SEARCH.md` §11.1), plus a citation

#### Scenario: Rerank is skippable

- **WHEN** the query is trivial (< 3 tokens) or rerank is disabled for the
  brain
- **THEN** the fused order is returned without reranking

### Requirement: Search Filters

The system SHALL support filters as query syntax and as structured params
(`SEARCH.md` §10): `tag`/`-tag`, `path`/`-path` (prefix), `type`, `language`,
`after`/`before`, `has` (link/backlink/attachment), `frontmatter.key`, and
`brain`. Filters SHALL be applied before fusion (indexed fields) or after
(computed fields); filtered-out chunks never reach fusion.

#### Scenario: Tag + date filter

- **WHEN** a query carries `tag:research after:2025-01-01`
- **THEN** only chunks from documents matching both are returned

#### Scenario: Cross-brain filter

- **WHEN** `brain` is specified (alone or with other brains)
- **THEN** only the requested brains are searched and each result shows its
  source brain

### Requirement: Backlinks, Outline, Unlinked Mentions

The system SHALL provide a backlinks panel (incoming links with context +
unresolved links), an outline panel (headings of the current note), and
**unlinked mentions** (occurrences of the note title as plain text), plus
orphan/dead-end detection (`GRAPH.md` §7).

#### Scenario: Backlinks resolve from the `links` table

- **WHEN** a note is opened
- **THEN** all incoming links (with anchor + display text) and unresolved
  targets are returned from `links`

#### Scenario: Unlinked mentions are suggested

- **WHEN** the note title appears as plain text in another note
- **THEN** it is surfaced as a candidate link

#### Scenario: Outlines follow heading structure

- **WHEN** the outline is requested for a note
- **THEN** its heading tree is returned

### Requirement: Daily Notes and Templates

The system SHALL support daily notes (auto-created on first open of the day at
a configurable path, default `daily/YYYY-MM-DD.md`) and templates
(`vault/templates/`, variables `{{date}}`, `{{time}}`, `{{datetime}}`,
`{{title}}`, `{{selection}}`, `{{clipboard}}`, `{{cursor}}`).

#### Scenario: Daily note is created once per day

- **WHEN** the daily note is opened and absent
- **THEN** it is created from the configured daily template and indexed

#### Scenario: Template rendering

- **WHEN** a template is inserted
- **THEN** its variables are substituted and `{{cursor}}` positions the caret

### Requirement: Graph Store (CozoDB) and Entity Extraction

The system SHALL maintain two graphs (`GRAPH.md` §2): the **backlink graph**
(documents + links, in SQLite `links`) and the **entity graph** (entities +
relations, in `graph.db` via CozoDB), linked through entity mentions. The
entity graph SHALL be accessed through a `GraphStore` trait and mirrored in
SQLite (`entities`, `relations`, `entity_mentions`) for fast lookups
(`GRAPH.md` §5).

Entity extraction SHALL be hybrid: **regex** (URLs, emails, dates, code
identifiers, file paths, markdown links) always on; **NER** for
person/org/place behind an `EntityExtractor` trait (regex-only degradation);
**LLM** extraction **off by default** (`BRAIN.md` §6.1, `STACK.md` §6.2,
decision D4).

#### Scenario: Structural relations are extracted

- **WHEN** a document is indexed
- **THEN** `links_to`, `mentions`, `tagged_with`, `in_folder` relations are
  recorded with confidence 1.0

#### Scenario: LLM extraction is opt-in

- **WHEN** `graph.llm_extraction` is false (the default)
- **THEN** no LLM calls are made; only regex + NER entities/relations are
  produced

#### Scenario: NER degrades gracefully

- **WHEN** no NER model is available
- **THEN** extraction proceeds with regex only and never fails

#### Scenario: Graph queries

- **WHEN** a `graph.query` (Datalog) or `graph.algo` (pagerank, shortest path,
  communities) command is sent
- **THEN** the result is returned from `graph.db`, with the SQLite mirror used
  for simple lookups

### Requirement: Graph Visualization

The desktop SHALL provide a graph view rendering the backlink and entity
graphs with force-directed layout, pan/zoom/drag, node focus (highlight
neighbors), open-note on double-click, filters (type/tag/date/confidence/
path/brain), in-graph search, and layout presets (`GRAPH.md` §8).

Phase 1 SHALL use **React Flow** (decision D2). A custom WebGL renderer is
deferred and adopted only if the graph exceeds ~5k nodes and React Flow
degrades below target.

#### Scenario: 1000 nodes at 60 fps

- **WHEN** a graph of 1000 nodes is loaded
- **THEN** it renders and interacts at 60 fps and loads in under 1 s

#### Scenario: Views

- **WHEN** the user switches views
- **THEN** local (1–2 hops), global, entity-only, backlink-only, and filtered
  views are available

### Requirement: Multi-Brain Operations

The system SHALL support (`BRAIN.md` §8, `ARCHITECTURE.md` §8): create from
template (`blank`, `personal`, `research`, `coding`, `team`); attach/detach a
brain to a project with a role (`primary`/`read-only`/`scratch`);
duplicate (new ID, independent copy); move (update registry, keep ID);
export/import `.fergbrain` (zip with `manifest.json` + checksums); and
save-as-template. Cross-brain search and cross-brain links (`[[brain:...]]`,
`[[id:...]]`) SHALL be supported.

#### Scenario: Duplicating a brain produces an independent copy

- **WHEN** a brain is duplicated
- **THEN** the copy has a new `brain.id`, its own index, and changes to one do
  not affect the other

#### Scenario: `.fergbrain` round-trip

- **WHEN** a brain is exported and then imported to a new path
- **THEN** the archive contains `brain.toml` + `vault/` + `manifest.json`
  (regenerable files excluded by default), validates, registers, and triggers
  indexing

#### Scenario: Cross-brain link resolution

- **WHEN** a note links to `[[brain:other/notes/foo]]`
- **THEN** the link resolves if the target brain is attached, otherwise it is
  shown as "unresolved"

#### Scenario: Roles are enforced

- **WHEN** a brain is attached as `read-only`
- **THEN** write operations targeting it are rejected

### Requirement: Brain Lifecycle and Registry

The system SHALL implement the brain lifecycle (`BRAIN.md` §9): created →
indexing → ready → archived → deleted, wired into the global `brains` /
`projects` / `project_brains` tables (Phase 0 schema) and emitting the
corresponding events.

#### Scenario: Create

- **WHEN** a brain is created
- **THEN** the folder + `brain.toml` are written, it is registered in the
  global DB, the first index runs, and `brain.created` is emitted

#### Scenario: Open and close

- **WHEN** a brain is opened
- **THEN** `brain.toml` is read, `index.sqlite` opened + migrated, the watcher
  started, an incremental index run, and `brain.opened` emitted; on close,
  pending work is flushed, the watcher stopped, and `brain.closed` emitted

#### Scenario: Archive / delete

- **WHEN** a brain is archived or deleted
- **THEN** archiving only removes it from the active list (files kept);
  deletion requires typed confirmation and offers export first, then moves
  files to trash per config

### Requirement: Phase 1 Protocol Surface

The system SHALL extend the JSON-RPC protocol with the Phase 1 commands and
events (Appendix A), generated into Rust and TypeScript via the existing
codegen pipeline, with a freshness test.

#### Scenario: Generated bindings stay in sync

- **WHEN** the Phase 1 schema is added and codegen runs
- **THEN** Rust and TypeScript types reflect it and the freshness tests pass

#### Scenario: Indexing is observable

- **WHEN** indexing runs
- **THEN** `index.queued` / `index.started` / `index.progress` /
  `index.completed` (or `index.failed`) events are emitted on the bus

### Requirement: Desktop UI (Brain)

The desktop SHALL provide (`UI.md`, `ROADMAP.md` §5): a **brain sidebar**
(file tree, tag list, backlinks panel, outline, search box), an **editor area**
with tabs and live preview, a **graph view**, a **search panel** (keyboard
navigation), and a per-brain **settings page**. All elements SHALL use only
`style/` tokens and components (include order `tokens.css` → `typography.css`
→ `components.css`); any new component SHALL be added to `style/` following
`patterns.html`; any new token SHALL be added to `tokens.json` + `tokens.css`
first.

#### Scenario: Explore and edit a note

- **WHEN** the user opens a brain
- **THEN** the sidebar shows files/tags, the editor opens notes in tabs with
  live preview, and backlinks/outline update within 1 s of an edit

#### Scenario: Search UI

- **WHEN** the user opens search and types
- **THEN** results render with snippets and highlights, navigable by keyboard

#### Scenario: Design-system compliance

- **WHEN** any brain UI element renders
- **THEN** it uses only `style/` tokens/components with no hardcoded colors,
  spacing, radii, or motion (dark-first, glass, accessible)

### Requirement: Offline and Performance

All Phase 1 operations SHALL work fully offline. The system SHALL meet the
`BRAIN.md` §16 / `SEARCH.md` §16 / `GRAPH.md` §12 targets.

#### Scenario: Offline

- **WHEN** the machine has no network
- **THEN** create/open/index/search/graph/multi-brain operations all work

#### Scenario: Performance targets

- **WHEN** benchmarked
- **THEN** open brain (10k notes) < 2 s; incremental reindex (1 file) < 200 ms;
  full reindex (10k notes) < 10 min; hybrid search top-10 < 100 ms; graph load
  (1k nodes) < 1 s

### Requirement: Testing and CI

Every task SHALL ship tests and keep CI green (Phase 0 gates unchanged:
`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build`,
`eslint`, `prettier --check`, `tsc --noEmit`, pnpm build, Windows/macOS/Linux).

#### Scenario: Kernel tests

- **WHEN** the Rust test suite runs
- **THEN** it covers `brain.toml` round-trip, per-brain migrations,
  chunking, links/tags extraction, the search pipeline (with a stub embedder),
  RRF fusion, backlinks, `.fergbrain` round-trip, and multi-brain ops — all
  without downloading the 2.3 GB model

#### Scenario: UI tests

- **WHEN** the vitest suite runs
- **THEN** it covers the brain sidebar, editor tabs, search panel, and graph
  view rendering with mock data

#### Scenario: End-to-end

- **WHEN** the integration test runs
- **THEN** it creates a brain, adds notes, indexes, searches, and reads
  backlinks against a temp vault

## MODIFIED Requirements

### Requirement: Brains Registry (Phase 0 §Registry)

The Phase 0 registry shipped an **empty** `brains` table and `brain.list`
returning `[]`. Phase 1 SHALL replace this with real persistence: brain
create/open/close register and update rows in `brains`, and `brain.list`
returns the registered brains. `project_brains` and `projects` SHALL be used
for attach/detach since their schema already exists.

#### Scenario: `brain.list` returns real brains

- **WHEN** a client sends `brain.list` after creating brains
- **THEN** the kernel returns the registered brains (id, name, path, role)

#### Scenario: Backward compatibility

- **WHEN** no brains exist (fresh install)
- **THEN** `brain.list` still returns an empty array with no error

### Requirement: Storage (Phase 0 §Storage Layer)

Phase 0 `SqliteStorage` opened the global store. Phase 1 SHALL reuse the same
`Storage` trait + `SqliteStorage` + migration runner for **per-brain**
databases, parameterized by a migrations directory (`migrations/brain/`) and a
database path (`<brain>/index.sqlite`). No behavior of the global store
changes.

#### Scenario: Same trait, different migrations

- **WHEN** a brain is opened
- **THEN** `SqliteStorage::open(<brain>/index.sqlite, migrations/brain/)` is
  used, applying the per-brain schema and refusing newer versions

## REMOVED Requirements

None.

## Dependencies

New dependencies introduced in Phase 1, each justified. No others shall be
added without justification.

| Crate / package | Purpose | Justification |
|---|---|---|
| `fastembed` | Local embeddings (BGE-M3) and cross-encoder rerank via ONNX | Required by `STACK.md` §5.1; local-first, no API |
| `ort` (ONNX Runtime) | Direct ONNX loading for NER and reranker fallbacks | Needed when `fastembed` lacks the exact model (D3, D4) |
| `notify` | Filesystem watcher | Required by `BRAIN.md` §5.3 / `SEARCH.md` §13.1 |
| `cozo` | Embedded graph database (Datalog) | Required by `STACK.md` §6.1 / ADR-013 |
| `pulldown-cmark` or `comrak` | Markdown parsing (CommonMark + GFM) | Required by `BRAIN.md` §4.2; `comrak` covers GFM + tables + task lists |
| `serde_yaml` / `toml` | Frontmatter and `brain.toml` parsing | Already used in Phase 0 for TOML; YAML added for frontmatter |
| `regex` | Entity extraction (URLs, emails, etc.) | Required by `GRAPH.md` §3.2 |
| `zip` | `.fergbrain` archive | Required by `BRAIN.md` §14 |
| `sha2` / `blake3` | Content hashing for embeddings cache + archive checksums | Already used in Phase 0 audit chain; extended to content |
| `ulid` | Brain, document, chunk, entity IDs | Consistent with Phase 0 event ids |
| `walkdir` | Vault traversal | Standard, lightweight |
| `ignore` | `.brainignore` support (gitignore syntax) | Required by `BRAIN.md` §3.4 |
| `pulldown-cmark-to-cmark` (optional) | Markdown round-trip for edits | Only if needed by the editor; otherwise omitted |
| `image` + `pdfium-render` or `lopdf` | Attachment thumbnails + PDF text extraction | Required by `BRAIN.md` §12; `pdfium-render` preferred for quality |
| `codemirror` (npm) + `@codemirror/*` | Markdown editor | Decision D1 |
| `reactflow` (npm) | Graph view | Decision D2 |
| `d3-force` (npm) | Force layout for React Flow | Standard layout engine |
| `katex` (npm) | Math rendering | Required by `BRAIN.md` §4.2 |
| `mermaid` (npm) | Diagram rendering | Required by `BRAIN.md` §4.2 |
| `@tanstack/react-query` (npm) | Data fetching against the kernel protocol | Already in Phase 0 desktop, reused |

No dependency shall be added for: code chunking (Phase 3), LLM providers
(Phase 2), sync (Phase 7), plugin runtime (Phase 4).

## Appendix A: Phase 1 protocol surface

**Command methods (Phase 1)**:

- Brain: `brain.create`, `brain.open`, `brain.close`, `brain.get`,
  `brain.update`, `brain.duplicate`, `brain.move`, `brain.archive`,
  `brain.unarchive`, `brain.delete`, `brain.export`, `brain.import`,
  `brain.attach`, `brain.detach`, `brain.reindex`, `brain.template.save`.
  (`brain.list` already exists and is extended.)
- Documents: `doc.list`, `doc.get`, `doc.create`, `doc.write`, `doc.rename`,
  `doc.move`, `doc.delete`, `doc.restore`, `doc.backlinks`, `doc.outline`,
  `doc.unlinked_mentions`.
- Links/Tags: `links.broken`, `tag.list`, `tag.rename`, `tag.delete`.
- Daily/Templates: `daily.open`, `template.list`, `template.create`,
  `template.render`.
- Search: `search.query`.
- Graph: `graph.query`, `graph.algo`, `graph.neighbors`, `graph.subgraph`,
  `graph.entities.merge`.
- Index: `index.status`.

**Event kinds (Phase 1)**:

```
brain.created / brain.opened / brain.closed / brain.updated
brain.archived / brain.deleted / brain.exported / brain.imported / brain.indexed
file.created / file.changed / file.deleted / file.renamed
note.created / note.updated / note.linked
index.queued / index.started / index.progress / index.completed / index.failed
embedding.created / embedding.failed
search.executed / graph.updated / tag.updated / link.updated
```

## Appendix B: Per-brain schema (normative)

`migrations/brain/0001_initial.sql`:

- **`schema_version`** — `version INTEGER PRIMARY KEY`, `description TEXT`,
  `checksum TEXT`, `applied_at INTEGER`.

- **`documents`** — `id TEXT PRIMARY KEY` (ULID), `path TEXT UNIQUE`
  (vault-relative), `kind TEXT` (`note|attachment|code|other`), `title TEXT`,
  `mime TEXT`, `size INTEGER`, `hash TEXT`, `frontmatter TEXT NULL` (JSON —
  **the single source of truth for frontmatter**), `word_count INTEGER`,
  `reading_time_s INTEGER`, `created_at INTEGER`, `updated_at INTEGER`,
  `indexed_at INTEGER`, `deleted_at INTEGER NULL`; indexes on `(path)`,
  `(kind)`, `(deleted_at)`.

- **`notes_meta`** — `document_id TEXT PRIMARY KEY REFERENCES documents` —
  contains only **derived** fields extracted from `documents.frontmatter` for
  fast querying; no duplication of the raw frontmatter:
  `aliases TEXT` (JSON), `cssclasses TEXT` (JSON), `publish INTEGER`.

- **`chunks`** — `id TEXT PRIMARY KEY`, `document_id TEXT REFERENCES
  documents`, `ordinal INTEGER`, `content TEXT`, `token_count INTEGER`,
  `start_offset INTEGER`, `end_offset INTEGER`, `heading_path TEXT NULL`,
  `language TEXT NULL`, `symbol TEXT NULL`; index on `(document_id, ordinal)`.

- **`chunks_fts`** — FTS5 virtual table over `content` (+ `heading_path`),
  `tokenize = 'unicode61 remove_diacritics 2'` by default (decision D11),
  configurable per brain via `brain.toml [index] fts_tokenizer`. No `porter`
  by default to remain multilingual-friendly.

- **`vec_chunks`** — `sqlite-vec` virtual table
  `USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[1024])`.

- **`embedding_meta`** — `chunk_id TEXT PRIMARY KEY REFERENCES chunks`,
  `model TEXT`, `dim INTEGER`, `content_hash TEXT`, `sparse TEXT` (JSON NULL),
  `created_at INTEGER`; index on `(content_hash)`.

- **`tags`** — `id TEXT PRIMARY KEY` (ULID, consistent with the rest of the
  schema), `name TEXT UNIQUE`, `parent TEXT NULL`.

- **`document_tags`** — `document_id REFERENCES documents`,
  `tag_id REFERENCES tags`, primary key `(document_id, tag_id)`.

- **`links`** — `id TEXT PRIMARY KEY` (ULID), `src_document_id TEXT
  REFERENCES documents`, `dst_document_id TEXT NULL`, `dst_path TEXT NULL`,
  `dst_heading TEXT NULL`, `dst_block TEXT NULL`, `display_text TEXT NULL`,
  `anchor TEXT NULL`, `kind TEXT` (`wiki|md|embed|cross_brain|by_id`),
  `unresolved INTEGER`; indexes on `(src_document_id)`, `(dst_document_id)`,
  `(dst_path)`.

- **`entities`** — `id TEXT PRIMARY KEY`, `name TEXT`, `type TEXT`,
  `canonical_name TEXT NULL`, `normalized_name TEXT`, `metadata TEXT` (JSON
  NULL), `first_seen_at INTEGER`, `last_seen_at INTEGER`; index on
  `(normalized_name, type)`.

- **`relations`** — `id TEXT PRIMARY KEY`, `src TEXT`, `dst TEXT`,
  `kind TEXT`, `weight REAL`, `confidence REAL`, `metadata TEXT` (JSON NULL),
  `source_doc TEXT NULL`, `created_at INTEGER`; indexes on `(src, kind)`,
  `(dst, kind)`.

- **`entity_mentions`** — `id TEXT PRIMARY KEY` (ULID; surrogate key to avoid
  NULL in composite primary key), `entity_id TEXT REFERENCES entities`,
  `document_id TEXT REFERENCES documents`, `chunk_id TEXT NULL`,
  `start_offset INTEGER NOT NULL DEFAULT 0`, `end_offset INTEGER NOT NULL
  DEFAULT 0`, `confidence REAL`; unique index on
  `(entity_id, document_id, start_offset)`.

- **`index_jobs`** — `id TEXT PRIMARY KEY` (ULID), `path TEXT`, `op TEXT`
  (`upsert|delete|reindex`), `priority INTEGER`, `enqueued_at INTEGER`,
  `attempts INTEGER`; index on `(priority, enqueued_at)`.

- **`index_meta`** — `key TEXT PRIMARY KEY`, `value TEXT`.

## Appendix C: Deferred to later phases

- Code chunking (tree-sitter AST), symbol/regex/structural code search —
  **Phase 3 (IDE)** (decision D8).
- LLM-based entity/relation extraction producing `cites`/`authored_by`/etc. —
  **Phase 2** (LLM router); Phase 1 ships it **off** and regex/NER only.
- Vision captions for images, audio transcription, video key frames —
  **Phase 2/3** (multimodal pipeline). Phase 1 indexes attachments (images,
  PDFs) by path/MIME + thumbnails + PDF text extraction.
- Shadow git reversibility and `.fergents` backups (write path) — **Phase 2**.
- Sync of the brain (`[sync]`) — **Phase 7**.
- Delta/BDI graph algorithms beyond pagerank/shortest-path/communities —
  later.
- Custom WebGL graph renderer — deferred unless React Flow degrades below
  target (decision D2).

## Appendix D: Search benchmark set

Phase 1 acceptance requires that hybrid search beats BM25 alone on a
benchmark of 50 queries. That benchmark set SHALL be produced as an explicit
deliverable in Phase 1:

- A JSON file `bench/search/queries.json` committed to the repo, containing
  50 entries of `{ query, expected_document_paths (1-5), mode_default }`.
- At least 3 content domains (e.g. technical notes, meeting notes,
  research clippings) to avoid overfitting.
- A runner (`cargo test -p index --features bench` or a small binary) that
  computes Recall@8 and MRR for both BM25-only and hybrid pipelines and prints
  a comparison. The test asserts `hybrid_recall@8 > bm25_recall@8` by a
  minimum margin (default: 5 percentage points).
- The benchmark runs against a small synthetic vault committed under
  `bench/search/vault/` (a few hundred notes, no copyrighted content).

The benchmark set is a deliverable of the first task that touches search
implementation; it is not part of the initial repo bootstrap.

## Appendix E: Key decisions (recorded as ADRs)

- **ADR-016:** Dedicated markdown editor (CodeMirror 6) for Phase 1;
  Monaco/LSP deferred to Phase 3.
- **ADR-017:** React Flow for graph visualization in Phase 1; custom WebGL
  deferred and adopted only if React Flow degrades below target.
- **ADR-018:** `VectorStore`, `Reranker`, `EntityExtractor` traits keep the
  embedding/rerank/NER engines swappable and keep heavy models out of CI
  (deterministic stubs in tests).
- **ADR-019:** Per-brain FTS5 tokenizer defaults to `unicode61
  remove_diacritics 2` (no Porter), configurable per brain.
- **ADR-020:** Phase 0 gaps fixed before Phase 1 implementation begins (see
  Appendix F).

## Appendix F: Phase 0 gaps (prerequisite)

Before any Phase 1 implementation task, the following four Phase 0 gaps
identified by the audit SHALL be closed as four small PRs, each with tests,
CI green:

1. **7-day log retention** — implement rotation pruning in `fergentsd`
   logging (SubTask 7.4 / Phase 0 spec "Structured logs").
2. **True daemon detachment** — `setsid` on Unix, `DETACHED_PROCESS` /
   `CREATE_NEW_PROCESS_GROUP` on Windows (SubTask 7.1 / Phase 0 spec "Start
   as a background daemon").
3. **`fergentsd stop`** — send SIGTERM (Unix) / console-close (Windows),
   wait up to 10 s, report the result (SubTask 7.2 / Phase 0 spec "Stop
   sends a termination signal").
4. **`fergentsd status`** — check process existence (PID), not only socket
   reachability (SubTask 7.2 / Phase 0 spec "Status reads the PID file").

These are tracked in `tasks.md` as Tasks 0.1–0.4 and must be merged before
Task 1 of Phase 1 starts.