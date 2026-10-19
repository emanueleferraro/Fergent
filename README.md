# Fergents

Fergents is a local-first, open-source agentic workspace that unifies a coding
agent, a second brain, a workflow engine, an MCP hub, a plugin runtime, and a
multi-agent orchestrator into a single, extensible desktop + CLI application.
Everything runs on the user's device by default; the cloud is used only for
LLM API calls, and only when providers are configured.

**Version:** 0.1.0-draft
**License:** AGPL-3.0-or-later
**Status:** Phase 1 (Brain) — in progress

---

## Phase 0 status

Phase 0 (see `ROADMAP.md` §4) builds the skeleton everything else depends on:
a monorepo that builds, a kernel daemon that runs and persists events, a
desktop shell and a CLI that connect to it, and the first CI pipeline. No
product features exist yet.

Delivered in Phase 0:

- Cargo workspace (`crates/`) and pnpm workspace (`packages/`, `apps/`).
- Kernel daemon `fergentsd`: JSON-RPC 2.0 server over a local socket, SQLite
  event bus, runtime registry (tools, providers, brains, sessions), structured
  logging, health check, graceful shutdown.
- Protocol contract in `protocol/` (JSON Schema) with committed Rust and
  TypeScript codegen and handshake version negotiation.
- Storage layer: global SQLite schema, numbered migration framework, and the
  `Storage` trait with a `SqliteStorage` implementation.
- Desktop shell (Tauri v2 + React 19) that spawns `fergentsd` as a sidecar and
  renders a placeholder UI using `style/` tokens.
- CLI (TUI, Ratatui) `fergents` with `status`, `events tail`, and `brain list`.
- GitHub Actions CI (`ci.yml`) and an inert `release.yml` stub.

Explicitly deferred to later phases: the LLM router, MCP host/server, agent
runtime, Monaco/LSP, the WASM plugin host, the workflow engine, sync, and
mobile.

---

## Phase 1 status

Phase 1 (see `ROADMAP.md` §5) turns the skeleton into a working second brain:
a markdown vault per brain, indexed for hybrid search, with an entity graph, a
desktop UI, and a kernel wired to it end to end.

Delivered in Phase 1:

- **Brain model and lifecycle** (`crates/brain`): `brain.toml`, vault layout,
  and the full lifecycle — create from a template (blank, personal, research,
  coding, team), open, close, update, archive/unarchive, delete, duplicate,
  move, `.fergbrain` export/import, attach/detach with roles, and
  `brain.template.save`.
- **Index and search** (`crates/index`): per-brain SQLite index (documents,
  chunks, FTS5, `sqlite-vec` embeddings, tags, links), chunking, daily notes,
  and templates, and hybrid search (BM25 + vectors + sparse fusion) behind
  swappable engines (ADR-018).
- **Entity graph** (`crates/graph`): a CozoDB Datalog graph with a SQLite
  mirror, regex and (feature-gated) ONNX entity extraction, and
  `graph.query`/`graph.algo`/`graph.neighbors`/`graph.subgraph`.
- **Kernel wiring and CLI** (`crates/core`, `crates/api`, `crates/fergents`):
  the Phase 1 commands dispatched over the JSON-RPC socket and the
  `fergents brain …` and `fergents search …` CLI commands.
- **Desktop UI** (`apps/desktop`): brain sidebar and explorer, markdown editor
  with live preview (CodeMirror 6, KaTeX, Mermaid, callouts, wiki links), a
  search panel, a React Flow graph view, per-brain settings, and a command
  palette.
- **Verification**: performance benchmarks behind the `bench` feature and an
  offline test (`FERGENTS_OFFLINE=1`); the heavy models are never downloaded in
  CI (deterministic stubs only).

---

## Repository layout

```
crates/        Cargo workspace (Rust)
  core/          event bus, registry, scheduler, permissions
  storage/       SQLite pools, migrations, Storage trait
  events/        event types and bus
  protocol/      protocol types + generated bindings
  api/           local socket server (JSON-RPC 2.0)
  supervisor/    health, restarts, crash recovery
  markdown/      markdown flavor, frontmatter, and link/tag extraction
  brain/         brain model, vault layout, lifecycle, multi-brain ops
  index/         per-brain index: chunking, embeddings, FTS5, hybrid search
  graph/         entity graph (CozoDB) with a SQLite mirror
  fergentsd/     kernel daemon binary
  fergents/      CLI (TUI) binary
packages/      pnpm workspace packages (protocol types, UI)
apps/          pnpm workspace applications (desktop shell)
protocol/      protocol contract sources (JSON Schema) — the source of truth
style/         design system (tokens + components), mandatory for all UI
migrations/    numbered SQL migrations (global/, brain/ reserved for Phase 1)
```

The Cargo workspace is defined by the root `Cargo.toml`; the pnpm workspace is
defined by `pnpm-workspace.yaml`.

---

## Build and run

### Kernel daemon (`fergentsd`)

```sh
cargo build --release          # builds all crates and both binaries

./target/release/fergentsd start             # start as a background daemon
./target/release/fergentsd start --foreground # run in the current process
./target/release/fergentsd status            # report running (PID) / not running
./target/release/fergentsd stop              # terminate the daemon
```

The daemon binds a local socket (`\\.\pipe\fergentsd` on Windows,
`$XDG_RUNTIME_DIR/fergentsd.sock` or `~/.fergents/fergentsd.sock` on Unix) and
persists state under `~/.fergents/`.

### CLI (`fergents`)

```sh
cargo run -p fergents -- status          # connect and print kernel health/state
cargo run -p fergents -- events tail     # stream events until interrupted

cargo run -p fergents -- brain list              # list the registered brains
cargo run -p fergents -- brain create "Personal" # create a brain (--template, --path)
cargo run -p fergents -- brain open <brain-id>   # open a brain by id

cargo run -p fergents -- search "hybrid query"   # hybrid search (--top-k, --mode, --brain)
```

The `brain` and `search` commands require a running kernel; start one with
`fergentsd start` first.

### Workspace build (TypeScript)

```sh
pnpm install
pnpm build
```

### Desktop shell (Tauri v2)

The desktop app lives in `apps/desktop` and is a **standalone** Cargo workspace
(`apps/desktop/src-tauri`) because its Tauri build script requires the
`fergentsd` **sidecar** to be staged for the host target triple. Build it like:

```sh
cargo build -p fergentsd                     # from the repo root
cp target/debug/fergentsd \
   apps/desktop/src-tauri/binaries/fergentsd-$(rustc -vV | sed -n 's/^host: //p')

cd apps/desktop
pnpm install
pnpm dev                                     # Vite dev server only
pnpm tauri dev                               # full desktop app (spawns the sidecar)
```

On Windows the sidecar file is `fergentsd-<triple>.exe`. The app spawns the
kernel detached (via `fergentsd start`), so closing the app leaves the kernel
running.

---

## Development environment

The reference development environment is **WSL2 (Ubuntu)**, and a
`.devcontainer/` is provided and recommended for reproducibility: it provisions
Rust (stable, with `clippy` and `rustfmt`), Node LTS, and pnpm. VS Code can
open the repository directly in the container. See `CONTRIBUTING.md` for the
full setup and the quality gates.

---

## Documentation

The entry point is this README. The full documentation set includes
`ARCHITECTURE.md`, `DECISIONS.md` (the ADR log), `STACK.md`, `ROADMAP.md`,
`DATA_MODEL.md`, `SECURITY.md`, and the runtime/data/UX docs, plus the design
system in `style/`.

---

## License

Fergents is licensed under the **GNU Affero General Public License, version 3
or later** (AGPL-3.0-or-later). You are free to use, study, modify, and
distribute Fergents, provided that any modified version — including one offered
as a network service — is also released under the same license. See `LICENSE`
for the full text.
