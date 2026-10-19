# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

**Phase 0 — Foundations**

- Monorepo: Cargo workspace (`crates/`) and pnpm workspace (`packages/`,
  `apps/`), with the shared protocol contract under `protocol/`.
- Kernel daemon `fergentsd`: JSON-RPC 2.0 server over a local socket, runtime
  registry (tools, providers, brains, sessions), health check, structured
  logging, and graceful shutdown. Entry points: `fergentsd start`,
  `fergentsd start --foreground`, `fergentsd status`, `fergentsd stop`.
- Event bus with SQLite persistence: append-only `events` table, topic
  filtering, correlation ordering, replay-from-id, and bounded backpressure.
  Lifecycle events `kernel.started`, `kernel.stopping`, and `kernel.error`
  are published on the corresponding transitions.
- Protocol and codegen: JSON Schema definitions for commands and events in
  `protocol/`, with committed Rust (`typify`) and TypeScript
  (`json-schema-to-typescript`) bindings and handshake version negotiation.
- Storage and migrations: global SQLite schema, numbered migration framework
  (`migrations/global/0001_initial.sql`), the `Storage` trait, and the
  `SqliteStorage` implementation. Connections use WAL with foreign keys
  enabled; reads are served by a pool of `min(4, cpu_count)` connections while
  all writes are serialized through a single dedicated writer task.
- Audit log: append-only, tamper-evident hash chain
  (`hash(n) = sha256(prev_hash || canonical_entry(n))`) with chain
  verification.
- Desktop shell: Tauri v2 + React 19 app that spawns `fergentsd` as a sidecar,
  connects over the local socket, and renders a placeholder UI built from
  `style/` tokens, with a Ctrl+K command palette, dark/light theme toggle, and
  i18n scaffolding.
- CLI (TUI): `fergents` binary (Ratatui) with `fergents status`,
  `fergents events tail`, and `fergents brain list` (stub).
- CI: GitHub Actions `ci.yml` (lint, test, build across Windows, macOS, and
  Linux) and an inert `release.yml` stub.
- Testing harness: kernel unit tests, a `fergentsd` end-to-end/lifecycle suite
  (start → connect → command → event → shutdown, single-instance, stale PID,
  graceful shutdown), and desktop/CLI render smoke tests.
- Repository governance and tooling: `README.md`, `LICENSE`
  (AGPL-3.0-or-later), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `rust-toolchain.toml`, and `.devcontainer/`.

**Phase 1 — Brain**

- Brain model and lifecycle (`crates/brain`): `brain.toml`, vault layout, and
  the full lifecycle — create from a template (`blank`, `personal`, `research`,
  `coding`, `team`), open, close, get, update, archive/unarchive, and delete;
  `brain.duplicate` and `brain.move`; `.fergbrain` export/import with a
  `manifest.json` and checksums; `brain.attach`/`brain.detach` with roles
  (primary, read-only, scratch) enforced on writes; `brain.template.save`;
  cross-brain link resolution (`[[brain:…]]`, `[[id:…]]`) and the cross-brain
  search filter; `brain.reindex`/`brain.indexed`; and a real `brain.list`.
- Per-brain index and search (`crates/index`): the SQLite index schema
  (documents, chunks, external-content FTS5, `sqlite-vec` embeddings, tags,
  links, entity mirror), chunking, embeddings behind swappable engines
  (ADR-018), hybrid search with BM25 + vector + sparse fusion and optional
  rerank, and daily notes and templates with `strftime` paths and a
  `{{cursor}}` offset.
- Entity graph (`crates/graph`): a CozoDB Datalog graph (`storage-sled`) with a
  SQLite mirror, regex entity extraction plus a feature-gated ONNX NER, and
  `graph.query`, `graph.algo` (including `shortest_path`), `graph.neighbors`,
  `graph.subgraph`, and `graph.merge_entities`.
- Kernel wiring and CLI: the Phase 1 brain commands dispatched over the
  JSON-RPC socket (with the indexing worker on the kernel runtime), and the
  `fergents brain list|create|open` and `fergents search <query>` CLI commands.
- Desktop UI (`apps/desktop`): brain sidebar and explorer (file tree, tag list,
  search box, backlinks, outline, unlinked mentions), a CodeMirror 6 markdown
  editor with live preview (KaTeX, Mermaid, callouts, wiki links) and tabs wired
  to `doc.write`, a search panel with snippets, highlights, filters, and
  keyboard navigation, a React Flow graph view (local/global/entity/backlink/
  filtered views, layout presets, filters, in-graph search), and a per-brain
  settings page wired to `brain.update`.
- Design system: the graph, editor, sidebar, search, and settings components
  added to `style/components.css` on the existing tokens.
- Verification: performance benchmarks behind the `bench` feature (open,
  reindex, incremental reindex, hybrid search, graph load) and an offline test
  driven by `FERGENTS_OFFLINE=1`; the heavy models are never downloaded in CI
  (deterministic stubs only).

### Changed

- `DATA_MODEL.md` now documents the per-brain schema alongside the global one.
- `DECISIONS.md` added: the ADR log, recording ADR-001 … ADR-020.
- `ARCHITECTURE.md` §17 and `README.md` updated for the Phase 1 crates,
  commands, and UI.
