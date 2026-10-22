# Phase 2 — Checklist

Verifiable items per task. A task is done when every box is checked **and** CI
is green on `ubuntu-latest`, `macos-latest`, `windows-latest`.

## Global gates (every PR)

- [ ] Conventional Commit message (`feat:` / `fix:` / `docs:` / `test:` / `refactor:` / `chore:`)
- [ ] Branch → PR against `main`; PR description states scope and tests
- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo test -p fergents-core --test offline` with `FERGENTS_OFFLINE=1`
- [ ] `cargo build --workspace`
- [ ] Node job untouched by Rust-only changes (ESLint / Prettier / `tsc` / `pnpm -r test` / `pnpm build` still green)
- [ ] No new dependency without a justification (spec §Dependencies)
- [ ] No network or API key required by default tests
- [ ] PR merged only after CI is green

## 2.1 — Capabilities registry

- [ ] `crates/capabilities` exists, is a workspace member and a workspace dependency
- [ ] `Capability` model carries id/name/version/source/dependencies/resources/default_enabled/hot_toggle/config
- [ ] `CapabilityRegistry`: register / list / get / enable / disable
- [ ] Duplicate id rejected (`capability_duplicate_id`)
- [ ] Unknown dependency rejected (`capability_unknown_dependency`)
- [ ] Enabling with a disabled dependency rejected (`capability_dependency_disabled`) unless `auto_enable_deps`
- [ ] `migrations/global/0003_capabilities.sql` applies idempotently
- [ ] Registry seeded from declarative registrations with `default_enabled` per spec Appendix D
- [ ] State persists across a kernel restart
- [ ] Events `capability.registered` / `.enabled` / `.disabled` / `.changed` emitted
- [ ] `is_enabled` + `require` guard return a structured `capability_disabled` error
- [ ] API dispatch enforces the guard (a disabled capability's command fails, no side effects)
- [ ] `capability.list` / `.get` / `.set` / `.register` work
- [ ] Test: toggling a capability changes dispatch behavior
- [ ] Test: disabling allocates nothing (no tasks/tools/commands registered for it)

## 2.2 — LLM router

- [ ] `crates/llm` exists, is a workspace member and a workspace dependency
- [ ] Normalized domain types (`Message`, `ContentBlock`, `ToolDef`, `ToolCall`, `ChatRequest`, `ChatResponse`, `StreamFrame`, `Usage`, `ModelInfo`)
- [ ] `Provider` trait: chat / chat_stream / embed / rerank / models / health
- [ ] `MockProvider` is deterministic and used by default tests
- [ ] OpenAI-compatible adapter (base-URL configurable → OpenAI/Ollama/llama.cpp)
- [ ] Anthropic and Google adapters
- [ ] `Router`: task→model map, fallback chain, per-request override
- [ ] Budget check rejects before the provider call (`budget_exceeded`)
- [ ] Cost table + usage records
- [ ] Prompt cache / response cache hit avoids a provider call
- [ ] Token counter is deterministic and shared with the budget (D8)
- [ ] Events `llm.routed` / `llm.error` / `llm.rate_limited` via a sink trait
- [ ] Tests: chat, stream, tool calling, vision passthrough, fallback, budgets, cache, cost
- [ ] No network in CI

## 2.3 — MCP host

- [ ] `crates/mcp` exists, is a workspace member and a workspace dependency
- [ ] JSON-RPC 2.0 codec round-trips request/response/notification and errors
- [ ] `Transport` trait + stdio / SSE / HTTP + in-memory (tests)
- [ ] `McpClient`: initialize, list/call tools, list/read resources, list/get prompts, ping
- [ ] `McpHost`: server registry, connect/disconnect, health with backoff, reconnect
- [ ] Per-server permissions; `mcp.connect` evaluated on add
- [ ] Minimal `McpServer` with pluggable handlers (exposes brain tools)
- [ ] Discovered tools registered in the kernel tool registry
- [ ] Events `mcp.server.connected` / `.disconnected` / `.error`, `mcp.tool.*`
- [ ] Tests: codec, in-memory client, host lifecycle/events, minimal server, stdio fixture, registry integration
- [ ] Works fully offline

## 2.4–2.21 (later tasks)

- [ ] 2.4 Agent runtime: ReAct loop, tools + permissions, stopping conditions, resumption, agent definitions
- [ ] 2.5 Memory: episodic/semantic, `brain.md`, `session.md`, decay/consolidation
- [ ] 2.6 Context builder + viewer backend: sections, token accounting, toggles, snapshots, RAG gating
- [ ] 2.7 Spec/plan/tasks workflow: artifacts, gates, autonomy, resume
- [ ] 2.8 Permissions: modes, policy engine, overrides, audit
- [ ] 2.9 Reversibility: shadow git + `.fergents` backups, restore
- [ ] 2.10 Chat backend: sessions/messages/FTS, fork, regenerate, auto-continue, attachments, prompt library
- [ ] 2.11 Protocol + codegen: Phase 2 commands/events, freshness green
- [ ] 2.12 Settings → Features page
- [ ] 2.13 Chat UI (streaming, tool cards, hover actions, multiple-choice, attachments, improve/explain/capture, search)
- [ ] 2.14 Context Viewer UI
- [ ] 2.15 Rich outputs (sandboxed, gated)
- [ ] 2.16 Agent Builder UI
- [ ] 2.17 Provider / Secrets / MCP managers
- [ ] 2.18 Multi-Model UI + compare
- [ ] 2.19 Agent Sync, Time Travel, async sub-agents
- [ ] 2.20 Ecosystem extensions (capability-gated)
- [ ] 2.21 Hardening, docs, ADRs, end-to-end test, tag `v0.2.0-phase2`

## Cross-cutting (spec requirements)

- [ ] Disabled capability ⇒ no tasks/watchers/pools, no tools/commands/events, UI hidden (ADR-021)
- [ ] Charts/Mermaid/HTML only behind capabilities, in a sandboxed iframe, schema-validated, sanitized (ADR-022)
- [ ] `brain.md` updates diff-gated, `suggest` by default, audited, size-capped, secret-scrubbed, revertible (ADR-023)
- [ ] Approval gates persisted and resumable; autonomy per run (ADR-024)
- [ ] Context token counts match kernel budgeting (ADR-025)
- [ ] Secrets only in the OS keychain; DB stores references (ADR-026)
- [ ] Offline: chat + agents + memory + context + spec workflow with a local model
