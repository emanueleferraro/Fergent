# Tasks

Scope: **ROADMAP.md Phase 1 (Brain) only**, per `.trae/specs/brain-phase-1/spec.md`.
Ordered, small, independently verifiable. Every task ships with tests and keeps
CI green (Windows/macOS/Linux: `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test`, `cargo build`, `eslint`, `prettier --check`, `tsc --noEmit`,
pnpm build). One deliverable per task; small PRs; Conventional Commits.

> Nothing in Tasks 1–24 starts until all of **Task 0** is merged (spec
> decision D12 / Appendix F).

- [ ] Task 0: Close the four Phase 0 gaps (prerequisite).
  - [ ] SubTask 0.1: Implement 7-day log retention — add a `[log]` section to the global `config.toml` with `retention_days = 7` and `max_files = 30`; prune rotated log files in `~/.fergents/logs/` older than `retention_days` on startup and once per day (spec Appendix F §1).
  - [ ] SubTask 0.2: Implement true daemon detachment — `setsid` (Unix) / `DETACHED_PROCESS` + `CREATE_NEW_PROCESS_GROUP` (Windows) so `fergentsd start` survives the parent shell (spec Appendix F §2).
  - [ ] SubTask 0.3: Implement `fergentsd stop` — send SIGTERM (Unix) / console-close (Windows), wait up to 10 s, report the outcome and non-zero exit on failure (spec Appendix F §3).
  - [ ] SubTask 0.4: Implement `fergentsd status` process check — verify PID existence (not only socket reachability) and remove stale PID files (spec Appendix F §4).
  - [ ] SubTask 0.5: Test: retention pruning removes old logs; detached child survives parent exit; `stop` returns after the daemon exits; `status` reports a killed-PID daemon as not running. CI green.

- [ ] Task 1: Scaffold the four new crates and workspace wiring.
  - [ ] SubTask 1.1: Add `crates/markdown`, `crates/brain`, `crates/index`, `crates/graph` to the workspace (empty libs, crate docs, lints inherit workspace).
  - [ ] SubTask 1.2: Add **only the dependencies required to compile empty crates** (no engine, no UI). All other dependencies are added in the task that first uses them: Task 5 → `comrak`/`serde_yaml`; Task 7 → `fastembed`/`ort`; Task 8 → `notify`/`walkdir`/`ignore`; Task 11 → reranker backend; Task 12 → `cozo`; Task 14 → `zip`; Task 18 → `codemirror`; Task 20 → `reactflow`+`d3-force`. Document every addition in the PR body with a one-line justification.
  - [ ] SubTask 1.3: Extend `ci.yml` minimally so the four new empty crates build/test/lint on all three OSes.
  - [ ] SubTask 1.4: Test: workspace builds; `cargo test` runs the empty crates; `cargo build --release` succeeds.

- [ ] Task 2: Phase 1 protocol surface.
  - [ ] SubTask 2.1: Author JSON Schema for the Phase 1 commands (Appendix A: `brain.*`, `doc.*`, `links.broken`, `links.orphans`, `links.dead_ends`, `tag.*`, `daily.*`, `template.*`, `search.query` with a `mode` field, `graph.*`, `index.status`) and events (Appendix A event kinds) in `protocol/`.
  - [ ] SubTask 2.2: Regenerate Rust (`typify`) and TypeScript (`json-schema-to-typescript`) bindings; commit the output.
  - [ ] SubTask 2.3: Test: codegen freshness tests (Rust + TS) + serde round-trip tests for the new request/response/event types.

- [ ] Task 3: `brain.toml` model and vault layout (`crates/brain`).
  - [ ] SubTask 3.1: Define the config schema for `[brain]`, `[vault]`, `[markdown]`, `[index]`, `[graph]`, `[search]`, `[sync]`, `[git]` with the keys in `BRAIN.md` §6.1 and defaults. Note: `[sync]` and `[git]` are **parsed and preserved only** in Phase 1; they have no runtime effect and are shown read-only in the settings page.
  - [ ] SubTask 3.2: Implement load/save/validate with a clear error on missing/invalid `brain.toml` (never a panic).
  - [ ] SubTask 3.3: Implement the vault layout (`vault/`, `templates/`, `daily/`, `attachments/`, `.fergents/`) and `.brainignore` (gitignore syntax).
  - [ ] SubTask 3.4: Test: round-trip, defaults, invalid input, and ignored-file matching.

- [ ] Task 4: Per-brain index schema and migration wiring.
  - [ ] SubTask 4.1: Write `migrations/brain/0001_initial.sql` exactly per spec Appendix B (`documents`, `notes_meta`, `chunks`, `chunks_fts`, `vec_chunks`, `embedding_meta`, `tags`, `document_tags`, `links`, `entities`, `relations`, `entity_mentions`, `index_jobs`, `index_meta`, `schema_version`).
  - [ ] SubTask 4.2: Open a per-brain store via the existing `SqliteStorage` + migration runner parameterized by `migrations/brain/` and `<brain>/index.sqlite`.
  - [ ] SubTask 4.3: Refuse to open a brain whose `schema_version` is newer than the kernel's maximum.
  - [ ] SubTask 4.4: Test: apply + idempotency, newer-schema refusal, FTS5 (`chunks_fts`) and `sqlite-vec` (`vec_chunks`) usable, WAL + `foreign_keys` ON.

- [ ] Task 5: Markdown flavor parser (`crates/markdown`).
  - [ ] SubTask 5.1: Split YAML frontmatter from body (`comrak` for CommonMark + GFM tables/task lists; `serde_yaml` for frontmatter).
  - [ ] SubTask 5.2: Extract wiki links `[[note]]`/`[[note|text]]`/`[[note#heading]]`, embeds `![[...]]`, block refs `^id` + block links `[[note#^id]]`, tags `#tag`/`#nested/tag`.
  - [ ] SubTask 5.3: Detect callouts `> [!note]`, math `$inline$`/`$$block$$`, Mermaid fenced blocks.
  - [ ] SubTask 5.4: Test: fixtures covering every construct + frontmatter passthrough (unknown keys preserved).

- [ ] Task 6: Chunking (`crates/index`).
  - [ ] SubTask 6.1: Define the chunk model and implement the markdown heading-aware chunker (heading → paragraph → sentence, merge to target; `chunk_size` 512, `chunk_overlap` 64, `min` 100, `max` 1024, configurable).
  - [ ] SubTask 6.2: Implement plain-text and attachment-text chunkers.
  - [ ] SubTask 6.3: Test: `heading_path`, `token_count`, `start/end_offset`, overlap between adjacent chunks, min/max merge behavior.

- [ ] Task 7: `VectorStore` trait and embeddings.
  - [ ] SubTask 7.1: Define the `VectorStore` trait and implement the `sqlite-vec` backend (`vec_chunks`); no business code touches `sqlite-vec` SQL directly.
  - [ ] SubTask 7.2: Define the `Embedder` trait; implement the BGE-M3 provider (`fastembed`, `ort`) plus a deterministic **stub** used in tests (feature-gated `embed-stub` so CI never downloads the 2.3 GB model).
  - [ ] SubTask 7.3: Implement content-hash embedding cache (`embedding_meta`) and a batched (default 64) background embedding worker.
  - [ ] SubTask 7.4: Test: identical text is never re-embedded; batch processing; model-change triggers a re-index warning; store swappable behind the trait.

- [ ] Task 8: Indexing pipeline, file watcher, and persisted job queue.
  - [ ] SubTask 8.1: Full vault scan on open with hash comparison; upsert `documents` (kind, mime, size, hash, frontmatter, counts).
  - [ ] SubTask 8.2: Implement the persisted `index_jobs` queue with resume-on-start recovery.
  - [ ] SubTask 8.3: Implement the `notify` watcher scoped to the vault, honoring `.brainignore`/ignored paths (consistent with `SECURITY.md` §7). Use `walkdir` + `ignore` for traversal.
  - [ ] SubTask 8.4: Incremental reindex of a changed file (re-chunk, re-embed, re-extract); soft delete on removal with restore; 50 MB / 1M-token limits.
  - [ ] SubTask 8.5: Emit `index.queued|started|progress|completed|failed`, `file.*`, `note.*`.
  - [ ] SubTask 8.6: Test: incremental reindex of one file < 200 ms; delete + restore; interrupted queue resumes; oversized file skipped.

- [ ] Task 9: Links, tags, backlinks, outline, unlinked mentions.
  - [ ] SubTask 9.1: Persist `links`, `document_tags`, and `notes_meta` from the parsed notes (all link kinds incl. `cross_brain`).
  - [ ] SubTask 9.2: Implement `doc.backlinks` (incoming links with anchor + display text, incl. unresolved) and `links.broken`.
  - [ ] SubTask 9.3: Implement `doc.outline` (heading tree of a note).
  - [ ] SubTask 9.4: Implement `doc.unlinked_mentions` (title as plain text); implement `links.orphans` (no incoming links) and `links.dead_ends` (no outgoing links).
  - [ ] SubTask 9.5: Implement `tag.list`, `tag.rename`, `tag.delete`.
  - [ ] SubTask 9.6: Test: backlinks resolve; unresolved reported; unlinked mention suggested; outline tree correct; orphan/dead-end detection correct; tag rename updates `document_tags`.

- [ ] Task 10: FTS5 BM25, query analysis, and filters.
  - [ ] SubTask 10.1: Maintain `chunks_fts` (external-content linked to `chunks`) and implement the BM25 query.
  - [ ] SubTask 10.2: Implement query analysis (terms, phrases, filters, negation).
  - [ ] SubTask 10.3: Implement structured filters (`tag`/`-tag`, `path`/`-path`, `type`, `language`, `after`/`before`, `has`, `frontmatter.key`, `brain`) applied before fusion.
  - [ ] SubTask 10.4: Test: BM25 ranking, phrase/negation parsing, each filter, cross-brain filter.

- [ ] Task 11: Hybrid search (fusion + rerank) and the benchmark.
  - [ ] SubTask 11.1: Define the `Reranker` trait; implement a cross-encoder provider (`fastembed`/`ort`) plus a deterministic stub for tests (feature-gated `rerank-stub`).
  - [ ] SubTask 11.2: Implement RRF fusion (k=60) across BM25 + vector + sparse.
  - [ ] SubTask 11.3: Implement the pipeline: analysis → BM25+vector+sparse+filters → fusion → rerank top-20 → top-8, with the result shape from `SEARCH.md` §11.1 (per-stage scores + citation) and the trivial-query rerank skip.
  - [ ] SubTask 11.4: Implement the `search.query` command (with `mode: bm25|vector|sparse|hybrid`, default `hybrid`) and the `search.executed` event.
  - [ ] SubTask 11.5: Add the benchmark set `bench/search/queries.json` (50 queries, ≥3 domains) + synthetic vault `bench/search/vault/`, and a runner asserting `hybrid_recall@8 > bm25_recall@8` by ≥5 pp (spec Appendix D). The runner is behind a `bench` feature and `#[ignore]` by default; it runs in the nightly benchmark job, not on every PR.
  - [ ] SubTask 11.6: Test: fusion order, result shape, rerank skip, benchmark assertion (stub embedder/reranker).

- [ ] Task 12: Graph store (CozoDB) and entity extraction (`crates/graph`).
  - [ ] SubTask 12.1: Define the `GraphStore` trait.
  - [ ] SubTask 12.2: Implement the CozoDB backend (`graph.db`) with the Datalog schema (`entity`, `relation`, `mention`).
  - [ ] SubTask 12.3: Mirror entities/relations/mentions into SQLite (`entities`, `relations`, `entity_mentions`) and keep them in sync on reindex.
  - [ ] SubTask 12.4: Define the `EntityExtractor` trait; implement the regex extractor (URLs, emails, dates, code identifiers, file paths, markdown links).
  - [ ] SubTask 12.5: Implement the NER extractor (person/org/place) via `ort` when a model is available; degrade to regex-only without failing.
  - [ ] SubTask 12.6: Infer structural relations (`links_to`, `mentions`, `tagged_with`, `in_folder`) with confidence 1.0; keep LLM extraction **off by default**.
  - [ ] SubTask 12.7: Implement `graph.query`, `graph.algo` (pagerank, shortest path, communities), `graph.neighbors`, `graph.subgraph`, `graph.entities.merge`; emit `graph.updated`.
  - [ ] SubTask 12.8: Test: schema create; mirror sync; regex extraction; NER degradation; each query/algo command.

- [ ] Task 13: Daily notes and templates.
  - [ ] SubTask 13.1: Implement `daily.open` — create once per day from the configured daily template at `daily/YYYY-MM-DD.md` (strftime, validated per platform) and index it.
  - [ ] SubTask 13.2: Implement `template.list`, `template.create`, `template.render` with variables `{{date}}`, `{{time}}`, `{{datetime}}`, `{{title}}`, `{{selection}}`, `{{clipboard}}`, `{{cursor}}`.
  - [ ] SubTask 13.3: Test: daily created once; variables substituted; `{{cursor}}` position returned.

- [ ] Task 14: Multi-brain operations and lifecycle.
  - [ ] SubTask 14.1: Implement `brain.create` from a template (`blank`, `personal`, `research`, `coding`, `team`); write folder + `brain.toml`; register in the global DB; run the first index; emit `brain.created`.
  - [ ] SubTask 14.2: Implement `brain.open`, `brain.close`, `brain.get`, `brain.update`, `brain.archive`, `brain.unarchive`, `brain.delete` (+ events); archive keeps files; delete requires typed confirmation and offers export first.
  - [ ] SubTask 14.3: Implement `brain.duplicate` (new `brain.id`, independent copy) and `brain.move` (registry updated, ID preserved).
  - [ ] SubTask 14.4: Implement `brain.export`/`brain.import` (`.fergbrain` zip via `zip` crate, with `manifest.json` + checksums; regenerable files excluded by default) and validation.
  - [ ] SubTask 14.5: Implement `brain.attach`/`brain.detach` with roles (`primary`/`read-only`/`scratch`) enforced on writes. Note: these are exposed via the protocol and CLI only in Phase 1; no desktop UI yet (projects UI arrives in a later phase).
  - [ ] SubTask 14.6: Implement `brain.template.save` (save a brain as a template under `$FERGENTS_HOME/templates/brains/`).
  - [ ] SubTask 14.7: Implement cross-brain link resolution (`[[brain:...]]`, `[[id:...]]`) and the cross-brain search filter.
  - [ ] SubTask 14.8: Implement `brain.reindex` and `brain.indexed`; make `brain.list` return real registered brains (extend the Phase 0 empty registry).
  - [ ] SubTask 14.9: Test: create/open/close/archive/delete; duplicate independence; move keeps ID; `.fergbrain` round-trip; role enforcement; cross-brain link resolution; `brain.list` returns rows and stays `[]` on a fresh install.

- [ ] Task 15: Kernel wiring and CLI.
  - [ ] SubTask 15.1: Register the brain service in `crates/core` and dispatch the new commands in `crates/api`.
  - [ ] SubTask 15.2: Add CLI commands: `fergents brain list|create|open`, `fergents search`, `fergents status` (unchanged).
  - [ ] SubTask 15.3: Test: command dispatch for the new surface; CLI commands against a running kernel; end-to-end create → note → index → search → backlinks on a temp vault.

- [ ] Task 16: Desktop — design-system extensions (inventory-driven, parallel to Tasks 17–21).
  - [ ] SubTask 16.1: As each of Tasks 17–21 begins, inventory the patterns it needs (`#graph`, `#sidebar`, `#editor`, `.file-tree`, `.tree-item`, `.cmdk-*`, panels) and identify any missing component.
  - [ ] SubTask 16.2: For each missing component, add it to `style/components.css` following `patterns.html`, and any new token to `style/tokens.json` + `style/tokens.css` **before** it is used; update `style/readme.md`.
  - [ ] SubTask 16.3: Test: vitest/visual check that the new components use only tokens (no hardcoded values).
  - [ ] Note: Task 16 is not a hard prerequisite; it runs in parallel with the UI tasks and is completed incrementally as components are needed.

- [ ] Task 17: Desktop — brain sidebar and explorer.
  - [ ] SubTask 17.1: Implement the brain sidebar (file tree, tag list, search box) with the dedicated Brain icon and keyboard navigation (`UI.md` §5.2).
  - [ ] SubTask 17.2: Implement the backlinks panel, outline panel, and unlinked-mentions list.
  - [ ] SubTask 17.3: Test: vitest for tree/tags/panels rendering and navigation with mock data.

- [ ] Task 18: Desktop — editor with live preview.
  - [ ] SubTask 18.1: Integrate CodeMirror 6 (decision D1) as the markdown editor with tabs (`.editor-tabs`) and a live preview pane rendering the flavor (KaTeX math, Mermaid, callouts, resolved wiki links).
  - [ ] SubTask 18.2: Wire edits to `doc.write` and reflect watcher updates within 1 s (backlinks/outline refresh).
  - [ ] SubTask 18.3: Test: vitest for tab open/close, editor mount, preview rendering of math/mermaid/callouts.

- [ ] Task 19: Desktop — search panel.
  - [ ] SubTask 19.1: Implement the search panel (`search.query`) with snippets/highlights, filters UI, and full keyboard navigation.
  - [ ] SubTask 19.2: Test: vitest for query submission, result rendering, keyboard navigation, empty state.

- [ ] Task 20: Desktop — graph view.
  - [ ] SubTask 20.1: Implement the graph view with **React Flow** (decision D2): force layout (`d3-force`), pan/zoom/drag, node focus, open-note on double-click, filters (type/tag/date/confidence/path/brain), in-graph search, layout presets, and the local/global/entity-only/backlink-only/filtered views.
  - [ ] SubTask 20.2: In CI, test: graph loads 1000 nodes in < 1 s, does not crash, and the rendered node count matches the mock. The "60 fps interaction" target is verified **manually** (documented with a screen recording in the PR body); ADR-017 migration trigger applies only if the manual check fails.
  - [ ] SubTask 20.3: Test: vitest renders the graph from mock data; layout/view switching; loading smoke on a 1000-node fixture (no frame-rate assertion in CI).

- [ ] Task 21: Desktop — per-brain settings page.
  - [ ] SubTask 21.1: Implement the per-brain settings page (markdown toggles, index settings, graph settings, search settings) writing through `brain.update`. `[sync]` and `[git]` sections are shown **read-only** in Phase 1 (parsed-only; see Task 3.1).
  - [ ] SubTask 21.2: Test: vitest for load/save, validation, and unchanged value handling.

- [ ] Task 22: Performance and offline verification.
  - [ ] SubTask 22.1: Add benchmarks behind a `bench` feature, `#[ignore]` by default: open brain (10k notes) < 2 s; incremental reindex (1 file) < 200 ms; full reindex (10k notes) < 10 min; hybrid search top-10 < 100 ms; graph load (1k nodes) < 1 s. **CI regression** uses a 1k-note fixture only; the full 10k benchmarks run in a dedicated nightly job (`bench.yml`), not on PRs.
  - [ ] SubTask 22.2: Add an offline test: set `FERGENTS_OFFLINE=1` (env var read by the kernel); any network attempt fails the test; run create/open/index/search/graph/multi-brain and assert no operation fails for network reasons.
  - [ ] SubTask 22.3: Test: the benchmark suite passes or documents an explicit, approved exception; offline test passes on all three OSes.

- [ ] Task 23: Documentation.
  - [ ] SubTask 23.1: Extend `DATA_MODEL.md` with the per-brain schema matching `migrations/brain/0001_initial.sql` (decision D7).
  - [ ] SubTask 23.2: Record ADR-016 … ADR-020 (spec Appendix E) in the ADR log.
  - [ ] SubTask 23.3: Update `README.md` with brain usage; add the Phase 1 `CHANGELOG.md` entry.
  - [ ] SubTask 23.4: Test: docs build/lint (if applicable) and links resolve.

- [ ] Task 24: Final CI hardening.
  - [ ] SubTask 24.1: Extend `ci.yml` for the desktop features; add `bench.yml` (nightly) for the `bench` feature with stub engines; add the offline test job (`FERGENTS_OFFLINE=1`).
  - [ ] SubTask 24.2: Verify CI is green on Windows, macOS, and Linux.
  - [ ] SubTask 24.3: Confirm the heavy models are never downloaded in CI (stub engines only); add a CI check that fails if a model download is attempted.

## Task Dependencies

- Task 0 precedes every other task (Phase 0 gaps must be merged first).
- Task 1 precedes Tasks 3, 5, 6, 7, 12 (crate scaffolding).
- Task 2 (protocol) can start after Task 1 and precedes Tasks 14, 15.
- Task 3 (brain model) precedes Tasks 4, 8, 14.
- Task 4 (per-brain schema) precedes Tasks 6, 7, 8, 9, 10, 11, 12.
- Task 5 (markdown) precedes Tasks 6, 9.
- Task 6 (chunking) precedes Task 7.
- Task 7 (embeddings) precedes Task 8 and Task 11.
- Task 8 (indexing/watcher) precedes Tasks 9, 10, 11, 12.
- Task 9 (links/backlinks) precedes Tasks 12, 20.
- Task 10 (BM25/filters) precedes Task 11.
- Task 11 (hybrid search) precedes Task 19.
- Task 12 (graph) precedes Task 20.
- Task 13 (daily/templates) depends on Tasks 3, 4, 8.
- Task 14 (multi-brain) depends on Tasks 2, 3, 4, 8; precedes Task 15.
- Task 15 (kernel/CLI) depends on Tasks 2, 8, 9, 11, 14.
- Task 16 runs in parallel with Tasks 17–21 (not a hard prerequisite); components are added incrementally as needed.
- Tasks 17–21 depend on Task 15 (protocol client); Task 18 precedes Task 17 (editor open action), Task 19 depends on 11, Task 20 depends on 12.
- Task 22 depends on Tasks 8, 11, 12, 15.
- Task 23 depends on Tasks 4, 14, 15.
- Task 24 depends on Tasks 1, 16, 17–21, 22; runs continuously alongside.