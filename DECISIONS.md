# Decisions (ADR log)

> The architectural decision record log for Fergents. One entry per
> consequential decision, in the order it was made.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md` §17 (key-decisions summary),
`ROADMAP.md`, `STACK.md`.

Each entry records the decision and the rationale behind it. Decisions are
**append-only**: a superseded decision keeps its number and gains a
`Superseded by ADR-0xx` note rather than being rewritten. ADR-001 … ADR-015
were recorded during Phase 0; ADR-016 … ADR-020 during Phase 1; ADR-021 …
ADR-026 during Phase 2; ADR-027 … ADR-033 during Phase 3.

| #       | Decision                                                    | Status   |
| ------- | ----------------------------------------------------------- | -------- |
| ADR-001 | Kernel as a separate daemon                                 | Accepted |
| ADR-002 | Rust for the kernel                                         | Accepted |
| ADR-003 | Event-sourced with SQLite persistence                       | Accepted |
| ADR-004 | Per-brain SQLite, not global                                | Accepted |
| ADR-005 | MCP as the only external integration surface                | Accepted |
| ADR-006 | WASM (Component Model) for plugins                          | Accepted |
| ADR-007 | React in Shadow DOM for UI plugins                          | Accepted |
| ADR-008 | Markdown + YAML/TOML for all definitions                    | Accepted |
| ADR-009 | LAN-first P2P with libp2p                                   | Accepted |
| ADR-010 | `.fergents` backups + shadow git                            | Accepted |
| ADR-011 | AGPL-3.0                                                    | Superseded by ADR-035 |
| ADR-012 | Tauri v2 for desktop + mobile                               | Accepted |
| ADR-013 | sqlite-vec + BGE-M3 for embeddings                          | Accepted |
| ADR-014 | No telemetry by default                                     | Accepted |
| ADR-015 | AI as a first-class user, no privileged APIs                | Accepted |
| ADR-016 | CodeMirror 6 for the Phase 1 markdown editor                | Accepted |
| ADR-017 | React Flow for Phase 1 graph visualization                  | Accepted |
| ADR-018 | Swappable `VectorStore`/`Reranker`/`EntityExtractor` traits | Accepted |
| ADR-019 | Per-brain FTS5 tokenizer defaults                           | Accepted |
| ADR-020 | Phase 0 gaps fixed before Phase 1 implementation            | Accepted |
| ADR-021 | Kernel-authoritative capability gating                      | Accepted |
| ADR-022 | Sandboxed rich outputs                                      | Accepted |
| ADR-023 | Diff-gated, file-backed `brain.md`                          | Accepted |
| ADR-024 | Persisted spec/plan/tasks approval runs                     | Accepted |
| ADR-025 | Secrets in the OS keychain                                  | Accepted |
| ADR-026 | Compare, fork, and time-travel semantics                    | Accepted |
| ADR-027 | Monaco for project files, CodeMirror for notes              | Accepted |
| ADR-028 | Kernel-authoritative project file I/O                       | Accepted |
| ADR-029 | Project trust gates every process                           | Accepted |
| ADR-030 | One kernel-side LSP client per (project, language)          | Accepted |
| ADR-031 | PTYs live in the kernel                                     | Accepted |
| ADR-032 | git2 (vendored), shadow git, off-by-default remote ops      | Accepted |
| ADR-033 | Inline AI and Composer as gated, reversible runs            | Accepted |
| ADR-034 | Agents and chat run on the configured providers             | Accepted |
| ADR-035 | Relicense to MIT                                            | Accepted |

---

## ADR-001 — Kernel as a separate daemon

**Decision.** The kernel runs as its own process (`fergentsd`), not embedded in
the UI.
**Rationale.** Multi-client, headless, testable, syncable.

## ADR-002 — Rust for the kernel

**Decision.** The kernel is written in Rust.
**Rationale.** Performance, MCP SDK, WASM, Tauri, safety.

## ADR-003 — Event-sourced with SQLite persistence

**Decision.** State changes are expressed as events persisted in SQLite.
**Rationale.** Audit, replay, sync, plugin reactivity.

## ADR-004 — Per-brain SQLite, not global

**Decision.** Each brain owns its own store instead of sharing one global
database.
**Rationale.** Portability, isolation, multi-attach.

## ADR-005 — MCP as the only external integration surface

**Decision.** External integrations go through MCP; no bespoke protocols.
**Rationale.** Interoperability, no bespoke protocols.

## ADR-006 — WASM (Component Model) for plugins

**Decision.** Plugins run as WASM components.
**Rationale.** Security, cross-language, sandboxing.

## ADR-007 — React in Shadow DOM for UI plugins

**Decision.** UI plugins render React inside a Shadow DOM.
**Rationale.** Isolation, reuse of the web ecosystem.

## ADR-008 — Markdown + YAML/TOML for all definitions

**Decision.** Agents, workflows, and configuration are markdown plus
YAML/TOML frontmatter.
**Rationale.** Human-readable, AI-editable, git-friendly.

## ADR-009 — LAN-first P2P with libp2p

**Decision.** Peer sync is LAN-first over libp2p.
**Rationale.** Requirement from the north star; no cloud dependency.

## ADR-010 — `.fergents` backups + shadow git

**Decision.** Reversibility via `.fergents` backups and a shadow git history.
**Rationale.** Reversibility by default.

## ADR-011 — AGPL-3.0

**Decision.** Fergents is licensed AGPL-3.0-or-later.
**Rationale.** Protect the commons, allow self-hosting.

## ADR-012 — Tauri v2 for desktop + mobile

**Decision.** The desktop (and later mobile) shell is Tauri v2.
**Rationale.** Small binaries, Rust backend, single codebase.

## ADR-013 — sqlite-vec + BGE-M3 for embeddings

**Decision.** Vector search uses `sqlite-vec` with BGE-M3 embeddings.
**Rationale.** Embedded, multilingual, offline.

## ADR-014 — No telemetry by default

**Decision.** The kernel sends no telemetry unless the user opts in.
**Rationale.** Privacy-first.

## ADR-015 — AI as a first-class user, no privileged APIs

**Decision.** The AI uses the same APIs and permission system as a human, with
no privileged back doors.
**Rationale.** Consistency, security, auditability.

## ADR-016 — CodeMirror 6 for the Phase 1 markdown editor

**Decision.** The Phase 1 markdown editor is built on CodeMirror 6;
Monaco/LSP is deferred to Phase 3.
**Rationale.** A dedicated, lightweight, extensible markdown editor keeps the
Phase 1 editor small and focused; Monaco's weight and LSP tooling are not
needed until code editing arrives in Phase 3.

## ADR-017 — React Flow for Phase 1 graph visualization

**Decision.** The graph view uses React Flow. A custom WebGL renderer is
deferred and adopted **only if** React Flow degrades below target.
**Rationale.** React Flow covers pan/zoom/drag, layout, and interaction with
far less bespoke code; a custom renderer is a cost worth paying only if a
measured regression demands it.

## ADR-018 — Swappable `VectorStore`/`Reranker`/`EntityExtractor` traits

**Decision.** Embedding, reranking, and NER engines sit behind traits
(`VectorStore`, `Reranker`, `EntityExtractor`) with deterministic stubs used in
tests.
**Rationale.** Keeps the engines swappable and keeps heavy models out of CI
(stub engines only; the real backends are feature-gated).

## ADR-019 — Per-brain FTS5 tokenizer defaults

**Decision.** The per-brain FTS5 index defaults to
`unicode61 remove_diacritics 2` (no Porter stemming), configurable per brain.
**Rationale.** Predictable, language-neutral tokenization for multilingual
vaults; stemming can be opted into per brain rather than imposed.

## ADR-020 — Phase 0 gaps fixed before Phase 1 implementation

**Decision.** The Phase 0 gaps identified by the audit (log retention, true
daemon detachment, `fergentsd stop`, …) were closed as small PRs **before**
Phase 1 implementation began.
**Rationale.** Phase 1 builds directly on the daemon lifecycle and logging, so
closing those gaps first avoided compounding them.

## ADR-021 — Kernel-authoritative capability gating

**Decision.** The kernel owns capability state (`capabilities` table); the UI is
a projection of `capability.list` and can never enable behavior the kernel has
not enabled. A disabled capability allocates nothing — no tasks, watchers,
pools, tools, commands, or events — and its commands return
`capability_disabled`. Optional capabilities default off; core ones default on.
**Rationale.** One authority keeps the UI honest, makes "off" mean genuinely
unallocated, and lets plugins participate through the same registry.

## ADR-022 — Sandboxed rich outputs

**Decision.** Charts (Recharts), Mermaid, and model-generated HTML render inside
`<iframe sandbox="allow-scripts">` with a strict CSP, no `allow-same-origin`, and
no network. Chart specs are validated JSON; markdown HTML is sanitized with an
allowlist. `charts`/`mermaid`/`html` capabilities gate the blocks, which degrade
to code blocks when disabled.
**Rationale.** Model output is untrusted; the frame boundary plus sanitization
contains it without giving up interactive visuals.

## ADR-023 — Diff-gated, file-backed `brain.md`

**Decision.** An agent's self-improving prompt lives at `agents/<id>/brain.md`,
alongside `session.md` for compaction summaries. Edits are diff-gated: the
default mode is **suggest** (user approves), **auto** applies immediately with
an audit entry and a backup, and every change is revertible.
**Rationale.** Plain files stay inspectable, git-versionable, and AI-editable;
gating them preserves user control over the agent's own identity.

## ADR-024 — Persisted spec/plan/tasks approval runs

**Decision.** Spec workflows are first-class persisted runs (`runs`,
`approvals`) with `awaiting_*_approval` states and per-run autonomy
(`supervised`, `batch`, `auto`). Approval reuses the permission-card pattern,
and resuming re-reads the (possibly edited) artifacts on disk.
**Rationale.** Gates must survive restarts, and the user's edits must be the
input the next stage sees.

## ADR-025 — Secrets in the OS keychain

**Decision.** API keys live in the OS keychain via `keyring`; the database
stores only `secret_refs`. Values are resolved at call time inside the provider
layer and never reach agents, prompts, logs, or plugins.
**Rationale.** The local database is backed up, synced, and inspected; keys
must not travel with it.

## ADR-026 — Compare, fork, and time-travel semantics

**Decision.** Multi-model compare is implemented as **parallel sessions** that
the UI renders side by side; forking copies messages up to a turn into a new
session with independent history and cost. Time travel restores session message
state plus brain state at a chosen event via the persisted event log and
per-brain snapshots, without rewinding unrelated projects, and the restore is
itself audited and reversible.
**Rationale.** Reusing sessions keeps everything in the existing model, and
scoping restores to their blast radius keeps them safe.

---

## ADR-027 — Monaco for project files, CodeMirror for notes

**Decision.** Project files open in **Monaco** (lazy-loaded in its own chunk,
with dedicated workers); brain notes keep **CodeMirror 6**. Both editors are
wrapped behind the same kernel-backed adapter surface, so controllers and
tests never touch the editor library directly.
**Rationale.** Monaco brings the LSP-shaped editing model project files need
(diagnostics, symbols, multi-cursor), while CodeMirror keeps the markdown
experience and its plugins. Lazy-loading keeps chat and brain startup budgets
unchanged (Phase 3 decision D1).

## ADR-028 — Kernel-authoritative project file I/O

**Decision.** The webview never reads or writes project files. Every read and
write goes through `file.*` in the kernel. Writes carry `base_etag`; a mismatch
returns `file_conflict` and never overwrites. Mutations are backed up before
they land, so they are reversible.
**Rationale.** The kernel is the only component that can enforce trust,
permissions, path sandboxing, and reversibility consistently. Etags make
out-of-band edits a detectable conflict instead of silent data loss (D2).

## ADR-029 — Project trust gates every process

**Decision.** A newly opened project is **untrusted**. Until `project.trust`
is granted, the kernel refuses LSP servers, terminals, git hooks, Inline AI,
and Composer with `project_untrusted`. Trust is persisted per project,
audited, and revocable; revoking it stops the running processes.
**Rationale.** Opening a folder must not execute code from it. One explicit
gate, enforced in the kernel, covers every process-spawning surface (D3).

## ADR-030 — One kernel-side LSP client per (project, language)

**Decision.** The LSP client lives in the kernel (`crates/lsp`). One client is
spawned per `(project, language)` on demand and shared by all editor tabs.
Servers come from `PATH` or the registry; nothing is auto-installed. Crashes
restart with backoff (max 3 in 5 minutes), then `lsp_server_failed` and a manual
restart; diagnostics from a dead server are marked stale.
**Rationale.** Centralizing LSP keeps behavior identical across tabs and
clients, and makes process lifetime a kernel concern rather than a UI one
(D4, D5).

## ADR-031 — PTYs live in the kernel

**Decision.** Terminals are kernel-owned PTYs (`portable-pty`); xterm.js is a
view over an event stream with a ring buffer. Sessions die with the kernel — no
resurrection. `shell.exec` stays non-interactive, `term.exec` runs in a visible
tab, and `term.read` returns a bounded, redacted tail; all three obey the same
permission and sandbox policies.
**Rationale.** Agents and humans must share one terminal model, and ownership
in the kernel is what makes output readable by the agent without a webview
(D6, D7).

## ADR-032 — git2 (vendored) with a CLI fallback; shadow git never merged

**Decision.** Git operations use `git2` with **vendored libgit2** for hermetic
builds, falling back to the `git` CLI for edge cases. Reads are allowed on a
trusted project; mutations follow the permission policy. Agent/Composer
commits append `Fergents-Agent` and `Fergents-Session` trailers. Remote
operations are a **separate, off-by-default** capability (`git-remote`), and
force-push is never offered.
**Rationale.** Vendoring removes a system dependency from CI and user machines.
Shadow git is a safety net for agent edits, not a branch of the user's history,
and remote access needs its own explicit switch (D8, D9, D10).

## ADR-033 — Inline AI and Composer as gated, reversible runs

**Decision.** Inline AI is **off by default** (per-keystroke token cost),
debounced, cancellable, budgeted, and never auto-inserted; it is suppressed in
untrusted projects and large files. A Composer run is a Phase 2 **run**
(`kind = 'composer'`) whose artifacts are per-file diffs; applying a subset
goes through backup + shadow git and is reversible, and rejecting writes
nothing. The agent's codebase context is **explicitly packaged and bounded**
(open file/selection, fuzzy file list, text search, LSP symbols) rather than
whole-repository embeddings.
**Rationale.** Both surfaces spend tokens without the user asking, so they need
their own switches, budgets, and a diff gate. Bounded context keeps cost and
latency predictable while still answering "what does this code do?" (D11, D12,
D13).

## ADR-034 — Agents and chat run on the configured providers

**Decision.** The agent runtime receives a `RegistryProvider` backed by
`ProviderService`, not a fixed adapter. Each request resolves its model from
the session's pinned `model`, then the `chat` route, then the first model of
the first configured provider; only when no provider exists does it fall back
to the deterministic mock (keeping offline development and tests green).
Agent calls stream (`Provider::chat_stream`), `chat.message.send` republishes
tokens as `chat.message.delta`, and token counts plus cost are persisted on
the session.
**Rationale.** Chat must reflect the provider the user configured; one
resolver keeps request/stream adapters interchangeable, and a strict offline
fallback keeps the kernel usable with no network and no keys.

---

## ADR-035 — Relicense to MIT

**Decision.** Fergents is relicensed from AGPL-3.0-or-later to the **MIT
License**, effective with `v0.4.0-phase4`. The `LICENSE` text (copyright 2026),
the README badge and license section, the contributor terms, every npm
manifest, and the Cargo workspace `license` field now read MIT; the workspace
crates inherit it through `license.workspace = true`. This entry supersedes
**ADR-011**, which stays in the log unchanged as history.

**Rationale.** The AGPL's network-copyleft obligation made embedding Fergents
in closed products, internal appliances, and commercial integrations legally
awkward, which worked against the goal of a broad extension ecosystem. The
platform's safety comes from the runtime — the sandbox, the permission engine,
the capability gates, audit, and reversibility — not from the license, so a
permissive license aligns the legal terms with the architecture. MIT keeps the
project maximally adoptable, keeps contributions simple (no CLA), and does not
change any runtime behaviour or security boundary.

**Consequences.** Dependency audits now accept permissive licenses broadly,
but copyleft dependencies must be avoided in the kernel and SDK; downstream
distributors may ship closed derivatives, which the project accepts. ADR-011
is not rewritten: it remains the record of the original decision.

---

*End of `DECISIONS.md`.*

