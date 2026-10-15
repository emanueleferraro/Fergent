# Bootstrap Phase 0 Foundation Spec

## Why

Fergents does not exist as a codebase yet: the repository contains only the
documentation set (`README.md`, `ARCHITECTURE.md`, `STACK.md`, `ROADMAP.md`,
the runtime/data/UX docs) and the design system (`style/`). Nothing builds,
nothing runs. Phase 0 (ROADMAP.md §4) defines the foundation everything else
depends on: a monorepo that builds, a kernel daemon that runs and persists
events, a client that connects to it, and the first CI pipeline. Without this
skeleton no later phase (Brain, Agent, IDE, …) can start.

This spec delivers exactly **ROADMAP.md §4 (Phase 0)** and nothing beyond it.

## What Changes

This is a **greenfield** change: it creates the entire Phase 0 skeleton.

- **Repository**: Cargo workspace + pnpm workspace monorepo, devcontainer,
  governance docs (`README`, `LICENSE`, `CONTRIBUTING`, `CODE_OF_CONDUCT`),
  formatter/linter configs (`rustfmt.toml`, `.editorconfig`, `.eslintrc`,
  `.prettierrc`), toolchain pins (`rust-toolchain.toml`,
  `pnpm-workspace.yaml`), and GitHub Actions (`ci.yml`, `release.yml` stub).
- **Kernel (`fergentsd`)**: Rust daemon with library crates `core`, `storage`,
  `events`, `protocol`, `api`, `supervisor`, plus the `fergentsd` binary crate.
  SQLite-backed event bus; runtime registry (tools, providers, brains,
  sessions); JSON-RPC 2.0 server over a local socket; `fergentsd
  start|stop|status` entry point; structured logging (`tracing`); health
  check; graceful shutdown.
- **Protocol**: JSON Schema definitions for commands and events in
  `protocol/`; codegen to Rust (via `typify`) and TypeScript (via
  `json-schema-to-typescript`) types; version negotiation on handshake.
- **Storage**: a self-designed SQLite schema for the global store
  (`~/.fergents/global.sqlite`), a numbered migration framework, and the
  `Storage` trait + `SqliteStorage` implementation (ARCHITECTURE.md §7).
- **Desktop shell (Tauri v2 + React 19)**: app that ships `fergentsd` as a
  Tauri sidecar, spawns it if absent, connects over the socket, and renders a
  placeholder shell built **only** from `style/` tokens/components; Ctrl+K
  command palette with stub commands; dark/light theme toggle; i18n
  scaffolding.
- **CLI (TUI)**: `fergents` binary (Ratatui) that connects to `fergentsd`,
  with `fergents status`, `fergents events tail`, `fergents brain list`
  (stub) and a placeholder panel.
- **Testing**: unit tests for event bus, protocol codegen, and storage; an
  end-to-end integration test (start kernel → connect → command → event); a
  minimal desktop smoke test (launch → connect → render).

There are **no BREAKING changes** (nothing exists to break).

## Impact

- **Affected specs (documentation as source of truth)**: `ARCHITECTURE.md`
  §3 (kernel/process), §3.4 (protocol), §4 (event bus), §7 (storage),
  §9 (permissions — minimal), §14 (observability), §16 (lifecycle);
  `SECURITY.md` §16 (audit log); `STACK.md` §2, §4, §13, §18;
  `UI.md` (layout, command palette, themes, i18n); `ROADMAP.md` §4;
  `style/` (design system, MANDATORY).
- **Affected code (new)**: root `Cargo.toml`,
  `crates/{core,storage,events,protocol,api,supervisor}`, `crates/fergentsd`,
  `crates/fergents`, `packages/{protocol,ui}`, `apps/desktop`,
  `protocol/*.schema.json`, `migrations/global/0001_initial.sql`,
  `.github/workflows/*`, `rust-toolchain.toml`, `pnpm-workspace.yaml`,
  `.devcontainer/`.
- **Out of scope (later phases)**: per-brain index/graph database, real
  brain features, LLM router, MCP host/server, agent runtime, Monaco/LSP,
  WASM plugin host, workflow engine, sync, mobile.

## ADDED Requirements

### Requirement: Monorepo and Toolchain

The repository SHALL be a single monorepo containing a Cargo workspace
(`crates/`, root `Cargo.toml`) and a pnpm workspace (`packages/`, `apps/`,
`pnpm-workspace.yaml`), with shared contract sources under `protocol/`.

#### Scenario: Workspace builds from a clean checkout

- **WHEN** a developer runs `cargo build --release` at the repository root
- **THEN** all Rust crates and both binaries compile without errors
- **AND** `pnpm install && pnpm build` succeeds in the pnpm workspace

#### Scenario: Toolchain is pinned and reproducible

- **WHEN** the repository is opened
- **THEN** `rust-toolchain.toml` pins the Rust channel/components and
  `pnpm-workspace.yaml` plus lockfiles pin the Node toolchain
- **AND** `.devcontainer/` provisions all required toolchains
- **AND** `cargo build` and `pnpm build` succeed inside the devcontainer, not
  only on the host

#### Scenario: Governance and formatting files exist

- **WHEN** a contributor inspects the repository root
- **THEN** `README.md`, `LICENSE` (AGPL-3.0), `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `.editorconfig`, `rustfmt.toml`, `.eslintrc`, and
  `.prettierrc` are present

#### Scenario: Crate names match binary names

- **WHEN** a contributor inspects `crates/`
- **THEN** the daemon binary crate is `crates/fergentsd` (produces the
  `fergentsd` binary) and the CLI binary crate is `crates/fergents` (produces
  the `fergents` binary)
- **AND** library crates are `crates/core`, `crates/storage`, `crates/events`,
  `crates/protocol`, `crates/api`, `crates/supervisor`

### Requirement: Kernel Daemon (`fergentsd`)

The system SHALL provide a Rust daemon binary `fergentsd` organized into the
crate structure above (ARCHITECTURE.md §3.2), exposing `fergentsd start`,
`fergentsd start --foreground`, `fergentsd stop`, and `fergentsd status`.

#### Scenario: Start as a background daemon (default)

- **WHEN** the user runs `fergentsd start`
- **THEN** the process detaches (fork + setsid on Unix; detached process on
  Windows), binds the local socket, writes a PID file at
  `~/.fergents/fergentsd.pid`, persists a `kernel.started` event, and becomes
  reachable

#### Scenario: Start in foreground

- **WHEN** the user runs `fergentsd start --foreground`
- **THEN** the daemon runs in the current process, streams logs to stderr, and
  does not detach

#### Scenario: Single instance enforcement

- **WHEN** `fergentsd start` is invoked while another instance is running
- **THEN** the second instance exits with a non-zero code and a clear
  message: `another instance is running (PID <pid>)`
- **AND** the socket bind is the primary lock; the PID file is a secondary
  record

#### Scenario: Status reads the PID file

- **WHEN** the user runs `fergentsd status`
- **THEN** the daemon reads `~/.fergents/fergentsd.pid`, checks whether the
  process exists, and reports `running (PID <pid>)` or `not running`
- **AND** a stale PID file (process does not exist) is removed and reported
  as `not running (stale pid file removed)`

#### Scenario: Stop sends a termination signal

- **WHEN** the user runs `fergentsd stop`
- **THEN** the daemon reads the PID file, sends SIGTERM on Unix or a console
  close event on Windows, waits up to 10 s for exit, and reports the result
- **AND** if the process does not exit within the timeout, the user is
  informed and can force-kill manually

#### Scenario: Graceful shutdown

- **WHEN** the daemon receives a shutdown signal or `kernel.shutdown` command
- **THEN** it persists a `kernel.stopping` event, closes the socket, flushes
  storage, removes the PID file, and exits with code 0

#### Scenario: Structured logs

- **WHEN** the daemon starts and handles a request
- **THEN** structured JSON logs are written via the `tracing` crate to
  `~/.fergents/logs/`
- **AND** logs rotate daily (`tracing-appender` with `Rotation::DAILY`) and
  are retained for 7 days

#### Scenario: Health is queryable

- **WHEN** a client sends the `system.health` command
- **THEN** the kernel returns uptime, version, storage status, and a health
  verdict

#### Scenario: Lifecycle events are emitted

- **WHEN** the kernel starts and stops
- **THEN** `kernel.started` and `kernel.stopping` events are persisted and
  broadcast
- **AND** on an unrecoverable error, a `kernel.error` event is emitted with
  a structured payload

### Requirement: Local IPC Transport

The system SHALL abstract local IPC behind an `IpcTransport` trait, using a
named pipe `\\.\pipe\fergentsd` on Windows and
`$XDG_RUNTIME_DIR/fergentsd.sock` (fallback `~/.fergents/fergentsd.sock`) on
Unix (ARCHITECTURE.md §3.3).

#### Scenario: Connect from a client

- **WHEN** a client connects to the socket
- **THEN** the connection is accepted and associated with a new kernel session

#### Scenario: Multiple clients coexist

- **WHEN** two clients connect concurrently
- **THEN** each obtains its own session with independent subscriptions

#### Scenario: Client reconnect after kernel restart

- **WHEN** the kernel is killed and restarted while a client is connected
- **THEN** the client detects the disconnect and reconnects automatically

#### Scenario: Localhost trust boundary

- **WHEN** a client connects from the local machine
- **THEN** the kernel accepts the connection without authentication, treating
  the OS user boundary as the trust boundary (documented in the spec; auth
  arrives with the self-hosted server in a later phase)

### Requirement: Kernel Protocol (JSON-RPC 2.0)

The system SHALL expose a JSON-RPC 2.0 protocol over the local socket with
three primitives — Command, Event, Stream — where the protocol contract is
defined by JSON Schema in `protocol/` (ARCHITECTURE.md §3.4).

#### Scenario: Command round-trip

- **WHEN** a client sends a valid command (e.g. `system.health`)
- **THEN** the kernel returns a well-formed JSON-RPC result correlated by `id`
- **AND** a trivial command round-trips in under 50 ms locally

#### Scenario: Invalid command is rejected

- **WHEN** a client sends an unknown method or malformed params
- **THEN** the kernel returns a JSON-RPC error object with a stable error code
  (see Appendix C) and does not crash

#### Scenario: Event notification delivery

- **WHEN** an event is emitted on a topic the client subscribed to
- **THEN** the kernel pushes a JSON-RPC notification containing the event
  with `id`, `ts`, `kind`, `source`, `correlation_id`, `payload`, `version`

#### Scenario: Handshake timeout

- **WHEN** a client connects but does not send `hello` within 5 s
- **THEN** the kernel closes the connection with error code `-32002`
  (`handshake required`) and frees the session slot

### Requirement: Protocol Codegen and Version Negotiation

The system SHALL generate Rust and TypeScript types from the JSON Schema in
`protocol/`, and SHALL negotiate the protocol version during the handshake.

- **Rust codegen**: `typify` reads the JSON Schema and emits Rust types into
  `crates/protocol`.
- **TypeScript codegen**: `json-schema-to-typescript` reads the same JSON
  Schema and emits TypeScript types into `packages/protocol`.
- Both outputs are committed. A freshness test fails when generated output
  is stale relative to the schema.

#### Scenario: Generated bindings stay in sync

- **WHEN** a developer modifies a JSON Schema file and runs the codegen
- **THEN** regenerated Rust and TypeScript types reflect the change
- **AND** a test fails if generated output is stale relative to the schema

#### Scenario: Handshake negotiates a compatible version

- **WHEN** a client sends `hello` with `protocol_version`, `client_kind`, and
  `capabilities`
- **THEN** the kernel replies with `welcome` containing its version and
  capabilities if compatible
- **AND** the kernel rejects the handshake with `-32001` (`protocol version
  unsupported`) if the client's major protocol version is unsupported

### Requirement: Event Bus with SQLite Persistence

The system SHALL provide an event bus that persists every event to an
append-only `events` table in the global SQLite store, using monotonic ULID
ids, supports topic filtering, correlation ordering, replay-from-id, and
bounded backpressure (ARCHITECTURE.md §4).

#### Scenario: Event persists across restart

- **WHEN** an event is emitted and the kernel is restarted
- **THEN** the event is still present in the `events` table

#### Scenario: Subscribe and tail

- **WHEN** a client subscribes to a topic and a matching event is emitted
- **THEN** the client receives the event notification

#### Scenario: Replay from a given event id

- **WHEN** a client requests replay from a known event id
- **THEN** the kernel delivers all matching events with a greater monotonic id

#### Scenario: Slow subscriber backpressure

- **WHEN** a subscriber cannot keep up beyond its buffer limit
- **THEN** the kernel drops events for that subscriber and emits an
  `events.dropped` notice indicating the last seen id

### Requirement: Registry

The system SHALL provide a runtime registry for tools, providers, brains, and
sessions, exposed through kernel commands. Brains and sessions SHALL be
persisted; tools and providers MAY be in-memory in Phase 0.

#### Scenario: Kernel restart marks sessions closed

- **WHEN** the kernel starts after an unclean stop
- **THEN** sessions without a `closed_at` are reconciled as closed

#### Scenario: Brains are listed (empty in Phase 0)

- **WHEN** a client sends `brain.list`
- **THEN** the kernel returns the registered brains from the `brains` table
- **AND** in Phase 0 the table is empty, so the command returns an empty
  array (no error)
- **AND** brain creation and folder scanning are deferred to Phase 1

### Requirement: Storage Layer and Migrations

The system SHALL define a `Storage` trait (`get`, `put`, `delete`, `query`,
`transaction`) with a `SqliteStorage` implementation, and SHALL apply
numbered SQL migrations from `migrations/`, recording versions in a
`schema_version` table and refusing to open a store newer than it understands
(ARCHITECTURE.md §7.3, §7.4).

#### Scenario: Migrations apply on open

- **WHEN** the kernel opens the global store for the first time
- **THEN** it applies `migrations/global/0001_initial.sql` and records the
  version in `schema_version`

#### Scenario: Migrations are idempotent

- **WHEN** the kernel opens an already-migrated store
- **THEN** no migration is re-applied and the store opens without error

#### Scenario: Newer schema is refused

- **WHEN** the on-disk `schema_version` is greater than the kernel's known
  maximum
- **THEN** the kernel refuses to open the store and returns a clear error

#### Scenario: Transactional writes

- **WHEN** a write is performed inside a `Storage::transaction`
- **THEN** it is atomic: on failure the transaction rolls back with no
  partial writes

#### Scenario: Connection pool discipline

- **WHEN** the store is opened
- **THEN** the kernel creates a pool of N read connections (N = min(4,
  cpu_count)) in WAL mode
- **AND** a single writer task owns all writes, serialized through an mpsc
  channel; no concurrent writers exist
- **AND** foreign keys are enabled on every connection

### Requirement: Global Database Schema

The system SHALL ship the Phase 0 global schema described below in
`migrations/global/0001_initial.sql`, following ARCHITECTURE.md §7.1 (global
store holds cross-brain state) and SECURITY.md §16.3 (`audit_log`).

Tables (columns are normative):

- **`schema_version`** — `version INTEGER PRIMARY KEY`, `description TEXT`,
  `checksum TEXT`, `applied_at INTEGER`.
- **`events`** — `id TEXT PRIMARY KEY` (ULID), `ts INTEGER`, `kind TEXT`,
  `source TEXT`, `correlation_id TEXT NULL`, `brain_id TEXT NULL`,
  `project_id TEXT NULL`, `payload TEXT` (JSON), `version INTEGER`; indexes
  on `(kind, ts)`, `(correlation_id)`, `(ts)`.
- **`audit_log`** — `id TEXT PRIMARY KEY`, `ts INTEGER`, `actor_type TEXT`,
  `actor_id TEXT`, `session_id TEXT NULL`, `action TEXT`, `target TEXT NULL`,
  `args_json TEXT` (JSON), `result TEXT` (`allowed|denied|error`),
  `reason TEXT NULL`, `correlation_id TEXT NULL`, `reversible INTEGER`,
  `revert_ref TEXT NULL`, `prev_hash TEXT`, `hash TEXT`; indexes on
  `(actor_type, actor_id, ts)`, `(action, ts)`, `(ts)`.
- **`sessions`** — `id TEXT PRIMARY KEY`, `client_kind TEXT`, `client_version
  TEXT NULL`, `protocol_version INTEGER`, `capabilities TEXT` (JSON),
  `started_at INTEGER`, `last_seen_at INTEGER`, `closed_at INTEGER NULL`.
- **`brains`** — `brain_id TEXT PRIMARY KEY` (UUID from `brain.toml`),
  `name TEXT`, `path TEXT` (unique), `role TEXT NULL`,
  `schema_version INTEGER`, `created_at INTEGER`, `last_opened_at INTEGER
  NULL`.
- **`projects`** — `project_id TEXT PRIMARY KEY`, `name TEXT`, `path TEXT`,
  `created_at INTEGER`.
- **`project_brains`** — `project_id REFERENCES projects`, `brain_id
  REFERENCES brains`, `role TEXT` (`primary|read-only|scratch`), primary key
  `(project_id, brain_id)`.
- **`llm_requests`** — `id TEXT PRIMARY KEY`, `ts INTEGER`, `provider TEXT
  NULL`, `model TEXT NULL`, `task TEXT NULL`, `input_tokens INTEGER`,
  `output_tokens INTEGER`, `cached_tokens INTEGER`, `cost_usd REAL`,
  `latency_ms INTEGER NULL`, `agent_id TEXT NULL`, `session_id TEXT NULL`,
  `status TEXT`; indexes on `(ts)`, `(provider, model, ts)`.
- **`settings`** — `key TEXT PRIMARY KEY`, `value_json TEXT`,
  `updated_at INTEGER`.

#### Scenario: Global store is initialized

- **WHEN** the kernel creates `~/.fergents/global.sqlite`
- **THEN** all tables above exist with the specified columns, the database is
  in WAL mode with foreign keys enabled, and `schema_version` has one row

### Requirement: Audit Log Hash Chain

The audit log SHALL be append-only and tamper-evident: each entry stores
`prev_hash` and `hash = sha256(prev_hash || canonical_entry)`, forming a
verifiable chain (SECURITY.md §16.6). Phase 0 SHALL implement append and
chain-verification; the permission writes that populate it arrive in Phase 2.

#### Scenario: Chain verifies on a fresh log

- **WHEN** entries are appended in order and the chain is verified
- **THEN** verification succeeds and the genesis entry uses a defined
  `prev_hash` (all-zero) value

#### Scenario: Tampering is detected

- **WHEN** any historical entry's fields are modified out of band
- **THEN** chain verification fails and reports the first mismatching id

### Requirement: Observability and Health

The system SHALL provide structured logging (`tracing`), a health check
exposed as a kernel command, and lifecycle events
(`kernel.started`, `kernel.stopping`, `kernel.error`) on the bus.

#### Scenario: Health command vs health event

- **WHEN** a client sends the `system.health` **command**
- **THEN** the kernel returns the current health snapshot
- **AND** the `system.health` **event** is emitted only when the health
  verdict transitions (e.g. `healthy → degraded`, `degraded → healthy`)

#### Scenario: Ping is liveness, not health

- **WHEN** a client sends `ping`
- **THEN** the kernel replies with `pong` and the current timestamp,
  regardless of internal subsystem state (liveness only, no diagnostics)

### Requirement: Desktop Shell (Tauri v2 + React 19)

The system SHALL provide a Tauri v2 desktop app that ships `fergentsd` as a
Tauri sidecar, spawns it if it is not running, connects over the local
socket, and renders a placeholder shell built exclusively from `style/`
tokens and components (STACK.md §3, §13; ARCHITECTURE.md §5.1).

#### Scenario: Launch and connect

- **WHEN** the user launches the desktop app with no kernel running
- **THEN** the app spawns `fergentsd` from its bundled sidecar path, connects,
  and renders the shell

#### Scenario: Kernel survives app exit

- **WHEN** the user closes the desktop app
- **THEN** the kernel keeps running (the sidecar is spawned detached; the app
  does not own its lifecycle)

#### Scenario: Sidecar path is deterministic

- **WHEN** the app spawns `fergentsd`
- **THEN** the path is resolved through Tauri's sidecar API, and the same
  `fergentsd` binary is shipped in the app bundle on every platform

#### Scenario: Design-system compliance

- **WHEN** any shell element is rendered
- **THEN** it uses only `style/` tokens and components (CSS include order:
  `tokens.css` → `typography.css` → `components.css`), with no hardcoded
  colors, spacing, radii, or motion
- **AND** if a component is missing it is added to `style/` following
  `patterns.html` conventions, and any new token is first added to
  `tokens.json` and `tokens.css`

### Requirement: Command Palette (Ctrl+K)

The desktop shell SHALL provide a Ctrl+K command palette with stub commands
(using the `style/patterns.html` `#cmdk` pattern) that dispatch through the
kernel protocol.

#### Scenario: Open and run a stub command

- **WHEN** the user presses Ctrl+K, types, and selects a command
- **THEN** the palette filters/executes the corresponding stub action and
  closes

#### Scenario: Keyboard accessible

- **WHEN** the palette is used without a mouse
- **THEN** navigation and selection work via arrow keys and Enter/Escape

### Requirement: Theme Toggle (Dark/Light)

The desktop shell SHALL support dark-first theming with a light theme toggle
driven by `style/` tokens (ARCHITECTURE.md §5.1; UI.md).

#### Scenario: Toggle persists

- **WHEN** the user toggles the theme
- **THEN** the UI switches via token overrides and the choice persists across
  restarts

#### Scenario: Dark is the default

- **WHEN** the app starts with no stored preference
- **THEN** the dark theme is applied

### Requirement: i18n Scaffolding

The desktop shell SHALL include internationalization scaffolding with English
as the default locale and all UI strings externalized (i18n-ready).

#### Scenario: Strings are not hardcoded

- **WHEN** a UI string is rendered
- **THEN** it resolves from a message catalog rather than an inline literal

#### Scenario: Default locale

- **WHEN** no locale is configured
- **THEN** English is used

### Requirement: CLI (TUI)

The system SHALL provide a `fergents` binary (Ratatui) that connects to
`fergentsd` and provides `fergents status`, `fergents events tail`, and
`fergents brain list` (stub) plus a TUI shell with a placeholder panel
(ARCHITECTURE.md §5.2).

#### Scenario: Status command

- **WHEN** the user runs `fergents status`
- **THEN** the CLI connects and prints the kernel health/state

#### Scenario: Tail events

- **WHEN** the user runs `fergents events tail`
- **THEN** the CLI streams events from the bus until interrupted

#### Scenario: Brain list stub

- **WHEN** the user runs `fergents brain list`
- **THEN** the CLI returns the (empty in Phase 0) list of registered brains

#### Scenario: TUI launches

- **WHEN** the user runs the TUI entry point
- **THEN** a placeholder panel renders and connects to the kernel

### Requirement: Testing Harness

The system SHALL provide unit tests (event bus, protocol codegen, storage), an
end-to-end integration test (start kernel → connect → send command → receive
event), and a minimal desktop smoke test (launch → connect → render)
(ROADMAP.md §4, STACK.md §21).

#### Scenario: Kernel integration test

- **WHEN** the integration test starts a kernel on a temp socket, connects a
  client, sends a command, and subscribes to a topic
- **THEN** it receives the command result and at least one event, and shuts
  down cleanly

#### Scenario: Storage unit tests

- **WHEN** the storage tests run against a temporary SQLite file
- **THEN** migration, CRUD, transaction rollback, and audit chain checks pass

#### Scenario: Desktop smoke test

- **WHEN** the smoke test launches the desktop app (via `tauri-driver` +
  WebdriverIO, or an equivalent harness)
- **THEN** it verifies that the app starts, connects to the kernel, and
  renders the shell
- **AND** full visual-regression testing is deferred to Phase 3

### Requirement: CI Pipeline

The system SHALL provide GitHub Actions workflows `ci.yml` (lint + test +
build, matrixed across Windows, macOS, Linux) and a `release.yml` stub
(STACK.md §18.3).

#### Scenario: CI passes on all OSes

- **WHEN** `ci.yml` runs on a pull request
- **THEN** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`,
  `cargo build`, plus `eslint`, `prettier --check`, `tsc --noEmit`, and the
  pnpm build all pass on Windows, macOS, and Linux

#### Scenario: Release stub is inert

- **WHEN** `release.yml` is inspected
- **THEN** it is a tag-triggered stub that does not publish artifacts yet

## MODIFIED Requirements

None. This is a greenfield change with no prior behavior to modify.

## REMOVED Requirements

None.

## Appendix A: Phase-0 protocol surface

**Command methods (Phase 0)**: `hello`, `ping`, `system.info`,
`system.health`, `events.subscribe`, `events.unsubscribe`, `events.replay`,
`brain.list`, `kernel.shutdown`.

**Event kinds (Phase 0)**: `kernel.started`, `kernel.stopping`,
`kernel.error`, `system.health`, `events.dropped`.

**Envelope (JSON-RPC 2.0)**:

- Request: `{ "jsonrpc": "2.0", "id", "method", "params" }`
- Result: `{ "jsonrpc": "2.0", "id", "result" }`
- Error: `{ "jsonrpc": "2.0", "id", "error": { "code", "message", "data?" } }`
- Event notification: `{ "jsonrpc": "2.0", "method": "event", "params": { <event> } }`

## Appendix B: Deferred to later phases

- Per-brain `index.sqlite` schema (documents, chunks, FTS5, `sqlite-vec`
  embeddings, tags, links) and `graph.db` (CozoDB) — **Phase 1** (reserve
  `migrations/brain/`).
- Brain creation, folder scanning, and vault indexing — **Phase 1**.
- Permission policy engine writes to `audit_log`, permission categories and
  modes — **Phase 2** (Phase 0 only ships the table and chain primitive).
- `llm_requests` writer, cost dashboards — **Phase 2/6** (Phase 0 ships the
  table only).
- Full visual-regression testing (Playwright or equivalent) — **Phase 3**
  (Phase 0 ships a launch/connect/render smoke test only).
- IPC authentication — **Phase 7** (server mode); Phase 0 treats the OS user
  boundary as the trust boundary.

## Appendix C: JSON-RPC error codes

Reserved JSON-RPC range:

| Code | Meaning |
|---|---|
| `-32700` | Parse error |
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |

Kernel-specific range (`-32000` to `-32099`):

| Code | Meaning |
|---|---|
| `-32001` | Protocol version unsupported |
| `-32002` | Handshake required (or timed out) |
| `-32003` | Subscription limit reached |
| `-32004` | Storage error |
| `-32005` | Shutdown in progress |