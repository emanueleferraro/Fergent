# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
