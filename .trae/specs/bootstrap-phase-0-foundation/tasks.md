# Tasks

Scope: **ROADMAP.md Phase 0 only**. Ordered, small, independently verifiable.
Every task ships with tests and keeps CI green.

- [ ] Task 0: Initialize the repository.
  - [ ] SubTask 0.1: `git init` in the repository root; add `.gitignore` (Rust, Node, OS, IDE, `~/.fergents` artifacts).
  - [ ] SubTask 0.2: Create the private GitHub repository (via `gh repo create`); set the default branch to `main`; push the initial commit.
  - [ ] SubTask 0.3: Protect `main` (require PR, require CI); enable GitHub Actions on the repository.
  - [ ] SubTask 0.4: Verify: `git remote -v` points to the new repo; CI is enabled but has nothing to run yet.

- [ ] Task 1: Bootstrap the monorepo skeleton.
  - [ ] SubTask 1.1: Create root `Cargo.toml` (workspace) and `crates/` layout; library crates `crates/{core,storage,events,protocol,api,supervisor}`; binary crates `crates/fergentsd` and `crates/fergents` (crate name matches binary name per spec).
  - [ ] SubTask 1.2: Create `pnpm-workspace.yaml` and `packages/{protocol,ui}` plus `apps/desktop`; wire pnpm scripts (build, lint, test, typecheck).
  - [ ] SubTask 1.3: Add `rust-toolchain.toml`, `.editorconfig`, `rustfmt.toml`, `.eslintrc`, `.prettierrc`, `tsconfig.json` (strict), `.gitignore`.
  - [ ] SubTask 1.4: Add governance docs `README.md`, `LICENSE` (AGPL-3.0), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
  - [ ] SubTask 1.5: Add `.devcontainer/` with Rust + Node toolchains; verify `cargo build` and `pnpm install && pnpm build` succeed inside the devcontainer.
  - [ ] SubTask 1.6: Verify: `cargo build` and `pnpm install` succeed from a clean checkout on the host.

- [ ] Task 2: Define the protocol contract and codegen.
  - [ ] SubTask 2.1: Author JSON Schema for commands (`hello`, `ping`, `system.info`, `system.health`, `events.subscribe`, `events.unsubscribe`, `events.replay`, `brain.list`, `kernel.shutdown`) and events (`kernel.started`, `kernel.stopping`, `kernel.error`, `system.health`, `events.dropped`) in `protocol/`.
  - [ ] SubTask 2.2: Add codegen to Rust types via `typify` into `crates/protocol`; commit generated output.
  - [ ] SubTask 2.3: Add codegen to TypeScript types via `json-schema-to-typescript` into `packages/protocol`; commit generated output.
  - [ ] SubTask 2.4: Implement JSON-RPC 2.0 envelope types, the error-code table from spec Appendix C, and `hello`/`welcome` version negotiation.
  - [ ] SubTask 2.5: Test: codegen freshness test (fails when generated output is stale) + envelope serde round-trip tests + version negotiation tests (accepts compatible, rejects unsupported major with `-32001`).

- [ ] Task 3: Implement storage layer and global schema.
  - [ ] SubTask 3.1: Define the `Storage` trait (`get`, `put`, `delete`, `query`, `transaction`).
  - [ ] SubTask 3.2: Implement `SqliteStorage`: pool of N read connections (WAL, foreign keys enabled), and a single writer task serialized through an mpsc channel (per spec).
  - [ ] SubTask 3.3: Write `migrations/global/0001_initial.sql` with the tables from the spec (`schema_version`, `events`, `audit_log`, `sessions`, `brains`, `projects`, `project_brains`, `llm_requests`, `settings`) with the specified columns and indexes.
  - [ ] SubTask 3.4: Implement the migration runner: apply numbered files, record versions in `schema_version`, refuse stores with a newer schema.
  - [ ] SubTask 3.5: Implement the audit log append + hash-chain verification (`hash = sha256(prev_hash || canonical_entry)`); genesis entry uses all-zero `prev_hash`.
  - [ ] SubTask 3.6: Test: migration idempotency, newer-schema refusal, CRUD, transaction rollback, audit-chain verify + tamper detection (reports first mismatching id).

- [ ] Task 4: Implement the event bus with SQLite persistence.
  - [ ] SubTask 4.1: Define the `Event` type (ULID `id`, `ts`, `kind`, `source`, `correlation_id`, optional `brain_id`/`project_id`, JSON `payload`, `version`) with monotonic ids.
  - [ ] SubTask 4.2: Implement publish → persist (append-only `events`) → broadcast to topic subscribers.
  - [ ] SubTask 4.3: Implement correlation ordering, replay-from-id, and bounded backpressure with an `events.dropped` notice.
  - [ ] SubTask 4.4: Test: unit tests for publish/subscribe, replay, ordering, backpressure; persistence across a simulated restart.

- [ ] Task 5: Build the kernel core and registry.
  - [ ] SubTask 5.1: Assemble the Tokio runtime and the `core` module wiring (event bus + storage + registries).
  - [ ] SubTask 5.2: Implement the registry for brains and sessions (persisted) and tools/providers (in-memory). Phase 0 ships an **empty** `brains` table and no brain-creation command.
  - [ ] SubTask 5.3: On startup, reconcile sessions lacking `closed_at` as closed.
  - [ ] SubTask 5.4: Implement `brain.list` (returns `[]` in Phase 0), `system.info`, and the `system.health` command; implement `ping` as a liveness probe (no diagnostics).
  - [ ] SubTask 5.5: Test: registry unit tests + session reconciliation test + `brain.list` returns an empty array (no error).

- [ ] Task 6: Implement the local IPC transport and JSON-RPC server.
  - [ ] SubTask 6.1: Define the `IpcTransport` trait and implement Windows named-pipe (`\\.\pipe\fergentsd`) and Unix socket (`$XDG_RUNTIME_DIR/fergentsd.sock`, fallback `~/.fergents/fergentsd.sock`) backends. (Depends only on Task 1.)
  - [ ] SubTask 6.2: Implement the `api` server: handshake (with a 5 s timeout, `-32002` on failure), per-client session, command dispatch, subscription registry, event push.
  - [ ] SubTask 6.3: Implement subscription commands (`events.subscribe`, `events.unsubscribe`, `events.replay`) with topic filtering.
  - [ ] SubTask 6.4: Test: transport tests per-OS + server tests for command dispatch, unknown method (JSON-RPC error), handshake timeout, and event delivery.

- [ ] Task 7: Add `fergentsd` lifecycle, logging, and health.
  - [ ] SubTask 7.1: Implement `fergentsd start` (default: background daemon; fork + setsid on Unix, detached process on Windows) and `fergentsd start --foreground`; single-instance enforcement via exclusive socket bind; write PID file at `~/.fergents/fergentsd.pid`.
  - [ ] SubTask 7.2: Implement `fergentsd status` (reads PID file, checks process existence, removes stale PID files) and `fergentsd stop` (SIGTERM / console close event, 10 s wait, report result).
  - [ ] SubTask 7.3: Implement graceful shutdown (emit `kernel.stopping`, close socket, flush storage, remove PID file, exit 0) with signal handling.
  - [ ] SubTask 7.4: Add `tracing` structured JSON logging with daily rotation to `~/.fergents/logs/`, 7-day retention (`tracing-appender`).
  - [ ] SubTask 7.5: Emit lifecycle events (`kernel.started`, `kernel.stopping`, `kernel.error`).
  - [ ] SubTask 7.6: Test: start/status/stop smoke test, single-instance test, stale-PID test, graceful-shutdown test.

- [ ] Task 8: Build the CLI TUI (`fergents`).
  - [ ] SubTask 8.1: Implement a kernel client (connect, handshake, send command, subscribe) with auto-reconnect.
  - [ ] SubTask 8.2: Implement `fergents status`, `fergents events tail`, `fergents brain list`.
  - [ ] SubTask 8.3: Implement the Ratatui TUI shell with a placeholder panel.
  - [ ] SubTask 8.4: Test: CLI command tests against a running kernel + a TUI render smoke test.

- [ ] Task 9: Scaffold the desktop shell (Tauri v2 + React 19).
  - [ ] SubTask 9.1: Create the Tauri v2 app; declare `fergentsd` as an `externalBin` sidecar in `tauri.conf.json`; resolve its path through Tauri's sidecar API; spawn it if absent; connect via the kernel client. (Depends on Task 6; can proceed in parallel with Tasks 7–8.)
  - [ ] SubTask 9.2: Build the placeholder shell (rail/sidebar/status bar) using only `style/` tokens and components (CSS include order `tokens.css` → `typography.css` → `components.css`); add any missing component to `style/` following `patterns.html` conventions; add any new token to `tokens.json` + `tokens.css` first.
  - [ ] SubTask 9.3: Implement the Ctrl+K command palette (`#cmdk` pattern) with stub commands dispatching through the protocol.
  - [ ] SubTask 9.4: Implement the dark/light theme toggle driven by tokens with persistence (dark default).
  - [ ] SubTask 9.5: Add i18n scaffolding (English default, all strings externalized).
  - [ ] SubTask 9.6: Test: vitest unit tests for the client hook, palette, and theme; verify kernel survives app exit.

- [ ] Task 10: Set up testing harness and CI.
  - [ ] SubTask 10.1: Add the end-to-end integration test (start kernel on a temp socket → connect → command → event → shutdown).
  - [ ] SubTask 10.2: Add a minimal desktop smoke test (launch → connect → render) using `tauri-driver` + WebdriverIO (or an equivalent harness). Full visual regression is deferred to Phase 3.
  - [ ] SubTask 10.3: Add `ci.yml` (matrix Windows/macOS/Linux: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build`, `eslint`, `prettier --check`, `tsc --noEmit`, pnpm build).
  - [ ] SubTask 10.4: Add `release.yml` as a tag-triggered stub.
  - [ ] SubTask 10.5: Verify CI is green on all three OSes.

- [ ] Task 11: Write the Phase 0 documentation deliverables.
  - [ ] SubTask 11.1: Write `DATA_MODEL.md` documenting the global schema (and per-brain deferral to Phase 1) matching `0001_initial.sql`.
  - [ ] SubTask 11.2: Update `README.md` with build/run instructions; update `ARCHITECTURE.md`/`STACK.md` only if implementation diverges (otherwise note "as specified").
  - [ ] SubTask 11.3: Add `CHANGELOG.md` with the Phase 0 entry.

## Task Dependencies

- Task 0 precedes Task 1.
- Task 2 depends on Task 1.
- Task 3 depends on Task 1; SubTask 3.5 depends on SubTasks 3.3 and 3.4.
- Task 4 depends on Task 3 (persistence).
- Task 5 depends on Tasks 2, 3, 4.
- Task 6: SubTask 6.1 depends only on Task 1; SubTasks 6.2–6.4 depend on
  Tasks 2 and 5.
- Task 7 depends on Task 6.
- Task 8 depends on Task 7.
- Task 9 depends on Task 6 and can proceed in parallel with Tasks 7–8.
- Task 10 depends on Tasks 7 and 9 (end-to-end and smoke test).
- Task 11 depends on Tasks 3 and 10.