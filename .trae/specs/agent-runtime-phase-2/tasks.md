# Phase 2 — Tasks

Each task is one branch → one PR → green CI → merge (Conventional Commits).
Backend foundation (2.1–2.3) must be merged before any `apps/desktop` work.
Task ids match the plan referenced by the Lead Engineer ("Task 2.1", …).

Legend: **Dep** = depends on · **🔒** = gated on another task merging first.

---

## 2.1 — Capabilities registry (`crates/capabilities`)

**Dep:** — · **Blocks:** everything (the guard is used by all later commands).

**Deliverables**
- New crate `fergents-capabilities` (library).
- `migrations/global/0003_capabilities.sql` (`capabilities` table).
- Kernel wiring: `Kernel` owns a `CapabilityRegistry`.
- API: capability guard on dispatch + `capability.list` / `capability.get` /
  `capability.set` / `capability.register` commands (parsed from JSON; no
  protocol-schema change yet — see 2.11).

**Steps**
1. `Capability` model: `id`, `name`, `version`, `source`, `dependencies`,
   `resources`, `default_enabled`, `hot_toggle`, `config`.
2. `CapabilityRegistry`: register, list, get, enable/disable with **dependency
   DAG** enforcement (`capability_dependency_disabled`), plugin registration
   (`capability_duplicate_id`, `capability_unknown_dependency`).
3. Persistence: seed from declarative registrations on first boot; upsert to
   `capabilities`; `default_enabled` per spec Appendix D.
4. Emit `capability.registered` / `.enabled` / `.disabled` / `.changed`.
5. `is_enabled(id)` + a `require(id)` guard returning a structured
   `capability_disabled` error; wire it into API dispatch.
6. Settings-facing list payload (id, name, version, source, enabled,
   dependencies, `restart_required`).

**Tests**
- Registry: register/get/list; duplicate id rejected; unknown dependency
  rejected.
- Enable/disable: enabling with a disabled dep fails; `auto_enable_deps`
  enables deps; disabling a dep with dependents is rejected or cascades
  (per decision).
- Persistence: state survives a kernel restart; seeding applies defaults.
- Guard: a disabled capability's command returns `capability_disabled`.
- Events: enable/disable emit the right kinds.
- Integration (api): `capability.list` shapes; toggling changes dispatch.

**Acceptance**
- `capability.*` works end-to-end; disabled capability allocates nothing and
  is unlisted for its commands; CI green.

---

## 2.2 — LLM router (`crates/llm`)

**Dep:** 2.1 (guard on `llm` capability). **🔒** none.

**Deliverables**
- New crate `fergents-llm` (library).
- Normalized domain types: `Message`, `Role`, `ContentBlock` (text/image/
  tool-call/tool-result), `ToolDef`, `ToolCall`, `ChatRequest`, `ChatResponse`,
  `StreamFrame`, `Usage`, `ModelInfo`.
- `Provider` trait (`chat`, `chat_stream`, `embed`, `rerank`, `models`,
  `health`) + adapters: `mock` (deterministic, used in CI), `openai`
  (OpenAI-compatible: OpenAI/Ollama/llama.cpp/custom), `anthropic`, `google`.
- `Router`: task → model mapping from TOML, fallback chains, per-request model
  override, budget checks, cost accounting, prompt cache, local cache.
- `CostTable` + `llm_requests`-shaped usage records (written by the caller).
- Errors: `provider`, `rate_limited`, `budget_exceeded`, `model_unavailable`,
  `invalid_request`, `timeout`.

**Steps**
1. Domain types + serde; stable JSON so the protocol layer can reuse them.
2. `Provider` trait (object-safe, async via `async_trait`-free `Box<dyn
   Future>` or `futures`), `ProviderError`.
3. `MockProvider`: scripted responses, deterministic streams, token counting.
4. `OpenAiCompatProvider` (reqwest): chat + stream (SSE), tool calling,
   embeddings, models; base-URL config so it covers Ollama/llama.cpp.
5. `AnthropicProvider`, `GoogleProvider` adapters.
6. `RouterConfig` (TOML) + `Router::route(task, override)`; fallback loop;
   `llm.routed` / `llm.error` events via a sink trait.
7. Budgets (`BudgetCheck`) + `CostTable`; caches (prompt + response LRU).
8. Token counting: `tokenizers`/`tiktoken-rs` wrapper with a deterministic
   estimator fallback (decision D8).

**Tests**
- Mock chat + stream: frames are normalized; usage reported.
- Router: primary success; fallback on error; task mapping; override wins.
- Budgets: rejection before the call when exceeded.
- Tool calling: request/response round-trip normalization.
- Vision: image content block passthrough.
- Cache hit avoids a provider call; cost table math.
- Token counter: monotonic, deterministic, matches budget use.

**Acceptance**
- Chat works fully offline with the mock/Ollama provider; no network in CI;
  CI green.

---

## 2.3 — MCP host (`crates/mcp`)

**Dep:** 2.1. **🔒** none.

**Deliverables**
- New crate `fergents-mcp` (library).
- MCP client/host: server registry, transports (`stdio`, `sse`, `http`), a
  JSON-RPC 2.0 codec matching the MCP spec, initialize/handshake, health.
- Discovery: tools, resources, prompts; call/read/get.
- Permissions hook: every server has a permission policy; `mcp.connect` is
  evaluated on add.
- Minimal MCP **server** exposing configurable handlers (used to expose brain
  tools: search/read/write).
- Events: `mcp.server.connected` / `.disconnected` / `.error`, `mcp.tool.*`.
- Registry integration: discovered tools registered in the kernel tool registry
  so agents can call them.

**Steps**
1. JSON-RPC codec (request/response/notification) + framing for stdio.
2. `Transport` trait + `StdioTransport`, `SseTransport`, `HttpTransport`, and
   an `InMemoryTransport` for tests.
3. `McpClient`: `initialize`, `list_tools`, `call_tool`, `list_resources`,
   `read_resource`, `list_prompts`, `get_prompt`, `ping`.
4. `McpHost`: registry of servers (config: command/url, transport, permissions),
   connect/disconnect, health checks with backoff, reconnect.
5. `McpServer`: minimal server over a `Transport` with pluggable handlers;
   expose brain tools via a handler trait.
6. Wire discovery into the kernel tool registry + events.

**Tests**
- Codec: round-trip request/response/notification; errors.
- Client over `InMemoryTransport`: initialize + list + call + read.
- Host: connect/disconnect/health/reconnect; events emitted; per-server
  permission enforced.
- Minimal server: serves tools/resources/prompts; handles unknown method.
- Stdio transport: spawns a fixture process and completes a handshake.
- Registry integration: discovered tools appear in `list_tools`.

**Acceptance**
- A stub MCP server's tools are discoverable and callable end-to-end offline;
  CI green.

---

## 2.4 — Agent runtime (`crates/agents`)

**Dep:** 2.1, 2.2, 2.3.

- ReAct loop (context builder → planner → executor → reflector → output).
- Tool registry execution with permission checks; streaming; parallel tool
  calls; cancellation; interruption.
- Loop variants: `react` (default), `plan_execute`, `reflexion`, `tot`,
  `debate`, `map_reduce`.
- Stopping conditions + budgets per `AGENTS.md` §13; resumable sessions.
- Agent definitions (`agents/<id>.md`) + schema validation; `agent.*` commands.

**Tests:** loop with mock provider/tools; permission denial path; stopping
conditions; resumption; invalid definition rejected.

---

## 2.5 — Memory

**Dep:** 2.4.

- Working / episodic / semantic tiers (`agent_memory`), embeddings + RAG reuse.
- `brain.md` (`agents/<id>/brain.md`) + `session.md` compaction summary.
- Memory tools: `remember` / `recall` / `forget` / `list`.
- Decay, reinforcement, consolidation; scope (global/agent/project/brain).

**Tests:** store/recall/forget; scope isolation; decay/consolidation; summary
written at the compaction threshold.

---

## 2.6 — Context builder + Context Viewer backend

**Dep:** 2.4, 2.5.

- Named/typed sections; token accounting via the llm counter (D8).
- `context.inspect` / `context.set_section` / `context.set_budget`.
- `context.built` event + `context_snapshots`; `rag` section absent when
  `capability.search` is off.

**Tests:** section token counts; toggling drops a section from the payload;
snapshot persisted; RAG gating.

---

## 2.7 — Spec / Plan / Tasks workflow

**Dep:** 2.4.

- Artifacts under `.fergents/specs/<slug>/`; gates persisted in `runs` /
  `approvals`.
- `awaiting_*_approval` states; `run.approve` / `run.reject`; autonomy modes.
- `spec.start` / `spec.get` / `spec.approve`; resume re-reads edited files.

**Tests:** generate → pause → approve → continue; edit-before-approve; autonomy
modes; artifacts on disk.

---

## 2.8 — Permissions

**Dep:** 2.4.

- Modes (manual/auto/full), categories, TOML policy engine (first match),
  per-agent/per-tool overrides, permission card payload, audit log.

**Tests:** mode behavior; policy precedence; agent overrides never widen; audit
rows written.

---

## 2.9 — Reversibility

**Dep:** 2.4.

- Shadow git + `.fergents` backups; restore per file/session/run; audited.

**Tests:** write creates a backup; restore returns exact prior state.

---

## 2.10 — Chat backend

**Dep:** 2.4.

- `agent_sessions` / `agent_messages` / FTS5; `chat.*` commands; fork;
  regenerate; auto-continue; attachments pipeline; prompt library.

**Tests:** create/list/rename/delete; FTS search; fork copies history; continue
cap; attachment routing degrades gracefully.

---

## 2.11 — Protocol surface + codegen

**Dep:** 2.1–2.10.

- Add Phase 2 commands/events to `protocol/schema/protocol.schema.json`;
  regenerate Rust + TS; freshness test green.

**Tests:** codegen freshness; typed round-trip for a representative command.

---

## 2.12 — Desktop: Settings → Features (capabilities)

**Dep:** 2.1, 2.11. **🔒** backend foundation merged.

- Features page: switch per capability, source/version/dependencies,
  `restart_required`.

**Tests (Vitest):** renders registry; toggle calls the kernel; dependency
feedback.

---

## 2.13 — Desktop: Chat UI

**Dep:** 2.10, 2.11. **🔒** backend foundation merged.

- Claude-style chat, streaming, tool cards, thinking, hover actions (copy/
  regenerate/fork), multiple-choice, attachments, @-mentions, sidebar history
  + prompt library, Improve Input, Explain This, Quick Capture, chat search.

**Tests (Vitest):** message rendering, streaming, tool cards, hover actions,
multiple-choice, search.

---

## 2.14 — Desktop: Context Viewer

**Dep:** 2.6, 2.13. **🔒** backend foundation merged.

- Token bar + per-section checkboxes; toggles call `context.set_section`.

**Tests (Vitest):** counts render; toggle updates the payload.

---

## 2.15 — Desktop: Rich Outputs (charts)

**Dep:** 2.13. Gated by `capability.charts`.

- Sandboxed iframe renderer for chart/mermaid/html; schema validation;
  sanitization; graceful degradation.

**Tests (Vitest):** valid spec renders; invalid spec errors; disabled capability
shows a code block.

---

## 2.16 — Desktop: Agent Builder + Agent list

**Dep:** 2.4, 2.11. **🔒** backend foundation merged.

- Create/configure agents; live frontmatter validation; icon/color; enable/
  disable/duplicate/delete.

**Tests (Vitest):** validation errors inline; save produces a valid definition.

---

## 2.17 — Desktop: Provider Manager + Secrets + MCP Manager

**Dep:** 2.2, 2.3, 2.11.

- Add/remove/test providers; keychain-backed secrets; MCP server add/test/
  health.

**Tests (Vitest):** provider test states; secret list never returns values; MCP
health rendering.

---

## 2.18 — Desktop: Multi-Model UI + compare

**Dep:** 2.2, 2.13.

- Model selector; two parallel sessions side by side.

**Tests (Vitest):** selector pins a model; compare renders two panes.

---

## 2.19 — Multi-agent: Agent Sync, Time Travel, async sub-agents

**Dep:** 2.4.

- Shared session/file collaboration; timeline restore (reversible); background
  sub-agents with monitor/collect.

**Tests:** attribution; restore exact state; async spawn/collect within limits.

---

## 2.20 — Ecosystem extensions (capability-gated)

**Dep:** 2.13.

- Voice (Whisper), Ombra overlay, Agent Web extension; Plugin Factory /
  Workflow Debugger capability hooks (spec Appendix F).

**Tests:** each is off by default; enabling allocates; disabling hides.

---

## 2.21 — Hardening, docs, and release

**Dep:** all.

- Update docs where behavior changed; write ADR-021…ADR-026; end-to-end test
  (create agent → run → tool permission → toggle capability → `brain.md`
  round-trip); tag `v0.2.0-phase2`.

---

## Active now

The Lead Engineer has started the backend foundation: **2.1 → 2.2 → 2.3**,
each merged before the next. No `apps/desktop` work (2.12–2.18, 2.20) until
2.1–2.3 are merged.
