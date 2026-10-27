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

**Phase 2 — Agent Runtime & Chat UI**

- Capability registry (`crates/capabilities`): declarative catalogue with
  dependencies, `default_enabled`/`hot_toggle`, plugin registration, persisted
  state, lifecycle events, and a kernel-authoritative gate that refuses the
  commands of disabled capabilities with `capability_disabled` (ADR-021).
- LLM router (`crates/llm`) and providers (`crates/core`): the `Provider` trait
  with mock, OpenAI, Anthropic, Google, and Ollama adapters, per-task routes,
  and token accounting; CI runs entirely against the deterministic mock.
- Secrets (`crates/secrets`): OS keychain storage via `keyring` with only
  `secret_refs` in the database; values never reach agents or logs (ADR-025).
- MCP host (`crates/mcp`): stdio JSON-RPC servers, tool discovery, health
  checks, and `mcp.server.*`/`mcp.tool.list` commands behind `capability.mcp`.
- Agent runtime (`crates/agents`, `crates/core`): markdown agent definitions
  with validation, enable/disable/update, the ReAct loop, sessions and
  messages, memory tiers (working/episodic/semantic) with decay and
  consolidation, compaction into `session.md`, per-agent `brain.md` (ADR-023),
  permission gates, and the spec/plan/tasks approval workflow (ADR-024).
- Sub-agents (`crates/core`): `agent.spawn`, `agent.call`, `agent.children`,
  `agent.collect`, `agent.cancel`, `agent.message`, and `agent.handoff`, with
  per-parent limits and `sub_agents.allow` enforcement.
- Chat backend (`crates/chat`): sessions, transcripts, send/retry/regenerate/
  fork, the prompt library, FTS5 chat search, attachment routing, the context
  builder with `context.inspect`/`context.set_section`, and multi-model
  `chat.compare.start` as parallel sessions (ADR-026).
- Provider manager: `provider.list`/`add`/`remove`/`test`/`models` and
  `provider.route.get`/`set` over the wire.
- Desktop UI (`apps/desktop`): the chat surface (transcript with tool cards and
  hover actions, streaming placeholder, sandboxed rich outputs — ADR-022 — and
  a composer with `@` mentions and `/` prompts), the context viewer, feature
  toggles, provider/secret/MCP managers, the model selector, the compare panel,
  the sub-agent monitor, and time travel over backups.
- Protocol: the Phase 2 command and event surface added to the schema with
  regenerated Rust and TypeScript bindings and round-trip suites.
- Verification: per-crate tests, wired IPC tests for every Phase 2 command, and
  a Phase 2 end-to-end test (create agent → run → tool permission → capability
  toggle → `brain.md` round-trip).

**Phase 3 — IDE & Codebase**

- Project layer (`crates/ide`): open/close/update projects, VCS and
  `.gitignore` detection, a file index, per-project trust with a strict
  `project_untrusted` gate, kernel-authoritative file I/O (`file.read`/`write`/
  `create`/`mkdir`/`rename`/`delete`/`list`/`stat`) with `base_etag` conflict
  detection, and persisted `editor.layout.get`/`set` (ADR-028, ADR-029).
- LSP client (`crates/lsp`): framed JSON-RPC over stdio, one session per
  `(project, language)`, the server registry (`lsp.server.add/remove/list/
  start/stop/restart/status`), document sync, diagnostics, completion, hover,
  definition, references, rename, symbols, format, code actions, and signature
  help. Crashes restart with backoff; `capability.lsp` gates the family
  (ADR-030).
- Terminal (`crates/terminal`): `portable-pty` sessions with a ring buffer,
  `term.open/write/resize/close/list/read` and the agent-facing `term.exec`,
  all gated by `capability.terminal` and project trust (ADR-031).
- Git (`crates/git`): status, diffs, stage/unstage, discard (backed up),
  commit with `Fergents-Agent`/`Fergents-Session` trailers, branches, log,
  show, blame, conflicts/resolve, and shadow-git safety. Remote operations
  (`git.fetch`/`pull`/`push`) sit behind the off-by-default
  `capability.git-remote`; force-push is never offered (ADR-032).
- Composer and Inline AI (`crates/core`): `composer.start/get/apply/reject/
  cancel` with per-file diff artifacts, subset application through backup +
  shadow git, and `inline.complete`/`inline.action`/`inline.cancel` with
  debouncing, budgets, and trust/size suppression (ADR-033).
- Desktop IDE (`apps/desktop`): Monaco (lazy chunk) with tabs, breadcrumb,
  outline, find/replace, and conflict banners; project explorer and search;
  Problems panel; xterm.js terminal with tabs, split, and shell selector;
  Source control panel; diff preview with per-hunk approval; Inline AI ghost
  text; Composer; agent run panel with spec/plan/tasks tabs, approval cards,
  and `@file`/`@selection`; command palette and shortcuts; and Settings → IDE
  (shell, editor defaults, language-server registry, git-remote toggle).
- Generic settings store: ungated `settings.get`/`settings.set` for
  non-secret preferences, shared by Settings and the terminal.
- Verification: protocol round-trip suites for the Phase 3 surface, per-crate
  tests (fake LSP server, temporary repositories, PTY sessions), Vitest suites
  over the editor/terminal/LSP/git adapters, and a Phase 3 end-to-end test
  (open → trust → edit → diagnostics → terminal → spec/plan/tasks approval →
  commit).

- Provider-backed agent runtime (`crates/core`, `crates/agents`): the kernel
  now hands the agent/chat runtime a `RegistryProvider` over
  `ProviderService`, resolving each request from the session's pinned model,
  then the `chat` route, then the first configured provider, and only falling
  back to the offline mock when nothing is configured. Chat turns stream
  (`chat.message.delta`) and persist token counts and cost (ADR-034).
- Provider streaming in the chat UI: `useChat` subscribes to
  `chat.message.delta` and renders the reply as it arrives; new sessions
  default to the provider's default model.
- `chat.session.create` now defaults `model` to the provider registry's
  default model.

**Phase 4 — Skills & plugins**

- Foundation: `capability.plugins` and `capability.skills` (default on),
  `capability.marketplace` (default off), `capability.plugin-factory`
  upgraded to hot-toggleable with dependencies, `skill.install` and
  `skill.invoke` permission categories, the `0015`–`0017` migrations, and
  `required_capability` routing for `plugin.*`, `skill.*`, `factory.*`, and
  `marketplace.*`.
- Packages and sandbox (`crates/packages`, `crates/wasm`): safe zip
  extraction, deterministic directory digests, Ed25519 signature
  verification, semver plus `api_version` N / N−1 compatibility, and a
  wasmtime Component Model host with fuel, epoch, memory, and I/O limits
  behind a single `host.call` dispatcher.
- Plugins (`crates/plugins`): `plugin.toml` parsing and validation, a
  reconciled registry, `inspect` → review → `install` with a permission
  subset, update and uninstall with trash restore, tool registration and
  `plugin.tool.call`, namespaced events, dev linking with debounced reload,
  and plugin-provided capabilities.
- Skills (`crates/skills`): `SKILL.md` parsing and validation, prompt, tool,
  code, and hybrid execution with closed schemas, chaining depth 5 with cycle
  detection, a TTL prompt cache, the registry lifecycle (install, enable,
  disable, remove, duplicate, test), and per-skill agent tools.
- Core, factory, and marketplace (`crates/core`): kernel wiring for the
  sandbox and both services, `factory.scaffold/build/test/package/install/
  report` in per-run workspaces with no auto-install, marketplace sources,
  refresh, search, get, install, and update checks over a signed index, audit
  events for every install/update/uninstall/permission change, and
  `plugin_calls` / `skill_invocations` metrics.
- Agent runtime: the factory commands are exposed as agent tools, so the AI
  can scaffold, build, test, and package a plugin and then hand the user an
  install review.
- Desktop (`apps/desktop`): a Shadow DOM slot host for the 16 slots of
  `UI.md` §22 with per-contribution React roots and N / N−1 compatibility,
  Settings → Plugins (search, enable/disable, reload, uninstall with storage
  choice, detail with stats and diagnostics) and Settings → Skills (list,
  enable, duplicate, delete, JSON test runner, install), a shared install
  review card that applies only the approved permissions, the plugin dev
  panel (scaffold through report), the marketplace browser, and palette
  entries for install, manage, dev panel, and every plugin command. Each
  surface is gated by its kernel capability.
- `packages/plugin-sdk`: `definePlugin` with validation, the slot catalogue
  and `isApiCompatible`, and the typed `PluginBridge`; the shell imports the
  catalogue from the SDK so there is one source of truth.
- Verification: per-crate tests (fake archives and keys, registry
  reconciliation, a fixture component under limits, marketplace fixture
  indexes), Vitest suites for the host, plugins, skills, marketplace, factory
  panel, and palette, and the SDK's own suite.

### Changed

- Relicensed the project from AGPL-3.0-or-later to the **MIT License**: the
  `LICENSE`, the README badge and license section, every `package.json`, and
  the Cargo workspace license now read MIT. The change removes the
  network-copyleft obligation and is intended to encourage broader adoption,
  embedding, and commercial use.
- `DATA_MODEL.md` now documents the per-brain schema alongside the global one.
- `DECISIONS.md` added: the ADR log, recording ADR-001 … ADR-026.
- `ARCHITECTURE.md` §17 and `README.md` updated for the Phase 1 crates,
  commands, and UI.
- `README.md` updated for the Phase 2 crates, commands, and UI, and its status
  now reads Phase 2 delivered.
- `DECISIONS.md` records ADR-027 … ADR-033 (the Phase 3 IDE decisions).
- `ARCHITECTURE.md` gained §18 (the IDE layer); `UI.md` §8.8, `SECURITY.md`
  §22.1, and `STACK.md` §14.5 document the implementation.
- `README.md` updated for the Phase 3 crates, commands, and UI, and its status
  now reads Phase 3 delivered.
- `PLUGINS.md` §5 and §9 now document the shipped `@fergents/plugin-sdk` API
  (`definePlugin`, `SlotProps`, `PluginBridge`) and the shadow-root isolation
  boundary; `README.md` status now reads Phase 4 complete.
