# Checklist

Verification points for Phase 1 (Brain). Each item maps to a spec requirement
in `.trae/specs/brain-phase-1/spec.md`. Check only when the referenced
evidence exists and passes.

> Status legend: `[x]` verified (evidence present/run); `[ ]` not yet verified.

## Phase 0 gaps (Appendix F / decision D12)

- [ ] Log retention prunes rotated logs older than 7 days on startup and daily; `[log] retention_days`/`max_files` are configurable in `config.toml`.
- [ ] `fergentsd start` detaches and survives the parent shell (Unix `setsid`, Windows `DETACHED_PROCESS` + `CREATE_NEW_PROCESS_GROUP`).
- [ ] `fergentsd stop` signals the daemon, waits up to 10 s, and reports the outcome.
- [ ] `fergentsd status` checks PID existence (not only the socket) and clears stale PID files.
- [ ] All four merged before Phase 1 feature work starts.

## Workspace & protocol

- [ ] `crates/{markdown,brain,index,graph}` exist and build; only justified dependencies were added (each in the task that uses it).
- [ ] Phase 1 command + event schema exists under `protocol/`; Rust and TS bindings are generated and committed.
- [ ] Codegen freshness tests pass; new envelope/event round-trip tests pass.
- [ ] `search.query` accepts `mode: bm25|vector|sparse|hybrid` (default `hybrid`).
- [ ] `links.orphans` and `links.dead_ends` are part of the protocol surface.

## Brain model & vault

- [ ] A brain is a folder with `brain.toml` + `vault/`; identity is a stable id in `brain.toml`.
- [ ] `brain.toml` validates all sections; invalid input errors clearly (no panic).
- [ ] `[sync]` and `[git]` sections are parsed-only in Phase 1 (no runtime effect); they are shown read-only in the settings page.
- [ ] `index.sqlite` and `graph.db` are regenerable from `brain.toml` + `vault/`.
- [ ] Ignored paths (`.fergents/`, `index.sqlite*`, `graph.db*`, OS junk, `.brainignore`) are excluded.

## Per-brain schema

- [ ] `migrations/brain/0001_initial.sql` matches spec Appendix B exactly.
- [ ] Migrations apply on first open and are idempotent; a newer schema is refused.
- [ ] `chunks_fts` (FTS5) and `vec_chunks` (`sqlite-vec`) exist and are usable; WAL + `foreign_keys` ON.

## Indexing & watcher

- [ ] Full scan on open; incremental reindex of one changed file updates within 1 s.
- [ ] Deletion is a soft delete and is restorable.
- [ ] `index_jobs` persists and resumes after an interrupted run.
- [ ] 50 MB skip and 1M-token chunk-only-without-embedding limits are enforced.
- [ ] The watcher watches only the vault and honors ignored paths.

## Markdown flavor & chunking

- [ ] Wiki links, embeds, block refs, tags, callouts, math, Mermaid, and frontmatter are parsed.
- [ ] Unknown frontmatter keys are preserved and queryable.
- [ ] Chunks store `heading_path`, `token_count`, `start/end_offset`; adjacent chunks overlap per config.
- [ ] Markdown, plain-text, and attachment-text chunkers work; code chunking is deferred (D8).

## Embeddings

- [ ] `VectorStore` trait wraps `sqlite-vec`; no direct engine SQL in business code.
- [ ] BGE-M3 embeddings are generated (dense + sparse) with a deterministic test stub; CI never downloads the model (feature `embed-stub`).
- [ ] Identical content is never re-embedded (content-hash cache); batching works.
- [ ] A model change warns and offers a background re-index.

## Search

- [ ] BM25 (FTS5) ranking works; query analysis handles terms, phrases, negation.
- [ ] All filters work (tag, path, type, language, after/before, has, frontmatter, brain).
- [ ] RRF fusion (k=60) and cross-encoder rerank (top-20 → top-8) are implemented; trivial queries skip rerank; reranker has a stub for tests (feature `rerank-stub`).
- [ ] Result shape matches `SEARCH.md` §11.1 (per-stage scores + citation).
- [ ] `bench/search/queries.json` (50 queries, ≥3 domains) exists; hybrid beats BM25 alone by ≥5 pp; the benchmark runs behind a `bench` feature, `#[ignore]` by default, in the nightly job.
- [ ] `search.query` command (with `mode`) and `search.executed` event work.

## Backlinks, outline, mentions

- [ ] Backlinks (with anchor/display text) and unresolved links are returned from `links`.
- [ ] Outlines return the heading tree; unlinked mentions are suggested.
- [ ] Orphans (`links.orphans`) and dead-ends (`links.dead_ends`) are detected.
- [ ] `tag.list|rename|delete` work and update `document_tags`.

## Graph

- [ ] `GraphStore` trait wraps CozoDB (`graph.db`); the SQLite mirror stays in sync on reindex.
- [ ] Regex extraction always on; NER (person/org/place) behind `EntityExtractor`, degrading to regex-only; LLM extraction off by default.
- [ ] Structural relations (`links_to`, `mentions`, `tagged_with`, `in_folder`) recorded with confidence 1.0.
- [ ] `graph.query|algo|neighbors|subgraph|entities.merge` work; `graph.updated` is emitted.

## Daily notes & templates

- [ ] Daily note is created once per day from the configured template and indexed.
- [ ] Templates render `{{date}}|{{time}}|{{datetime}}|{{title}}|{{selection}}|{{clipboard}}|{{cursor}}`.

## Multi-brain

- [ ] `brain.create` (templates), `open/close/get/update/archive/unarchive/delete` work with events.
- [ ] `duplicate` yields an independent copy (new id); `move` preserves the id.
- [ ] `.fergbrain` export/import round-trips with `manifest.json` + checksums; regenerable files excluded.
- [ ] `attach`/`detach` with roles; `read-only` rejects writes. Protocol/CLI only in Phase 1 (no desktop UI yet).
- [ ] `brain.template.save` writes under `$FERGENTS_HOME/templates/brains/`.
- [ ] Cross-brain links resolve when attached, else show unresolved; cross-brain search filter works.
- [ ] `brain.list` returns real brains; `[]` on a fresh install (Phase 0 behavior preserved).

## Kernel & CLI

- [ ] The brain service is registered in `core`; all Phase 1 commands dispatch via `api`.
- [ ] CLI: `fergents brain list|create|open`, `fergents search` work against a running kernel.

## Desktop UI & design system

- [ ] Brain sidebar (file tree, tags, search box, backlink/outline panels) renders and navigates by keyboard.
- [ ] Editor uses CodeMirror 6 (D1) with tabs + live preview (KaTeX math, Mermaid, callouts, resolved links).
- [ ] Search panel shows snippets/highlights with keyboard navigation.
- [ ] Graph view uses React Flow (D2) with filters, in-graph search, layout presets, views; CI verifies load of 1000 nodes in < 1 s and node-count match; the 60 fps interaction target is verified manually (recorded in the PR body) and only triggers ADR-017 migration if it fails.
- [ ] Per-brain settings page loads/saves via `brain.update`; `[sync]` and `[git]` shown read-only.
- [ ] Every element uses only `style/` tokens/components; no hardcoded colors/spacing/radii/motion; any new component/token was added to `style/` first.

## Offline & performance

- [ ] Offline test with `FERGENTS_OFFLINE=1`: create/open/index/search/graph/multi-brain all work; any network attempt fails the test.
- [ ] Benchmarks (behind `bench` feature, `#[ignore]` by default) meet the targets; CI regression uses a 1k-note fixture; the full 10k suite runs nightly.
- [ ] Open brain (10k notes) < 2 s; incremental reindex < 200 ms; full reindex (10k notes) < 10 min; hybrid search top-10 < 100 ms; graph load (1k nodes) < 1 s.

## Testing & CI

- [ ] Kernel tests cover `brain.toml` round-trip, per-brain migrations, chunking, link/tag extraction, the search pipeline (stub engines), RRF, backlinks, `.fergbrain` round-trip, and multi-brain ops — without downloading the model.
- [ ] UI tests cover the sidebar, editor tabs, search panel, and graph view with mock data.
- [ ] End-to-end test (create → note → index → search → backlinks) passes on a temp vault.
- [ ] CI is green on Windows, macOS, and Linux; heavy models are never downloaded.
- [ ] `bench.yml` (nightly) runs the full benchmark suite; `ci.yml` runs the regression subset only.

## Constraints (global)

- [ ] No unjustified dependency; no global mutable state.
- [ ] Phase 0 code modified only where Phase 1 strictly requires it.
- [ ] No Phase 2+ features (LLM router, MCP, agents, Monaco/LSP/terminal, skills/plugins, workflows, dashboard, sync/mobile).
- [ ] Conventional Commits in every commit.