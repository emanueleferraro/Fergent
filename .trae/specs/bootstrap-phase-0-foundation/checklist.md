# Checklist

Verification points for the Phase 0 foundation. Each item maps to a spec
requirement. Check only when the referenced evidence exists and passes.

> Status legend: `[x]` verified locally (evidence run/present); `[ ]` not yet
> verified in this environment — see the note and the "Remaining blockers"
> section at the bottom.

## Repository & toolchain

- [x] Root `Cargo.toml` defines a workspace; library crates `crates/{core,storage,events,protocol,api,supervisor}` and binary crates `crates/fergentsd` / `crates/fergents` exist (crate name matches binary name).
- [x] `pnpm-workspace.yaml`, `packages/{protocol,ui}`, and `apps/desktop` exist; pnpm scripts build.
- [x] `rust-toolchain.toml`, `.editorconfig`, `rustfmt.toml`, `.eslintrc`, `.prettierrc`, `tsconfig.json` (strict) are present.
- [x] `README.md`, `LICENSE` (AGPL-3.0), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` exist.
- [ ] `.devcontainer/` provisions Rust + Node toolchains; `cargo build` and `pnpm install && pnpm build` succeed **inside** the devcontainer. — Files present (`.devcontainer/Dockerfile`, `devcontainer.json`) but not built here (no Docker in this environment).
- [x] `cargo build --release` succeeds and `pnpm install && pnpm build` succeeds from a clean checkout on the host. — Verified on the host (WSL2 Ubuntu); `pnpm build` and `cargo build --release` both exit 0.
- [ ] The repository is initialized with git, remote points to GitHub, default branch is `main`, and `main` is protected (require PR, require CI). — `git init` done, branch `main`, **no commit yet**; GitHub remote + branch protection pending `gh auth login`.

## Protocol & codegen

- [x] JSON Schema for the Phase-0 commands and events exists under `protocol/`.
- [x] Rust types are generated via `typify` into `crates/protocol` and TypeScript types via `json-schema-to-typescript` into `packages/protocol`; generated output is committed.
- [x] A codegen-freshness test fails when generated output is stale. — `generated_rust_is_fresh` (Rust) and `committed src/generated.ts is fresh` (TS).
- [x] JSON-RPC 2.0 envelope serde round-trip tests pass.
- [x] `hello`/`welcome` version negotiation accepts compatible clients and rejects unsupported major versions with `-32001`.
- [x] The full error-code table from spec Appendix C is implemented and tested.

## Storage & schema

- [x] `Storage` trait is defined with `get`, `put`, `delete`, `query`, `transaction`.
- [x] `SqliteStorage` opens WAL mode with foreign keys enabled on every connection; a pool of N read connections plus a single writer task via mpsc.
- [x] `migrations/global/0001_initial.sql` creates `schema_version`, `events`, `audit_log`, `sessions`, `brains`, `projects`, `project_brains`, `llm_requests`, `settings` with the spec's columns and indexes.
- [x] The migration runner applies `0001_initial.sql` and records the version; re-opening is idempotent.
- [x] Opening a store with a newer schema is refused with a clear error.
- [x] Transaction rollback leaves no partial writes (test passing).
- [x] Audit log append and hash-chain verification pass; genesis entry uses all-zero `prev_hash`; tamper detection reports the first mismatch.

## Event bus

- [x] `Event` type fields and monotonic ULID ids match the spec.
- [x] Published events are persisted in `events` and broadcast to matching subscribers.
- [x] Replay-from-id returns all matching events with a greater id.
- [x] Correlation ordering is preserved.
- [x] Slow subscribers trigger an `events.dropped` notice.
- [x] Events persist across a kernel restart (test passing).

## Kernel, registry & health

- [x] Kernel `core` wires the event bus, storage, and registries on a Tokio runtime.
- [x] Brains and sessions are persisted; tools/providers are registered in-memory.
- [x] Phase 0 ships an **empty** `brains` table; `brain.list` returns `[]` (no error); brain creation is deferred to Phase 1.
- [x] Sessions without `closed_at` are reconciled as closed on startup.
- [x] `brain.list`, `system.info`, and the `system.health` command return correct results.
- [x] `ping` returns `pong` regardless of internal state (liveness only).
- [x] Lifecycle events `kernel.started`, `kernel.stopping`, `kernel.error` are emitted. — `kernel.started`/`kernel.stopping` on the normal path; `kernel.error` on a fatal accept-loop error (`Kernel::announce_error`, `is_fatal_accept`), covered by `lifecycle_error_event_is_persisted`.

## IPC & server

- [ ] `IpcTransport` is implemented for Windows named pipe and Unix socket. — Both code paths present; exercised on Unix only. The Windows named-pipe branch is `cfg`-gated and is not compiled/run in this Linux environment.
- [x] The `api` server performs handshake with a **5 s timeout** (`-32002` on timeout), assigns per-client sessions, dispatches commands, and pushes subscribed events.
- [x] Unknown method / malformed params return JSON-RPC error objects (no crash). — `unknown_method_returns_rpc_error` asserts `-32601`.
- [ ] A trivial command round-trips in under 50 ms locally. — No explicit latency assertion; not measured here.
- [ ] Two concurrent clients each get independent subscriptions. — Single-client subscription is tested (`subscription_delivers_events_live`); the two-client case is not asserted.

## Daemon lifecycle

- [x] `fergentsd start` (background, default) and `fergentsd start --foreground` both work.
- [x] Single-instance enforcement: a second `fergentsd start` while one is running exits non-zero with a clear message (`another instance is running (PID <pid>)`).
- [x] The PID file is written to `~/.fergents/fergentsd.pid` and removed on graceful shutdown.
- [x] `fergentsd status` reports healthy with PID; a stale PID file is removed and reported as `not running`.
- [x] `fergentsd stop` sends SIGTERM (Unix) / console close (Windows), waits up to 10 s, and reports the result. — SIGTERM handling is tested (`graceful_shutdown_on_sigterm`); `stop` also issues `kernel.shutdown` over IPC.
- [x] Graceful shutdown flushes storage, closes the socket, removes the PID file, and exits 0.
- [x] `tracing` JSON logs are written to `~/.fergents/logs/` with daily rotation and 7-day retention.

## Desktop shell & design system

- [x] `fergentsd` is declared as a Tauri **sidecar** (`externalBin`) and resolved through Tauri's sidecar API.
- [x] The app spawns `fergentsd` when absent and connects over the socket.
- [x] Closing the app does not kill the kernel.
- [x] The shell uses only `style/` tokens/components (include order `tokens.css` → `typography.css` → `components.css`); no hardcoded colors/spacing/radii/motion.
- [x] Any added component follows `patterns.html`; any new token is added to `tokens.json` + `tokens.css` first.
- [x] Ctrl+K palette opens, filters, executes stub commands, and is keyboard-accessible.
- [x] Dark is the default theme; the light toggle works and persists across restarts.
- [x] All UI strings resolve from the i18n catalog; English is the default locale.

## CLI

- [x] `fergents status` prints kernel health/state.
- [x] `fergents events tail` streams events until interrupted.
- [x] `fergents brain list` returns the (empty in Phase 0) registered brains without error.
- [x] The Ratatui TUI launches with a placeholder panel and connects.

## Testing & CI

- [x] End-to-end integration test (start kernel → connect → command → event → shutdown) passes.
- [ ] A minimal desktop smoke test (launch → connect → render) runs via `tauri-driver` + WebdriverIO or equivalent. Full visual regression is deferred to Phase 3. — A Vitest render smoke (`App.test.tsx`) exists; `tauri-driver` is not wired up.
- [ ] `ci.yml` passes on Windows, macOS, and Linux (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build`, `eslint`, `prettier --check`, `tsc --noEmit`, pnpm build). — Workflow defined and green locally on Linux; the GitHub matrix has not run (no remote yet).
- [x] `release.yml` exists as an inert tag-triggered stub.

## Documentation

- [x] `DATA_MODEL.md` documents the global schema consistent with `0001_initial.sql` and notes the per-brain schema as deferred to Phase 1.
- [x] `README.md` includes build/run instructions.
- [x] `CHANGELOG.md` has the Phase 0 entry.

## Constraints (global)

- [x] No task introduced an unjustified dependency.
- [x] No global mutable state was introduced.
- [ ] Conventional Commits used in every commit. — No commits exist yet; blocked on the repository step above.
- [x] No permissions/downloads/rollbacks performed outside the approved task scope.
- [x] The IPC trust boundary (OS user) is documented; no authentication is added in Phase 0.

## Remaining blockers

1. **Git commit + GitHub remote + `main` protection** — requires `gh auth login`. No commit has been created yet; therefore Conventional Commits is unverified and CI has not run on the GitHub matrix.
2. **`.devcontainer` build** — not executed (no Docker in this environment).
3. **Windows named-pipe transport** — code path present but not compiled/run on this Linux host.
4. **`tauri-driver` desktop smoke** — replaced for now by a Vitest render smoke.
