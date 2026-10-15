# Roadmap

> Phases, milestones, acceptance criteria, and risks for building Fergents.

**Audience:** the architect, the AI developer, early contributors.
**Companion docs:** `ARCHITECTURE.md`, `STACK.md`, `DECISIONS.md`.

---

## Table of contents

1. [How to read this roadmap](#1-how-to-read-this-roadmap)
2. [Guiding principles](#2-guiding-principles)
3. [Phase overview](#3-phase-overview)
4. [Phase 0 — Foundations](#4-phase-0--foundations)
5. [Phase 1 — Brain](#5-phase-1--brain)
6. [Phase 2 — Agent & chat](#6-phase-2--agent--chat)
7. [Phase 3 — IDE](#7-phase-3--ide)
8. [Phase 4 — Skills & plugins](#8-phase-4--skills--plugins)
9. [Phase 5 — Workflow](#9-phase-5--workflow)
10. [Phase 6 — Dashboard & observability](#10-phase-6--dashboard--observability)
11. [Phase 7 — Sync & mobile](#11-phase-7--sync--mobile)
12. [Phase 8 — Workflow→app, marketplace, server](#12-phase-8--workflowapp-marketplace-server)
13. [Milestones & acceptance criteria](#13-milestones--acceptance-criteria)
14. [Risk register](#14-risk-register)
15. [Budget & resources](#15-budget--resources)
16. [Definition of Done](#16-definition-of-done)
17. [Change management](#17-change-management)

---

## 1. How to read this roadmap

- **Phases** are sequential but overlap slightly.
- Each phase has: goal, deliverables, acceptance criteria, risks, and
  estimated duration.
- Durations are estimates for **one focused developer + one AI assistant**.
- Every deliverable is testable. If it isn't, it's a goal, not a deliverable.
- The roadmap is a living document. Changes are recorded in `DECISIONS.md`.

**MVP** = end of Phase 3. **v1** = end of Phase 6. **v2** = end of Phase 8.

---

## 2. Guiding principles

1. **Vertical slices over horizontal layers.** Each phase delivers something
   usable end-to-end, not a partial layer.
2. **Dogfood from day one.** Every phase must be usable by the author.
3. **Ship early, ship often.** Nightly builds from Phase 1 onward.
4. **Reversibility first.** Every feature that mutates state has an undo path.
5. **No feature without tests.** Unit + integration + (if UI) visual.
6. **Documentation is a deliverable.** Every phase updates `README`,
   `ARCHITECTURE`, and `CHANGELOG`.

---

## 3. Phase overview

| Phase | Goal | Duration | MVP? | v1? |
|---|---|---|---|---|
| 0 | Foundations: monorepo, kernel, event bus, Tauri shell, CLI shell | 1–2 mo | | |
| 1 | Brain: markdown vault, backlinks, indexer, hybrid search | 2–3 mo | | |
| 2 | Agent & chat: LLM router, MCP host, agent runtime, chat UI | 2–3 mo | ✓ | |
| 3 | IDE: Monaco, LSP, terminal, git | 3–4 mo | ✓ | |
| 4 | Skills & plugins: WASM host, plugin SDK, skill runtime | 3–4 mo | | |
| 5 | Workflow: DAG engine, visual editor, triggers | 3–4 mo | | ✓ |
| 6 | Dashboard & observability: prebuilt + custom widgets | 2–3 mo | | ✓ |
| 7 | Sync & mobile: LAN, P2P, Android companion | 3+ mo | | |
| 8 | Workflow→app, marketplace, self-hosted server | ongoing | | |

**Cumulative estimates:**
- MVP (end of Phase 3): **~8–12 months**
- v1 (end of Phase 6): **~18–24 months**
- v2 (end of Phase 8): **~24–36 months**

---

## 4. Phase 0 — Foundations

**Goal:** a monorepo that builds, a kernel that runs, a desktop shell and a
CLI that connect to it, an event bus that persists, and the first CI
pipeline. No features yet — just the skeleton.

**Duration:** 1–2 months.

### Deliverables

**Repository**
- Monorepo with Cargo workspace + pnpm workspace.
- `.devcontainer/` with all toolchains.
- `README.md`, `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
- `.editorconfig`, `rustfmt.toml`, `.eslintrc`, `.prettierrc`.
- `rust-toolchain.toml`, `pnpm-workspace.yaml`.
- GitHub Actions: `ci.yml` (lint + test + build), `release.yml` (stub).

**Kernel (`fergentsd`)**
- Crate structure: `core`, `storage`, `events`, `protocol`, `api`,
  `supervisor`.
- Event bus with SQLite persistence.
- Registry (tools, providers, brains, sessions).
- Local socket server (JSON-RPC 2.0 over named pipe / Unix socket).
- CLI entry point: `fergentsd start`, `fergentsd stop`, `fergentsd status`.
- Health endpoint, structured logs (`tracing`), graceful shutdown.

**Protocol**
- JSON Schema definitions for commands and events.
- Codegen: Rust types + TypeScript types.
- Version negotiation on handshake.

**Desktop shell (Tauri)**
- Tauri v2 app that spawns `fergentsd` if absent, connects, and renders a
  placeholder UI using `/style/` tokens.
- Command palette (Ctrl+K) with stub commands.
- Theme toggle (dark/light).
- i18n scaffolding.

**CLI (TUI)**
- `fergents` binary that connects to `fergentsd`.
- `fergents status`, `fergents events tail`, `fergents brain list` (stub).
- TUI shell with a placeholder panel.

**Storage**
- SQLite initialization (global DB).
- Migration framework (`migrations/global/0001_initial.sql`).
- `Storage` trait + `SqliteStorage` implementation.

**Testing**
- Unit tests for event bus, protocol codegen, storage.
- Integration test: start kernel, connect client, send command, receive
  event.
- Visual regression harness (Playwright) set up but with minimal tests.

### Acceptance criteria

- [ ] `cargo build --release` succeeds on Windows, macOS, Linux.
- [ ] `pnpm build` succeeds.
- [ ] `fergentsd start` runs; `fergents status` shows healthy.
- [ ] Desktop app launches, connects to kernel, renders the shell.
- [ ] CLI launches, connects, tails events.
- [ ] Killing the desktop app does not kill the kernel.
- [ ] Killing the kernel makes clients reconnect automatically.
- [ ] CI passes on all three OSes.
- [ ] A trivial command round-trips in <50 ms.
- [ ] Events persist across kernel restarts.
- [ ] Documentation: `README`, `ARCHITECTURE`, `STACK`, `DATA_MODEL`,
  `ROADMAP` are in place.

### Risks

- **Rust build times on Windows.** Mitigation: `sccache`, incremental
  builds, split crates.
- **Tauri v2 maturity.** Mitigation: track upstream, use stable APIs only.
- **Cross-platform socket differences.** Mitigation: abstract behind an
  `IpcTransport` trait; test on all three.

---

## 5. Phase 1 — Brain

**Goal:** a working second brain with markdown files, backlinks, tags,
daily notes, full-text search, semantic search, and a graph view. The user
can create, edit, link, and search notes — entirely offline.

**Duration:** 2–3 months.

### Deliverables

**Vault & documents**
- Markdown files on disk (source of truth).
- SQLite index (documents, notes_meta, tags, links).
- File watcher (notify crate) → incremental reindex.
- Soft delete + restore.
- Attachments (images, PDFs) with thumbnails.

**Editor**
- Monaco in markdown mode (or a dedicated markdown editor, decided in
  Phase 1).
- Wiki links `[[...]]`, tag `#tag`, block refs `^id`, embeds `![[...]]`.
- Live preview.
- Backlinks panel.
- Outline panel.
- Daily notes, templates.

**Search**
- FTS5 (BM25) working.
- sqlite-vec + BGE-M3 embeddings working.
- Hybrid search with RRF.
- Reranker (BGE-reranker-v2-m3).
- Filters (path, tag, kind, date).
- Search UI with keyboard navigation.

**Graph**
- CozoDB graph store.
- Entity extraction (regex + NER + optional LLM).
- Backlink graph (relational).
- Graph view (React Flow or custom canvas) with force layout.
- Filters and search in graph.

**Multi-brain**
- Brain creation from templates (blank, research, coding, personal).
- Brain attach/detach to projects.
- Brain duplicate, move, export/import (`.fergbrain`).
- Per-brain settings.

**UI**
- Brain sidebar (files, tags, backlinks, graph, search).
- Editor area with tabs.
- Graph view.
- Settings page for brain.

### Acceptance criteria

- [ ] Create a brain, add 100 notes, search returns relevant results.
- [ ] Backlinks update within 1 s of an edit.
- [ ] Hybrid search returns better results than BM25 alone on a benchmark
  set of 50 queries.
- [ ] Graph view renders 1000 nodes at 60 fps.
- [ ] Duplicating a brain produces an independent copy.
- [ ] Importing a `.fergbrain` archive works.
- [ ] All operations work offline.
- [ ] Documentation: `BRAIN.md`, `SEARCH.md`, `GRAPH.md` written.

### Risks

- **Chunking quality.** Mitigation: benchmark on real notes early; iterate.
- **Embedding speed on CPU.** Mitigation: batch, cache, allow GPU.
- **Graph layout performance.** Mitigation: use a WebGL renderer
  (Sigma.js or custom) for large graphs.

---

## 6. Phase 2 — Agent & chat

**Goal:** a chat-first interface where users talk to an agent that can read
and write the brain, call tools, use MCP servers, and run autonomously
within permissions. This is the MVP.

**Duration:** 2–3 months.

### Deliverables

**LLM router**
- `Provider` trait + implementations: OpenAI, Anthropic, Google, Ollama,
  llama.cpp, custom OpenAI-compatible.
- Task-based routing (TOML config).
- Streaming (SSE, token-by-token).
- Tool calling (normalized across providers).
- Vision (images).
- Cost tracking, budget enforcement.
- Prompt cache + local cache.

**MCP host**
- `rmcp` integration.
- stdio, SSE, HTTP transports.
- Server registry, health checks, permissions.
- Tool discovery and registration in the kernel registry.
- Sampling, roots.

**MCP server (basic)**
- Expose brain tools (search, read, write note) to external clients.

**Agent runtime**
- ReAct loop with planner/executor/reflector.
- Context manager (compaction, summarization, RAG injection).
- Tool call execution with permission checks.
- Sub-agents (spawn, budget, messaging).
- Persistent sessions, episodic memory.
- Agent definitions as `.md` (frontmatter + system prompt).
- Built-in tools: file read/write, brain search, brain write, web fetch,
  shell (sandboxed), MCP tool call.

**Chat UI**
- Message bubbles (user, assistant, tool, system).
- Streaming tokens.
- Thinking collapsible.
- Tool-call inline with args and result.
- Diff preview for file edits, with approve/reject.
- Permission bar (manual / auto / full).
- Attachments (files, images).
- Session list, search, rename, delete.
- Multi-model: parallel chats, model switcher.

**Permissions**
- Permission categories, rules engine, prompt flow.
- Policy file (TOML).
- Audit log for every decision.

**Reversibility**
- Shadow git per project.
- `.fergents` backups for out-of-repo files.
- Restore UI (per-file, per-session, per-run).

### Acceptance criteria

- [ ] User can chat with an agent, stream tokens, and see tool calls.
- [ ] Agent can read and write brain notes, with permission prompts.
- [ ] Agent can call an MCP server and use its tools.
- [ ] Agent can spawn a sub-agent for a subtask.
- [ ] Every file modification is reversible.
- [ ] Cost is tracked; budget alerts fire.
- [ ] Permission modes work as specified.
- [ ] Audit log records every action.
- [ ] Documentation: `AGENTS.md`, `LLM.md`, `MCP.md`, `SECURITY.md`.

### Risks

- **Provider API drift.** Mitigation: normalization layer, feature flags,
  contract tests.
- **Agent loop fragility.** Mitigation: budgets, retries, circuit breakers,
  pause-and-ask.
- **Cost overruns.** Mitigation: hard budget stops, routing, caching,
  local fallbacks.

---

## 7. Phase 3 — IDE

**Goal:** a full coding IDE integrated with the agent. This completes the
MVP.

**Duration:** 3–4 months.

### Deliverables

**Editor**
- Monaco integration.
- File tree (project + brain).
- Tabs (open files, preview, pinned).
- Breadcrumb.
- Multi-cursor, search/replace, go-to-definition.

**LSP**
- LSP client in the kernel.
- Language servers: rust-analyzer, typescript-language-server, pyright,
  gopls, clangd, lua-language-server.
- Diagnostics, hover, completions, refactorings.
- LSP server management UI.

**Terminal**
- xterm.js + portable-pty.
- Multiple tabs, shell selection.
- Agent can read terminal output and run commands (sandboxed).

**Git**
- Status, diff, stage, commit, branch, log, blame.
- Shadow git integration (agent commits auto-labeled).
- Conflict resolution UI (basic).

**Agent in the IDE**
- Inline AI (ghost text, quick actions).
- Composer (multi-file edit).
- Agent mode (spec.md → plan.md → tasks.md → execute).
- Diff preview with per-file stats (lines added/removed).
- "Free the agent" toggle (autonomy within permissions).

**Split view**
- Chat and IDE side by side, resizable.
- Chat can reference the current file/selection.

### Acceptance criteria

- [ ] Open a Rust/TS/Python project, edit, save, and see diagnostics.
- [ ] Run a terminal command, see output, agent can read it.
- [ ] Agent edits a file, diff shown, user approves, commit recorded.
- [ ] Go-to-definition works across the project.
- [ ] Inline AI suggests completions that respect the project context.
- [ ] Spec/plan/tasks workflow produces usable artifacts.
- [ ] Documentation: `UI.md`.

---

## 8. Phase 4 — Skills & plugins

**Goal:** a plugin and skill ecosystem where users (and the AI) can extend
Fergents without touching the kernel.

**Duration:** 3–4 months.

### Deliverables

**Skill runtime**
- SKILL.md parser and validator.
- Skill kinds: prompt, tool, code, hybrid.
- WASM sandbox (wasmtime, Component Model).
- Permissions and manifests.
- Skill registry, install/uninstall, enable/disable.
- AI can create, install, and configure skills.

**Plugin host**
- WASM host with host functions (fs, net, llm, mcp, storage, events).
- UI plugin host (React in Shadow DOM).
- Plugin manifest, permissions, signature verification.
- Installation from local, git, marketplace.
- UI slots and command palette contributions.
- Plugin dev mode with hot reload.

**Plugin SDK**
- Rust SDK (crate).
- TypeScript SDK (npm package).
- Templates and examples.
- Documentation site.

**Skill/plugin UI**
- Marketplace browser (basic).
- Install/configure flow with permission review.
- Plugin settings page.
- Plugin state management.

### Acceptance criteria

- [ ] A plugin can register a new tool and a new UI panel.
- [ ] A skill can be installed, run, and removed.
- [ ] The AI can create a plugin and install it.
- [ ] The AI can create a skill and use it.
- [ ] Permissions are enforced and auditable.
- [ ] Plugin isolation works (a malicious plugin can't read the FS).
- [ ] Documentation: `SKILLS.md`, `PLUGINS.md`.

---

## 9. Phase 5 — Workflow

**Goal:** a visual workflow engine, integrated with agents, MCP, brain, and
the event bus. Triggers include cron, webhooks, file watch, events, and MCP.

**Duration:** 3–4 months.

### Deliverables

**Engine**
- DAG execution with typed nodes.
- Node types: llm, tool, agent, code, http, condition, loop, subworkflow,
  human, trigger, transform.
- Persistence of runs and steps.
- Retry, timeout, checkpointing, compensation.
- Concurrency control, rate limits.

**Triggers**
- Cron, webhook, file watch, event, MCP, manual, AI.

**Editor**
- Visual DAG editor (React Flow).
- Node inspector with schema-driven forms.
- Debugger: breakpoints, step, inspect, replay.
- Run history with traces.
- Versioning of workflows.

**YAML**
- Workflows defined in YAML, round-trippable with the visual editor.
- Schema validation, autocompletion.

**Node SDK**
- Plugins can register new node types.
- Node types can be written in TypeScript or Rust (compiled to WASM).

### Acceptance criteria

- [ ] Build a workflow visually, run it, see the trace.
- [ ] Trigger via cron and via webhook.
- [ ] A workflow can call an agent, which calls a tool, which calls MCP.
- [ ] Replay a failed run from a step.
- [ ] Plugins can add custom node types.
- [ ] Documentation: `WORKFLOW.md`.

---

## 10. Phase 6 — Dashboard & observability

**Goal:** full observability with prebuilt and custom dashboards.

**Duration:** 2–3 months.

### Deliverables

**Prebuilt dashboards**
- Costs (per provider, model, agent, day/week/month).
- Tokens (input, output, cached).
- Runs (agents, workflows).
- Errors (grouped, filterable).
- Agents (activity, limits, status).
- Brains (index size, growth, search quality).

**Custom widgets**
- YAML widget definitions.
- Data sources: SQL (brain/global), MCP, plugin, chat, event stream.
- Widget types: metric, chart, table, list, timeline, gauge, custom (React).
- Drag-and-drop grid.
- Widget sharing (export/import).

**Observability**
- OpenTelemetry spans (local).
- Metrics endpoint (Prometheus format on localhost).
- Audit log viewer.
- Run replay.

### Acceptance criteria

- [ ] Create a custom widget in YAML, see it render.
- [ ] Widget pulls data from an MCP server.
- [ ] Widget pulls data from a plugin.
- [ ] Cost dashboard matches actual API invoices.
- [ ] Audit log is queryable and exportable.
- [ ] Documentation: `DASHBOARD.md`.

---

## 11. Phase 7 — Sync & mobile

**Goal:** multi-device sync, LAN-first P2P, and an Android companion app.

**Duration:** 3+ months.

### Deliverables

**Sync**
- mDNS discovery on LAN.
- libp2p transport (QUIC, Noise, Yamux).
- Device pairing (QR code).
- Automerge CRDTs for markdown.
- Event log merge for structured data.
- Conflict detection and resolution UI.
- Per-brain, per-project sync scopes.
- Optional relay for WAN.

**Mobile (Android companion)**
- Tauri Mobile build.
- Chat with agents.
- View brains (read/write notes).
- Monitor runs and dashboards.
- Respond to permission requests.
- Notifications.
- Local cache, offline mode.

**Server mode (preview)**
- `fergentsd --server` headless.
- HTTP API for mobile and external clients.
- Optional relay for P2P.

### Acceptance criteria

- [ ] Two desktops on the same Wi-Fi sync a brain in <5 s.
- [ ] Edits made offline merge without data loss.
- [ ] Android app connects to desktop over LAN.
- [ ] Chat and notes work offline on mobile, sync when reconnected.
- [ ] Documentation: `SYNC.md`.

---

## 12. Phase 8 — Workflow→app, marketplace, server

**Goal:** the killer feature (workflow→native app), a plugin/skill
marketplace, and a production-ready self-hosted server.

**Duration:** ongoing.

### Deliverables

**Workflow→app**
- A workflow can be "published" as a mini-app.
- UI manifest (YAML) describing the app's UI.
- Isolated context, sandboxed.
- Icon in the sidebar.
- Can call agents, brains, MCP, other workflows.
- Versioning and rollback.

**Marketplace**
- Plugin and skill marketplace.
- Ratings, reviews, verification badges.
- Payments (optional, TBD).
- Moderation pipeline.

**Server**
- Production-grade self-hosted server.
- Multi-user, roles, quotas.
- Managed backups, migrations.
- Helm chart / Docker Compose.

**Polish**
- Onboarding flow.
- Templates gallery.
- Importers (Obsidian, Notion, Logseq).
- Exporters.
- Accessibility audit.
- Performance pass.

### Acceptance criteria

- [ ] A workflow can be published as an app and used by another user.
- [ ] Marketplace lists, installs, and updates plugins/skills.
- [ ] Self-hosted server supports 10 users on a single VM.
- [ ] Importers migrate an Obsidian vault with <5% data loss.

---

## 13. Milestones & acceptance criteria

| Milestone | End of phase | Definition |
|---|---|---|
| **M0 — Skeleton** | Phase 0 | Kernel + shell + CLI + event bus + CI |
| **M1 — Brain** | Phase 1 | Notes, backlinks, search, graph |
| **M2 — MVP** | Phase 2 | Chat, agents, MCP, permissions, audit |
| **M3 — IDE** | Phase 3 | Monaco, LSP, terminal, git, agent in IDE |
| **M4 — Extensible** | Phase 4 | Skills + plugins + SDK |
| **M5 — Automation** | Phase 5 | Workflows + visual editor + triggers |
| **M6 — Observable** | Phase 6 | Dashboards + custom widgets |
| **M7 — Multi-device** | Phase 7 | Sync + P2P + mobile |
| **M8 — Platform** | Phase 8 | Workflow→app + marketplace + server |

---

## 14. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Rust complexity slows development | High | Medium | AI writes, compiler reviews; modular crates; incremental builds |
| R2 | MCP spec changes | Medium | High | Pin spec version; adapt incrementally; contribute upstream |
| R3 | Provider API drift | High | Medium | Normalization layer; contract tests; feature flags |
| R4 | Agent quality insufficient | Medium | High | Invest in context management, RAG, tool design; benchmarks |
| R5 | Cost overruns | Medium | High | Routing, caching, local models, hard budgets |
| R6 | Sync conflicts cause data loss | Medium | Critical | CRDTs, shadow git, backups, extensive tests |
| R7 | Plugin security breach | Low | Critical | WASM sandbox, permissions, signatures, audits |
| R8 | Scope creep | High | High | Strict phase gates; MVP discipline; ADRs |
| R9 | Single-developer bottleneck | High | High | AI assistance; documentation; templates |
| R10 | Cross-platform issues | Medium | Medium | CI matrix; devcontainers; early testing |
| R11 | Webview inconsistencies | Medium | Low | Visual regression tests; avoid exotic features |
| R12 | SQLite corruption | Low | Critical | WAL, backups, integrity checks, shadow git |
| R13 | Embedding model too slow on CPU | Medium | Medium | Batching, caching, optional GPU, smaller fallback |
| R14 | Graph performance at scale | Medium | Medium | WebGL renderer, sampling, pagination |
| R15 | Legal / license issues | Low | High | AGPL from day 1; audit dependencies; SPDX headers |

---

## 15. Budget & resources

### Development

- **Team**: 1 developer (you) + 1 AI assistant + occasional feedback (father).
- **Time**: 8+ hours/day when possible; realistic pace is 4–6 focused hours.
- **Tooling**: free (Rust, Node, Tauri, GitHub Actions free tier).
- **Hardware**: a decent machine with 16+ GB RAM, SSD. GPU optional.
- **Soft envelope**: a modest monthly envelope covers LLM API calls, small
  hosting, and any paid tooling during development. It is a soft ceiling,
  not a target, and it is defined by the user — not hardcoded in Fergents.

### LLM API costs

Fergents never assumes a budget. The user defines their own limits in
settings, and the kernel enforces them.

**Strategy:**

- Default to local models (Ollama, llama.cpp) for simple tasks.
- Route complex tasks to paid models only when needed.
- Cache aggressively (local + provider-side).
- Hard budget stops per day / week / month, configurable per provider,
  per agent, and globally.
- Track every request; surface real costs in the dashboard.
- Alerts at soft thresholds; hard stops at the configured ceiling.

### Infrastructure (later)

- **GitHub**: free for public repos; Actions minutes for private.
- **Domain**: ~€10/year.
- **Code signing**: Windows (EV cert ~€300/year, optional initially),
  macOS (Apple Developer $99/year, required for notarization).
- **Server (future)**: small VPS for the self-hosted option.

### Non-budget items

- **Legal**: AGPL-3.0, no incorporation needed for open source.
- **Trademark**: optional; consider registering "Fergents" if the project
  gains traction.

---

## 16. Definition of Done

A deliverable is "done" only when:

- [ ] Code is written, reviewed by the compiler/CI, and merged.
- [ ] Unit tests cover the new code (target: 70% kernel, 60% UI).
- [ ] Integration tests cover the new flow.
- [ ] Visual regression tests pass (if UI).
- [ ] Documentation is updated (`README`, `ARCHITECTURE`, phase-specific
  doc, `CHANGELOG`).
- [ ] No new `clippy` warnings, no `eslint` errors.
- [ ] Accessibility checks pass (axe-core, keyboard nav).
- [ ] Performance is within budget (no regressions >10% on benchmarks).
- [ ] The feature is usable end-to-end by the author.
- [ ] Rollback path exists (if it mutates state).
- [ ] ADR written (if it's a non-trivial decision).

---

## 17. Change management

- **ADRs**: every non-trivial decision gets an ADR in `DECISIONS.md`, with
  context, options, decision, consequences.
- **Roadmap changes**: recorded at the bottom of this file with date and
  reason.
- **Phase gates**: no phase starts until the previous phase's acceptance
  criteria are met (exceptions require an ADR).
- **Post-mortems**: after each phase, a short retrospective in
  `RETROSPECTIVES.md`.

---

## Change log

| Date | Change | Reason |
|---|---|---|
| (initial) | Roadmap created | — |

---

*End of `ROADMAP.md`.*