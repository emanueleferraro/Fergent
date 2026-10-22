# Agent Runtime & Chat UI — Phase 2 Spec

## Why

Phase 1 (`v0.1.0-phase1`) delivered the **brain**: a markdown vault as the
source of truth, a per-brain index, hybrid search, a graph, and multi-brain
management — all offline. What is missing is the thing the brain was built
for: an **agent** to reason over it, and a **chat** to talk to that agent.

Phase 2 (`ROADMAP.md` §6) turns Fergents from a knowledge base into a
chat-first, agent-native workspace: an LLM router over multiple providers, an
MCP host, a ReAct agent runtime with memory and sub-agents, a
permission-governed tool loop, reversibility for every mutation, and the
Claude-style chat UI that surfaces all of it.

`BACKLOG.md` adds one cross-cutting requirement on top of `ROADMAP.md` §6:
**everything must be modular**. Every feature SHALL be a registered,
kernel-level **Capability** that can be switched off, and disabling it SHALL
stop the kernel from allocating resources for it — not merely hide a button.
This spec delivers exactly `ROADMAP.md` §6 **plus** the `BACKLOG.md` Phase 2
list, and nothing beyond it.

## What Changes

This change is **additive** to the Phase 1 brain. Phase 0/1 code is touched
only where Phase 2 strictly requires it (the global schema, the protocol
surface, the registry, and the settings page).

### 0. Modular architecture — Capabilities (cross-cutting)

- **New crate `crates/capabilities`**: a kernel-authoritative Capability
  registry. Every feature (chat, search, graph, voice, charts, mcp, memory,
  self-improve, spec-workflow, compare, …) is a **Capability** with an id,
  version, dependencies, declared resources, and an enabled/disabled state.
- **Kernel-level gating**: a disabled capability's subsystem tasks are never
  started, its tools/commands/events are never registered, its DB
  pools/watchers/indexes are never allocated, and its commands return a
  structured `capability_disabled` error. The UI is a **projection**: it hides
  entry points for disabled capabilities and never receives their events.
- **Plugin registration**: plugins and custom features can register optional
  capabilities at load time (`capability.register`), gated by the same
  permission model as everything else.
- **Settings → Features page**: a switch for every capability, with its
  dependency graph, source (kernel/plugin), and a `restart_required` marker
  when a toggle cannot be applied hot.

### 1. Chat interface (minimalist & powerful)

- Centered, Claude-style chat layout (`UI.md` §7), built only from `style/`.
- File / image / PDF attachments via drag-and-drop, paste, and `@` mentions.
- Message hover actions: **Copy**, **Regenerate**, **Fork** (visual branching).
- Sidebar: chat history (searchable) + a **prompt library**.
- Composer with an **Improve Input** action (magic wand).
- **Multiple-choice questions**: A/B/C option buttons + an "Other" field,
  driven by a structured tool call.
- **Rich Outputs**: AI-generated **charts** rendered safely (capability-gated).
- **Explain This**: select text or an error → quick explanation in a popup.
- **Quick Capture**: a global shortcut that sends a prompt on the fly.
- **Chat Search**: full-text search within chat history.

### 2. Agent runtime & memory

- **LLM router**: `Provider` trait + OpenAI, Anthropic, Google, Ollama,
  llama.cpp, and custom OpenAI-compatible; task-based routing; streaming;
  normalized tool calling; vision; cost tracking; budgets; caches (`LLM.md`).
- **MCP host** (and a minimal MCP server exposing brain tools) (`MCP.md`).
- **Agent runtime**: ReAct loop with planner/executor/reflector, context
  manager (compaction + summarization + RAG injection), permission-checked
  tool execution, persistent sessions, episodic/semantic memory (`AGENTS.md`).
- **Agent definitions as files** (`agents/<id>.md`) + an **Agent Builder UI**.
- **Permission bar** (manual / auto / full) and the full policy engine.
- **`brain.md`** per-agent memory and **`session.md`** compaction summary.
- **Self-Improving Prompt**: the agent proposes (and, in auto mode, applies)
  updates to its own `brain.md`, always diffed, audited, and reversible.
- **Context Viewer**: a panel with per-section token counts and checkboxes to
  toggle injected sections (Data, Capabilities, RAG, History, …).
- **Spec / Plan / Tasks workflow**: for complex coding tasks the agent
  generates `spec.md` → `plan.md` → `tasks.md` and **pauses for human
  approval** at each gate before executing.
- **Auto-Continue**: on truncation, the kernel automatically sends "continue",
  up to a user-configurable limit.

### 3. Multi-agent & orchestration

- **Agent Sync**: two or more agents collaborate on one session/file with
  visible interleaved changes.
- **Time Travel**: a timeline bar in the chat to return to a previous message,
  restore the exact agent + brain state at that point, and re-explore a path
  (combined with **Fork**).
- **Asynchronous sub-agents**: a manager spawns background sub-agents,
  continues working, and collects results when ready.

### 4. Providers & integrations

- **Multi-Model UI**: a model selector in the chat; two parallel sessions on
  different models, compared side by side.
- **Provider Manager**: add/remove/test API keys (OpenAI, Anthropic, Ollama,
  …).
- **MCP Manager**: add/remove/test/monitor MCP servers and health.
- **Secret Vault**: secret storage in the OS keychain; only references are
  persisted (never plaintext).

### 5. Ecosystem & extensions (capability-gated)

- **Ombra**: a global hotkey overlay.
- **Voice Mode**: Whisper-based voice input/output (capability `voice`).
- **Agent Web**: a browser extension letting the agent read web pages.
- **Plugin Factory** / **Workflow Debugger**: gated behind their substrates
  (see Appendix F for the Phase 2 delivery boundary).

### Foundation

- **Reversibility**: shadow git per project + `.fergents` backups, with a
  restore UI (per-file, per-session, per-run) (`ARCHITECTURE.md` §10, §15).
- **Global schema**: new `agent_*`, `runs`, `approvals`, `capabilities`,
  `chat_*`, and `context_snapshots` tables (`migrations/global/00NN_*.sql`).
- **Protocol**: new Phase 2 commands and events, with Rust + TypeScript codegen
  through the existing pipeline.

No **BREAKING** changes: all Phase 0/1 commands and events keep working; the
Phase 1 brain service is unchanged except for the capability check on
`search`/`graph` commands.

## Impact

- **Affected docs (source of truth)**: `AGENTS.md` §5–§19; `LLM.md` §2–§18;
  `MCP.md` §2–§16; `UI.md` §2, §3, §7, §12, §13, §14, §16, §22; `SECURITY.md`
  §3–§5, §11, §14–§16, §20; `ARCHITECTURE.md` §9, §11, §12, §14, §15;
  `STACK.md` §7, §8, §9, §13; `ROADMAP.md` §6; `DECISIONS.md`; `style/`
  (MANDATORY design system).
- **Affected code (new)**: `crates/{capabilities,llm,mcp,agents}`;
  `migrations/global/0003_*.sql` … (agent/chat/run/capability schema);
  desktop features under `apps/desktop/src/features/{chat,chat-context,
  agents,agent-builder,providers,mcp,secrets,capabilities,settings}`;
  a `render/` sandbox package for rich outputs.
- **Affected code (modified, minimal)**: `crates/core` (capability registry +
  service registration), `crates/api` (dispatch new commands + capability
  guard), `crates/storage` (global migrations), `crates/fergents` (CLI chat /
  agent / model commands), `protocol/schema/protocol.schema.json` (extended),
  `packages/protocol` (regenerated).
- **Out of scope (later phases)**: Monaco/LSP/terminal/IDE (Phase 3); skills &
  plugin runtime + marketplace (Phase 4); workflow engine + visual editor +
  workflow debugger (Phase 5); dashboard (Phase 6); sync & mobile (Phase 7).
  Where a `BACKLOG.md` item depends on a later substrate, Phase 2 ships the
  capability hooks/plumbing and defers the runtime (Appendix F).

## Decisions (resolved)

Normative for Phase 2.

| # | Topic | Decision |
|---|---|---|
| D1 | Capability authority | The **kernel** is the source of truth for capability state. The UI is a projection of `capability.list`; it never enables behavior the kernel has not enabled. |
| D2 | Capability storage | Persisted in the global `capabilities` table, seeded by declarative kernel registrations and extended by plugin registrations. Dependencies are a DAG; enabling a capability whose dependency is disabled is rejected (`capability.dependency_disabled`) unless `auto_enable_deps` is set. |
| D3 | Disabled means unallocated | A disabled capability allocates **nothing**: no Tokio tasks, no watchers, no pools, no registered tools/commands/events. Toggling is hot when the subsystem supports it, else it reports `restart_required`. |
| D4 | Chart renderer | **Recharts**, rendered inside a sandboxed `<iframe sandbox="allow-scripts">` (no `allow-same-origin`) with a strict CSP and no network. Chart specs are JSON validated against a schema before rendering; never raw `innerHTML`. |
| D5 | Other rich output | Mermaid and sanitized HTML also render in the sandboxed frame. All markdown/model HTML is sanitized (DOMPurify, allowlist). Capability `charts` gates chart blocks; `mermaid`/`html` blocks degrade to code blocks when disabled. |
| D6 | `brain.md` | Per-agent, file-backed (`agents/<id>/brain.md`), diff-gated. Default mode is **suggest** (user approves); **auto** applies immediately with an audit entry + shadow-git/backup. Always revertible. |
| D7 | Spec/Plan/Tasks gates | First-class, persisted run states (`runs`, `approvals`). Approval reuses the permission-card component pattern (`UI.md` §12). Autonomy is per-run: `supervised` (approve each gate) \| `batch` (approve spec+plan once) \| `auto`. |
| D8 | Token accounting | The Context Viewer reports the **same** counts the kernel uses to budget. Counts come from the provider tokenizer when available, else a model-aware local estimator; the estimator is deterministic and testable. |
| D9 | Multi-model compare | Implemented as **parallel sessions** (not intra-message branches); the UI renders them side by side and the user picks or merges a winner. |
| D10 | Secrets | OS keychain via `keyring`; the global DB stores only references (`secret_refs`). Keys are resolved at call time and never reach agents/plugins (`LLM.md` §16, `SECURITY.md` §14). |
| D11 | Chat search | FTS5 in the global store over message content, mirroring Phase 1's FTS approach (D11 of Phase 1). |
| D12 | Time Travel scope | Restores **session message state + brain state at that event** via the persisted event log + per-brain snapshots; it does **not** rewind unrelated projects. A restore is itself an audited, reversible action. |
| D13 | Fork semantics | Forking copies messages up to a chosen turn into a new session (new `session_id`, shared agent, independent history and cost). The UI draws the branch relationship. |
| D14 | Provider crates | Providers behind the `Provider` trait (`LLM.md` §2); a deterministic mock provider is used in tests; no real network or API key is required by CI. |
| D15 | Capability defaults | Capabilities that exist because of Phase 0/1 (`search`, `graph`) default **on**; expensive/optional ones (`voice`, `charts`, `compare`, `agent-web`, `plugin-factory`, `workflow-debugger`, `self-improve`) default **off**. |

## ADDED Requirements

### Requirement: Capability Registry (kernel-authoritative)

The kernel SHALL maintain a registry of **Capabilities** in the new crate
`crates/capabilities` and persist it in the global `capabilities` table. A
capability SHALL declare: `id` (namespaced, e.g. `capability.charts`), `name`,
`version`, `source` (`kernel` \| `plugin:<id>`), `dependencies` (ids),
`resources` (declared allocations: tasks, watchers, pools, registrations, UI
slots), `default_enabled`, `hot_toggle`, and `config` schema.

#### Scenario: Every feature is a capability

- **WHEN** the kernel boots
- **THEN** each Phase 2 feature registers a capability (chat, llm, mcp, agents,
  memory, self-improve, spec-workflow, compare, voice, charts, mermaid, html,
  capture, explain, chat-search, agent-sync, time-travel, sub-agents, ombra,
  agent-web, plugin-factory, workflow-debugger) with its dependencies

#### Scenario: Disabling stops allocation

- **WHEN** a capability is disabled
- **THEN** the kernel starts no task/watcher/pool for it, does not register its
  tools/commands/events, and returns `capability_disabled` for its commands —
  the UI receives no events and shows no entry point for it

#### Scenario: Dependencies are enforced

- **WHEN** a client enables `capability.charts`
- **THEN** the kernel checks `capability.chat` (a dependency); if it is
  disabled, the enable is rejected with `capability.dependency_disabled`
  unless `auto_enable_deps` is set, in which case dependencies are enabled
  first

#### Scenario: Plugin capabilities

- **WHEN** a plugin registers a capability (`capability.register`)
- **THEN** it is validated (unique id, known dependencies), persisted with
  `source = plugin:<id>`, listed in Settings → Features, and gated by the same
  permission model

#### Scenario: Hot vs restart

- **WHEN** a capability is toggled
- **THEN** the kernel applies it hot if it declares `hot_toggle`, otherwise it
  persists the new state and returns `restart_required`

#### Scenario: State is observable

- **WHEN** capability state changes
- **THEN** `capability.registered` / `capability.enabled` /
  `capability.disabled` / `capability.changed` events are emitted, and
  `capability.list` returns the full registry with state

### Requirement: LLM Provider Layer and Router

The kernel SHALL implement the `LLM.md` provider layer: a `Provider` trait
(`chat`, `chat_stream`, `embed`, `rerank`, `models`, `health`) with adapters
for OpenAI, Anthropic, Google Gemini, Mistral, Ollama, llama.cpp, and custom
OpenAI-compatible endpoints; a task→model **router** configured in TOML with
fallback chains; normalized streaming; normalized tool calling; vision input;
cost tracking into `llm_requests`; budgets; and local/prompt caches.

#### Scenario: Streaming is normalized

- **WHEN** any provider streams
- **THEN** the kernel emits the normalized SSE frames of `LLM.md` §5.1
  (`start`, `token`, `tool_call`, `usage`, `stop`) over the protocol

#### Scenario: Task-based routing with fallback

- **WHEN** a request for `task = code.edit` is made and the primary model fails
- **THEN** the router tries the configured fallbacks in order and records
  `llm.routed` / `llm.error` events

#### Scenario: Budgets are enforced before the call

- **WHEN** a call would exceed a per-run, per-provider, or global budget
- **THEN** the call is rejected with a clear error and the agent adapts
  (cheaper model/fallback/stop), per `LLM.md` §13

#### Scenario: No network in CI

- **WHEN** the test suite runs
- **THEN** a deterministic mock `Provider` is used; no API key or network is
  required (decision D14)

### Requirement: MCP Host and Minimal MCP Server

The kernel SHALL act as an **MCP host** (`MCP.md` §2): connect to MCP servers
over stdio/SSE/HTTP, maintain a server registry with health and per-server
permissions, and expose discovered tools/resources/prompts to agents. It SHALL
also expose a **minimal MCP server** (`MCP.md` §3) with brain tools (search,
read, write) to external clients.

#### Scenario: Discovered tools are callable

- **WHEN** an MCP server connects
- **THEN** its tools are registered in the kernel registry, listed by
  `mcp.tool.list`, and callable via `mcp.call` and by agents (`mcp.call` tool)

#### Scenario: Health is monitored

- **WHEN** a server is registered
- **THEN** the hub performs health checks and emits
  `mcp.server.connected` / `mcp.server.disconnected` / `mcp.server.error`

#### Scenario: Server connections are permissioned

- **WHEN** an agent or the AI adds a new MCP server
- **THEN** the `mcp.connect` category is evaluated (prompt for new servers in
  Auto mode) and the decision is audited

### Requirement: Agent Runtime (ReAct loop)

The kernel SHALL implement the `AGENTS.md` §5 agent loop: context builder →
planner (optional) → executor (LLM + tool calls with permission checks) →
reflector → output, with streaming, parallel tool calls, cancellation, and
interruption (pause-and-ask).

#### Scenario: Tool loop with streaming

- **WHEN** an agent run executes a tool
- **THEN** tokens stream to the client, the tool call is shown inline with
  status and duration, and the loop continues until a final answer or a
  stopping condition

#### Scenario: Stopping conditions

- **WHEN** a budget (tokens/cost/time/tool calls/iterations) is exhausted, the
  user cancels, or a fatal error occurs
- **THEN** the run stops, the reason is recorded, and the user is notified

#### Scenario: Loop variants are selectable

- **WHEN** an agent definition sets `loop: react | plan_execute | reflexion |
  tot | debate | map_reduce`
- **THEN** the runtime uses that pattern (default `react`)

#### Scenario: Resumable

- **WHEN** a run is paused (budget/user/permission)
- **THEN** its full state is persisted and it can be resumed exactly from the
  UI or via `run.resume`

### Requirement: Agent Definitions and Agent Builder UI

Agent definitions SHALL remain **files** (`agents/<id>.md`, YAML frontmatter +
system prompt, `AGENTS.md` §4) validated against `schemas/agent.schema.json`.
The desktop SHALL provide an **Agent Builder UI** to create/configure agents
(identity, model, tools, skills, MCP, permissions, limits, memory, triggers,
icon/color) with live frontmatter validation, without hand-editing the file.

#### Scenario: Create an agent visually

- **WHEN** the user builds an agent in the Agent Builder and saves
- **THEN** a valid `agents/<id>.md` is written, the agent is registered, and
  `agent.created` is emitted

#### Scenario: The AI can create agents

- **WHEN** the AI calls `agent.create`
- **THEN** the definition is validated, written, registered, and subject to the
  same permissions

#### Scenario: Invalid definitions are rejected

- **WHEN** a definition fails schema validation
- **THEN** the error is surfaced inline and nothing is written

### Requirement: Chat Sessions, Messages, and Streaming UI

The desktop SHALL provide the `UI.md` §7 chat: user/assistant/tool/system/error
message types, live token streaming with a stop affordance, collapsible
thinking, inline tool-call cards, and a session header (session · agent ·
model). Sessions SHALL be persisted (`agent_sessions`, `agent_messages`),
listed, searchable, renameable, deletable, and exportable.

#### Scenario: Stream and stop

- **WHEN** the agent runs
- **THEN** tokens render as they arrive and a stop button cancels the run;
  scrolling follows unless the user scrolls up

#### Scenario: Tool cards and thinking

- **WHEN** the model emits a tool call or thinking
- **THEN** the tool call renders as a card (icon, name, args, status, duration,
  result preview) and thinking renders collapsed by default and is excluded
  from the conversation context

#### Scenario: Hover actions

- **WHEN** the user hovers a message
- **THEN** **Copy**, **Regenerate**, and **Fork** are available, and Fork
  creates a new session branch (decision D13) with a visible branch affordance

#### Scenario: Multiple-choice questions

- **WHEN** the agent asks a question with options
- **THEN** the UI renders A/B/C buttons plus an "Other" free-text field, and
  the answer is returned to the run as a normal user turn

### Requirement: Attachments (file, image, PDF)

The composer SHALL accept drag-and-drop, paste, and `@`-mention attachments
(files, images, PDFs) and SHALL route them through the `LLM.md` §8 multimodal
pipeline (images to vision models; PDFs text-extracted + optionally rendered;
audio transcribed when `voice` is enabled).

#### Scenario: Image to a vision model

- **WHEN** the user attaches an image and the routed model supports vision
- **THEN** it is passed as an image content block and can be referenced in the
  response

#### Scenario: PDF handling

- **WHEN** the user attaches a PDF
- **THEN** text is extracted (`brain.import`) and passed to the model

#### Scenario: Unsupported modality degrades

- **WHEN** the routed model lacks the needed modality
- **THEN** the kernel adapts (route to a capable model or explain the
  limitation) instead of failing silently

### Requirement: Improve Input, Explain This, Quick Capture

The desktop SHALL provide: an **Improve Input** action (magic wand) in the
composer that rewrites the draft prompt via a cheap routed model; an **Explain
This** action on selected chat text or an error that returns a short
explanation in a popup; and a **Quick Capture** global shortcut that opens a
minimal prompt surface from anywhere.

#### Scenario: Improve Input

- **WHEN** the user invokes Improve Input
- **THEN** the draft is rewritten (same intent, clearer wording) through
  `chat.simple` routing and shown for acceptance

#### Scenario: Explain This

- **WHEN** the user selects text or an error and invokes Explain This
- **THEN** a popup shows a concise explanation, generated with the selection as
  context

#### Scenario: Quick Capture

- **WHEN** the global Quick Capture shortcut fires
- **THEN** a capture surface opens, sends the prompt to a chosen agent/session,
  and reports the result without opening the full app

### Requirement: Chat Search and Prompt Library

The system SHALL provide full-text **chat search** over message history and a
**prompt library** (saved, reusable prompts) in the chat sidebar.

#### Scenario: Search history

- **WHEN** the user searches chat history
- **THEN** matching messages are returned with snippets/highlights and jump to
  the source message (FTS5, decision D11)

#### Scenario: Prompt library

- **WHEN** the user saves a prompt
- **THEN** it is stored (`chat_prompts`) and insertable into the composer

### Requirement: Context Viewer and Toggleable Injection

The kernel SHALL assemble the agent context as **named, typed sections**
(`system`, `capabilities`, `constraints`, `memory.episodic`, `memory.semantic`,
`rag`, `history`, `summary`, `input`, `attachments`) and SHALL report, per
section, the token count and source references. The desktop SHALL provide a
**Context Viewer** with a token counter and checkboxes to toggle sections.

#### Scenario: Inspect context

- **WHEN** the client calls `context.inspect(session_id)`
- **THEN** the kernel returns each section with `tokens`, `source_refs`,
  `enabled`, and `cacheable`, plus the total against the input budget

#### Scenario: Toggle a section

- **WHEN** the user unchecks a section (e.g. RAG)
- **THEN** `context.set_section` persists the choice and the section is **not
  sent** to the model on subsequent turns, and the token counter updates

#### Scenario: Counts match the budget

- **WHEN** the Context Viewer shows token counts
- **THEN** they use the same counter the kernel uses to budget the context
  (decision D8), so the numbers are consistent

#### Scenario: Context is observable

- **WHEN** a turn's context is built
- **THEN** a `context.built` event carries the section/token breakdown and is
  persisted in `context_snapshots` for replay

> **RAG is capability-gated**: when `capability.search` is disabled, the `rag`
> section is absent from the Context Viewer and from the context.

### Requirement: Rich Outputs — Sandboxed Charts (capability `charts`)

The chat SHALL render AI-generated rich outputs from typed fenced code blocks
(```fergents-chart, ```fergents-mermaid, ```fergents-html) **only** when the
corresponding capability is enabled, and SHALL render them inside a sandboxed
frame that cannot access the host origin, the network, or the DOM.

#### Scenario: Chart block renders safely

- **WHEN** the agent emits a `fergents-chart` block
- **THEN** the JSON spec is validated against a schema and rendered with
  Recharts inside an `<iframe sandbox="allow-scripts">` (no
  `allow-same-origin`) with a strict CSP and no network access

#### Scenario: Disabled capability changes behavior

- **WHEN** `capability.charts` is disabled
- **THEN** the kernel instructs agents not to emit chart blocks, the UI does
  not render them, and any such block is shown as a plain code block with a
  notice

#### Scenario: No script execution from output

- **WHEN** the model emits HTML/markdown
- **THEN** it is sanitized (DOMPurify allowlist) and never inserted via
  `innerHTML`; `fergents-html` renders only inside the sandboxed frame

#### Scenario: Bounded resources

- **WHEN** a rich output renders
- **THEN** its size/dimensions are bounded and malformed specs produce a
  graceful error, never a crash

### Requirement: Agent Memory (`brain.md`, `session.md`, episodic/semantic)

The system SHALL implement `AGENTS.md` §8 memory: working (context), episodic
(session history in `agent_memory`), and semantic (facts in `agent_memory` +
the brain via RAG), plus two file-backed artifacts: a per-agent **`brain.md`**
(long-term preferences, common errors, learned rules) and a per-session
**`session.md`** (rolling compaction summary), both human- and AI-editable.

#### Scenario: Episodic and semantic memory

- **WHEN** an agent completes turns and records facts
- **THEN** episodic entries and semantic facts are stored, embedded, and
  retrievable via `memory.remember` / `memory.recall` / `memory.forget` /
  `memory.list`

#### Scenario: Compaction produces session.md

- **WHEN** history exceeds its share of the context budget
- **THEN** the kernel compacts (drop/summarize/retrieve per `AGENTS.md` §6.2),
  writes the rolling summary to `session.md` and `agent_sessions.summary`, and
  injects it as the `summary` section

#### Scenario: User control

- **WHEN** the user opens the memory panel
- **THEN** all memory is visible, editable, and deletable

### Requirement: Self-Improving Prompt (`brain.md`) (capability `self-improve`)

When `capability.self-improve` is enabled, the agent SHALL be able to propose
updates to its own `brain.md` (learned preferences, recurring errors, rules).
Updates SHALL always be produced as a **diff**, SHALL be permission-checked,
SHALL default to **suggest** mode (user approval), and SHALL be audited and
reversible.

#### Scenario: Propose an update

- **WHEN** the agent learns a durable fact about the user or a recurring error
- **THEN** it calls `memory.brain.propose` and the UI shows a "Memory update"
  card with the diff

#### Scenario: Apply, gated

- **WHEN** the proposal is approved (or the agent is in `auto` mode)
- **THEN** `memory.brain.apply` writes `agents/<id>/brain.md`, commits a
  shadow-git entry / creates a `.fergents` backup, and emits
  `memory.brain.applied`

#### Scenario: Reversible

- **WHEN** the user clicks Revert on a memory update
- **THEN** `memory.brain.revert` restores the previous `brain.md` and the
  revert is audited

#### Scenario: Bounded and scrubbed

- **WHEN** `brain.md` grows beyond its configured cap
- **THEN** the agent consolidates/trims it, and secrets are never written
  (scrubbed before write)

### Requirement: Spec / Plan / Tasks Workflow with Approval Gates

For complex coding tasks, the agent SHALL run a **spec → plan → tasks →
execute** workflow that writes artifacts (`.fergents/specs/<slug>/spec.md`,
`plan.md`, `tasks.md`), **pauses for human approval** at each configured gate,
and only executes after approval. Gates SHALL be persisted and resumable.

#### Scenario: Generate and pause at spec

- **WHEN** the user starts a complex task in agent mode
- **THEN** the agent writes `spec.md` and the run enters `awaiting_spec_approval`
  with an approval card (Approve / Edit / Reject)

#### Scenario: Approve and continue

- **WHEN** the user approves the spec
- **THEN** the run writes `plan.md`, enters `awaiting_plan_approval`, and so on
  to `tasks.md` and execution

#### Scenario: Edit before approval

- **WHEN** the user edits an artifact
- **THEN** the agent re-reads the edited file on resume and continues from it

#### Scenario: Autonomy levels

- **WHEN** the run is `supervised` / `batch` / `auto`
- **THEN** it pauses at each gate / at spec+plan once / never (decision D7),
  and every automatic gate is audited

#### Scenario: Reproducible artifacts

- **WHEN** the workflow completes
- **THEN** `spec.md`, `plan.md`, and `tasks.md` exist on disk as deliverables
  and the run history is replayable

### Requirement: Auto-Continue on Truncation

When a model response is truncated by a token limit, the kernel SHALL
automatically send a "continue" turn, up to a **user-configurable** limit, and
SHALL allow the user to disable or adjust it.

#### Scenario: Continue until done or cap

- **WHEN** a response ends truncated (`stop_reason = max_tokens`)
- **THEN** the kernel appends a continue instruction and re-invokes the model,
  up to `max_auto_continues`, emitting `chat.continued` for each

#### Scenario: Bounded

- **WHEN** the cap is reached
- **THEN** the run stops and reports that output may be incomplete

### Requirement: Permission Bar and Policy Engine

The system SHALL implement the `SECURITY.md` §3–§5 / `ARCHITECTURE.md` §9
permission system: three modes (manual / auto / full) selectable per session in
the permission bar, permission categories, a declarative TOML policy engine
(first match wins), per-agent and per-tool overrides, the permission-request
card (`UI.md` §12), and complete audit logging.

#### Scenario: Prompt flow

- **WHEN** a tool call needs approval
- **THEN** the kernel emits `agent.permission_requested`, the UI shows a card
  (action, target, reason; Deny / Allow once / Allow for session), and the
  decision is recorded and emitted

#### Scenario: Modes

- **WHEN** the mode is Manual / Auto / Full
- **THEN** all / risky-only / no actions prompt, per `ARCHITECTURE.md` §9.1

#### Scenario: Agent overrides never widen

- **WHEN** an agent declares stricter permissions
- **THEN** they apply; an agent can never exceed the session's permissions

#### Scenario: Audit

- **WHEN** any decision occurs
- **THEN** it is written to `audit_log` with actor, action, args, result,
  reason, and reversibility reference

### Requirement: Reversibility (shadow git + `.fergents` backups)

Every agent file mutation SHALL be reversible via shadow git (in-repo) or a
timestamped `.fergents` backup (out-of-repo), with a restore UI (per-file,
per-session, per-run) per `ARCHITECTURE.md` §10.3, §15.

#### Scenario: Reversible write

- **WHEN** the agent writes or edits a file
- **THEN** a shadow-git commit or `.fergents` backup is created first, and the
  change appears in a diff view with Approve/Reject where applicable

#### Scenario: Rollback

- **WHEN** the user restores a file, a session, or a run
- **THEN** the exact prior state is restored and the rollback is audited

### Requirement: Multi-Model UI and Provider Manager

The desktop SHALL provide a model selector in the chat, a **Provider Manager**
(add/remove/test API keys for OpenAI, Anthropic, Ollama, …; test connections;
show models and capabilities), and a **compare** mode running two parallel
sessions on different models side by side.

#### Scenario: Pin a model per session

- **WHEN** the user picks a model in the chat
- **THEN** it overrides the route for that session, still subject to budgets and
  capabilities (`LLM.md` §4.4)

#### Scenario: Provider test

- **WHEN** the user tests a provider
- **THEN** the kernel calls `provider.test` and reports success/failure and the
  discovered models

#### Scenario: Compare two models

- **WHEN** the user starts a compare
- **THEN** two sessions run in parallel and render side by side (decision D9)

### Requirement: MCP Manager

The desktop SHALL provide an MCP Manager to add/remove/test MCP servers,
configure transports and permissions, and monitor health, per `MCP.md` §5,
§13.

#### Scenario: Add and monitor

- **WHEN** a server is added
- **THEN** it is persisted, connected, listed with health, and its tools
  appear in the agent toolset

#### Scenario: AI-managed servers

- **WHEN** the AI adds/removes a server
- **THEN** `mcp.connect` permission is enforced and the action is audited
  (`MCP.md` §16)

### Requirement: Secret Vault (OS keychain)

API keys and P2P identities SHALL be stored in the OS keychain via `keyring`;
the global DB SHALL store only references (`secret_refs`). Keys SHALL be
resolved at call time and never exposed to agents/plugins (`SECURITY.md` §14).

#### Scenario: Store and reference

- **WHEN** a key is added
- **THEN** it is written to the keychain under a stable name and a reference is
  recorded; `secret.list` returns names/metadata, never values

#### Scenario: No plaintext in config

- **WHEN** providers are configured
- **THEN** config contains `$KEYCHAIN:<name>` references only

### Requirement: Agent Sync (multi-agent collaboration)

Two or more agents SHALL be able to collaborate on one session or file, with
each other's changes visible in real time.

#### Scenario: Shared session

- **WHEN** a second agent joins a session
- **THEN** messages identify the acting agent and each agent's changes are
  visible and attributed

#### Scenario: Shared file

- **WHEN** two agents edit the same file
- **THEN** edits are serialized through the kernel and rendered in the diff view
  without lost updates

### Requirement: Time Travel (state rollback)

The chat SHALL offer a timeline bar to return to a previous message, restore
the exact agent + brain state at that point, and re-explore a decision path
(combined with Fork).

#### Scenario: Restore to a turn

- **WHEN** the user selects a prior turn on the timeline and restores
- **THEN** the session message state and the referenced brain state are restored
  (decision D12), the restore is audited and itself reversible, and the user can
  Fork from there

#### Scenario: Scope is explicit

- **WHEN** a restore would affect state outside the session's scope
- **THEN** it does not; the boundary is shown before confirming

### Requirement: Asynchronous Sub-Agents

A manager agent SHALL be able to spawn background sub-agents, continue its own
work, and collect results when ready, within the `AGENTS.md` §9 limits (depth,
count, budget inherited-not-widened).

#### Scenario: Spawn and collect

- **WHEN** a manager spawns a sub-agent asynchronously
- **THEN** it runs in the background (visible as a child session), the manager
  continues, and `orchestrator.monitor` returns status/results when ready

#### Scenario: Constraints

- **WHEN** a sub-agent is spawned
- **THEN** its tools/permissions/budget are a subset of the parent's and depth
  and count limits are enforced

### Requirement: Ecosystem Extensions (capability-gated)

The following SHALL be delivered as capability-gated features, off by default:
**Ombra** (global hotkey overlay), **Voice Mode** (Whisper input/output),
**Agent Web** (browser extension for page reading), and the **Plugin Factory**
and **Workflow Debugger** hooks (see Appendix F for the Phase 2 boundary).

#### Scenario: Voice Mode

- **WHEN** `capability.voice` is enabled
- **THEN** Whisper transcribes input locally and can speak output, and the
  voice affordances appear in the composer

#### Scenario: Ombra

- **WHEN** `capability.ombra` is enabled and its hotkey fires
- **THEN** a global overlay opens for quick prompts/commands

#### Scenario: Agent Web

- **WHEN** `capability.agent-web` is enabled and the extension is installed
- **THEN** the agent can read the active web page's content through the kernel

#### Scenario: Gated off

- **WHEN** any of these capabilities is disabled
- **THEN** the kernel allocates nothing for it and the UI hides its entry
  points

### Requirement: Settings → Features Page

The Settings SHALL include a **Features** page listing every registered
capability with an On/Off switch, its source, version, dependencies, and
`restart_required` status when applicable.

#### Scenario: Toggle from Settings

- **WHEN** the user flips a switch
- **THEN** `capability.set` persists it; the kernel enables/disables the
  subsystem (hot or restart-required) and the UI updates immediately

#### Scenario: Dependency feedback

- **WHEN** a capability with disabled dependencies is enabled
- **THEN** the page explains the dependency and offers to enable it (or
  auto-enables when configured)

### Requirement: Phase 2 Protocol Surface

The system SHALL extend the JSON-RPC protocol with the Phase 2 commands and
events (Appendix A), generated into Rust and TypeScript via the existing
codegen pipeline, with a freshness test.

#### Scenario: Generated bindings stay in sync

- **WHEN** the Phase 2 schema is added and codegen runs
- **THEN** Rust and TypeScript types reflect it and the freshness tests pass

#### Scenario: Capability guard on dispatch

- **WHEN** a command for a disabled capability is dispatched
- **THEN** the kernel returns a structured `capability_disabled` error without
  side effects

### Requirement: Global Schema (Phase 2)

The system SHALL add the Phase 2 tables to the global store via new
`migrations/global/00NN_*.sql` files, applied by the existing runner, per
Appendix B.

#### Scenario: Migrations apply

- **WHEN** the kernel opens the global store
- **THEN** the Phase 2 migrations are applied idempotently and recorded in
  `schema_version`; a newer schema is refused

#### Scenario: Seeding capabilities

- **WHEN** the kernel boots for the first time after migration
- **THEN** the `capabilities` registry is seeded from declarative kernel
  registrations with `default_enabled` per decision D15

### Requirement: Desktop Chat UI (design-system compliance)

The chat, context viewer, agent builder, provider manager, MCP manager,
secrets, capability settings, and all new panels SHALL be built **only** from
`style/` tokens and components (include order `tokens.css` → `typography.css`
→ `components.css`); any new component/token SHALL be added to `style/` first
and mirrored in `patterns.html`.

#### Scenario: No hardcoded style

- **WHEN** any Phase 2 UI element renders
- **THEN** it uses only `style/` tokens/components with no hardcoded colors,
  spacing, radii, or motion (dark-first, glass, accessible)

#### Scenario: Accessibility

- **WHEN** the chat and panels are used by keyboard/screen reader
- **THEN** all interactive elements are reachable and labelled per `UI.md` §19–§20

### Requirement: Testing and CI

Every task SHALL ship tests and keep CI green (existing gates unchanged:
`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build`,
`eslint`, `prettier --check`, `tsc --noEmit`, pnpm build,
Windows/macOS/Linux). Phase 2 tests SHALL use mock providers, a stub
tokenizer, and a stub MCP server — no network or API keys in CI.

#### Scenario: Kernel tests

- **WHEN** the Rust suite runs
- **THEN** it covers the capability registry (enable/disable/dependencies/
  guard), the router (routing, fallback, budgets, caching), the agent loop
  (tool calls, permissions, stopping conditions), memory (episodic/semantic/
  `brain.md` propose/apply/revert), context building + section toggling +
  token accounting, the spec/plan/tasks gates, and reversibility

#### Scenario: UI tests (Vitest)

- **WHEN** the Vitest suite runs
- **THEN** it covers the chat (messages, streaming, tool cards, hover actions,
  multiple-choice), the context viewer, the capability settings page, the agent
  builder, and rich-output sandbox rendering logic — with mocked kernel data

#### Scenario: End-to-end

- **WHEN** the integration test runs
- **THEN** it creates an agent, runs a session with the mock provider, invokes a
  tool with a permission prompt, toggles a capability, and verifies a
  `brain.md` update round-trip (propose → apply → revert)

### Requirement: Offline and Local Models

Phase 2 SHALL function fully offline with local models (Ollama, llama.cpp):
chat, agents, memory, context viewer, spec workflow, and reversibility SHALL
work with no network when a local provider is configured.

#### Scenario: Fully local run

- **WHEN** the only configured provider is Ollama and the machine is offline
- **THEN** chatting, running agents, and all local features work

#### Scenario: Cloud is opt-in

- **WHEN** no cloud key is configured
- **THEN** routes requiring cloud models report a clear error rather than
  failing obscurely

## MODIFIED Requirements

### Requirement: Registry (Phase 0/1 §Registry)

Phase 0/1 shipped a registry for tools, providers, brains, and sessions. Phase
2 SHALL extend it with **capability** registration and **agent/run** services,
and SHALL gate command dispatch on capability state, without changing the
behavior of existing registrations.

#### Scenario: Capability-aware dispatch

- **WHEN** a command belongs to a disabled capability
- **THEN** dispatch returns `capability_disabled`; all other dispatch is
  unchanged

#### Scenario: Existing registrations unaffected

- **WHEN** Phase 0/1 commands are dispatched
- **THEN** they behave exactly as before

### Requirement: Storage (Phase 0/1 §Storage)

Phase 0/1 `SqliteStorage` opened the global store and per-brain databases. Phase
2 SHALL add the global Phase 2 migrations and SHALL cache the derived memory
embeddings/facts in the global store while keeping the **brain** as the source
of truth for RAG. No behavior of existing stores changes.

#### Scenario: Global store gains Phase 2 tables

- **WHEN** the global store is opened
- **THEN** `agent_*`, `capabilities`, `runs`, `approvals`, `chat_*`, and
  `context_snapshots` exist and are queryable

### Requirement: Event Bus (Phase 0 §Event bus)

The Phase 0 bus carried kernel/brain events. Phase 2 SHALL register the agent,
llm, mcp, capability, chat, memory, and run topics (Appendix A) and SHALL
continue to persist `agent.*`, `run.*`, `permission.*`, and `llm.*` per the
`ARCHITECTURE.md` §4.3 retention policy.

#### Scenario: New topics persist

- **WHEN** an `agent.*` / `llm.*` / `run.*` event is emitted
- **THEN** it is persisted and replayable per the retention policy

#### Scenario: Disabled capability emits nothing

- **WHEN** a capability is disabled
- **THEN** its topics are not emitted (the subsystem is not running)

### Requirement: Settings (Phase 1 §Settings)

The Phase 1 settings page covered brains. Phase 2 SHALL add the Settings
categories from `UI.md` §16 relevant to Phase 2 — Models, Permissions, MCP,
Secrets, and **Features** — using the existing `.settings*` components.

#### Scenario: New settings categories

- **WHEN** the user opens Settings
- **THEN** Models, Permissions, MCP, Secrets, and Features are available and
  each setting has a description and scope

## REMOVED Requirements

None.

## Dependencies

New dependencies introduced in Phase 2, each justified. No others SHALL be
added without justification (per `STACK.md` §7–§9).

| Crate / package | Purpose | Justification |
|---|---|---|
| `reqwest` (+ `rustls`) | HTTP for provider + MCP HTTP/SSE transports | Required by `LLM.md` §2, `MCP.md` §4 |
| `eventsource-stream` / `tokio-stream` | SSE parsing for streaming | Required by `LLM.md` §5 |
| `rmcp` | MCP host/server SDK | Required by `ROADMAP.md` §6 / `MCP.md` §2 |
| `keyring` | OS keychain access | Required by `SECURITY.md` §14 / decision D10 |
| `tokenizers` (HF) or `tiktoken-rs` | Token counting for budgeting + Context Viewer | Required by decision D8 |
| `whisper-rs` (or `whisper.cpp` bindings) | Local voice transcription (`voice`, off by default) | Required by `BACKLOG.md` §5; capability-gated |
| `similar` / `diffs` | Diff generation for `brain.md`, spec artifacts, edits | Required by decisions D6/D7 and `UI.md` §13 |
| `schemas/agent.schema.json` (asset) | Agent frontmatter validation | Required by `AGENTS.md` §4.3 |
| `recharts` (npm) | Chart rendering (`charts`) | Decision D4 |
| `dompurify` (npm) | Sanitizing model HTML/markdown | Decision D5 |
| `mermaid` (npm) | Rich diagram blocks (`mermaid`) | `BACKLOG.md` §1; sandboxed |
| `@tanstack/react-query` (npm) | Data fetching against the kernel protocol | Already in Phase 0/1, reused |
| `reactflow` (npm) | Agent graph / timeline visuals | Already used in Phase 1, reused |

No dependency shall be added for: Monaco/LSP/terminal (Phase 3), plugin runtime
(Phase 4), workflow engine (Phase 5), sync/mobile (Phase 7).

## Appendix A: Phase 2 protocol surface

**Command methods (Phase 2)**

- Capabilities: `capability.list`, `capability.get`, `capability.set`,
  `capability.register`, `capability.resolve`.
- Chat/session: `chat.session.create`, `chat.session.list`, `chat.session.get`,
  `chat.session.rename`, `chat.session.delete`, `chat.session.search`,
  `chat.session.export`, `chat.message.send`, `chat.message.list`,
  `chat.message.regenerate`, `chat.message.fork`, `chat.continue`,
  `chat.stop`, `chat.attach`, `chat.explain`, `chat.compare.start`.
- Prompts: `prompt.list`, `prompt.create`, `prompt.delete`.
- Capture: `capture.send`.
- Context: `context.inspect`, `context.set_section`, `context.set_budget`.
- Agents: `agent.list`, `agent.get`, `agent.create`, `agent.update`,
  `agent.delete`, `agent.enable`, `agent.disable`, `agent.spawn`, `agent.call`,
  `agent.message`, `agent.handoff`, `agent.cancel`.
- Memory: `memory.remember`, `memory.recall`, `memory.forget`, `memory.list`,
  `memory.brain.get`, `memory.brain.propose`, `memory.brain.apply`,
  `memory.brain.revert`.
- Runs: `run.list`, `run.get`, `run.pause`, `run.resume`, `run.cancel`,
  `run.approve`, `run.reject`.
- Spec workflow: `spec.start`, `spec.get`, `spec.approve`.
- Providers: `provider.list`, `provider.add`, `provider.remove`,
  `provider.test`, `provider.models`, `provider.route.get`,
  `provider.route.set`.
- Secrets: `secret.list`, `secret.set`, `secret.delete`, `secret.test`.
- MCP: `mcp.server.list`, `mcp.server.add`, `mcp.server.remove`,
  `mcp.server.test`, `mcp.server.health`, `mcp.tool.list`, `mcp.call`.
- Render: `render.validate`.
- Orchestration: `orchestrator.decompose`, `orchestrator.monitor`,
  `orchestrator.merge`, `orchestrator.escalate`.
- Time travel: `timetravel.inspect`, `timetravel.restore`.

**Event kinds (Phase 2)**

```
capability.registered / capability.enabled / capability.disabled / capability.changed

chat.session.created / chat.session.updated / chat.session.deleted
chat.message.created / chat.token / chat.completed / chat.continued / chat.stopped
chat.forked / chat.explain.ready

context.built

agent.created / agent.updated / agent.deleted / agent.enabled / agent.disabled
agent.started / agent.thinking / agent.token / agent.tool_call / agent.message
agent.finished / agent.error
agent.permission_requested / agent.permission_granted / agent.permission_denied
agent.subagent.spawned / agent.subagent.finished / agent.handoff

memory.written / memory.brain.proposed / memory.brain.applied / memory.brain.reverted

run.created / run.step.started / run.step.finished / run.paused
run.approval_requested / run.approved / run.rejected
run.resumed / run.completed / run.failed / run.cancelled

llm.request / llm.response / llm.error / llm.cost / llm.routed / llm.cached / llm.rate_limited

mcp.server.connected / mcp.server.disconnected / mcp.server.error
mcp.tool.registered / mcp.tool.called / mcp.resource.read / mcp.prompt.requested

secret.set / secret.deleted
timetravel.restored
```

## Appendix B: Global schema (normative)

New files under `migrations/global/` (numbered after `0002_*.sql`).

- **`capabilities`** — `id TEXT PRIMARY KEY`, `name TEXT NOT NULL`,
  `version TEXT NOT NULL`, `source TEXT NOT NULL`, `dependencies TEXT` (JSON
  array), `resources TEXT` (JSON), `default_enabled INTEGER NOT NULL DEFAULT 0`,
  `enabled INTEGER NOT NULL`, `hot_toggle INTEGER NOT NULL DEFAULT 0`,
  `config TEXT` (JSON NULL), `registered_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`.

- **`agent_sessions`** — `id TEXT PRIMARY KEY` (ULID), `agent_id TEXT NOT NULL`,
  `project_id TEXT NULL`, `brain_id TEXT NULL`, `parent_session_id TEXT NULL`,
  `title TEXT NULL`, `model TEXT NULL`, `state TEXT NOT NULL`
  (`created|running|paused|waiting_permission|done|error|cancelled`),
  `permission_mode TEXT NOT NULL DEFAULT 'auto'`, `summary TEXT NULL`,
  `tokens_in INTEGER NOT NULL DEFAULT 0`, `tokens_out INTEGER NOT NULL DEFAULT 0`,
  `cost_usd REAL NOT NULL DEFAULT 0`, `created_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`, `closed_at INTEGER NULL`; indexes on
  `(agent_id, updated_at)`, `(state)`.

- **`agent_messages`** — `id TEXT PRIMARY KEY` (ULID), `session_id TEXT NOT NULL
  REFERENCES agent_sessions`, `seq INTEGER NOT NULL`, `role TEXT NOT NULL`,
  `content TEXT NOT NULL` (JSON content blocks), `tool_calls TEXT` (JSON NULL),
  `tokens INTEGER NOT NULL DEFAULT 0`, `created_at INTEGER NOT NULL`; unique
  index on `(session_id, seq)`.

- **`agent_messages_fts`** — FTS5 virtual table over `content` (chat search,
  decision D11).

- **`agent_memory`** — `id TEXT PRIMARY KEY` (ULID), `agent_id TEXT NOT NULL`,
  `scope TEXT NOT NULL` (`global|agent|project|brain`), `kind TEXT NOT NULL`
  (`episodic|fact`), `content TEXT NOT NULL`, `importance REAL NOT NULL
  DEFAULT 0.5`, `created_at INTEGER NOT NULL`, `last_used_at INTEGER NULL`;
  index on `(agent_id, kind, scope)`.

- **`agent_children`** — `parent_session_id TEXT NOT NULL`,
  `child_session_id TEXT NOT NULL`, `task TEXT NULL`, `status TEXT NOT NULL`,
  `created_at INTEGER NOT NULL`, primary key `(parent_session_id,
  child_session_id)`.

- **`runs`** — `id TEXT PRIMARY KEY` (ULID), `session_id TEXT NULL`, `kind TEXT
  NOT NULL` (`chat|spec_workflow`), `mode TEXT NOT NULL`
  (`supervised|batch|auto`), `state TEXT NOT NULL`, `budget TEXT` (JSON NULL),
  `artifacts TEXT` (JSON NULL), `created_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`; index on `(session_id, created_at)`.

- **`run_steps`** — `id TEXT PRIMARY KEY` (ULID), `run_id TEXT NOT NULL
  REFERENCES runs`, `seq INTEGER NOT NULL`, `kind TEXT NOT NULL`,
  `state TEXT NOT NULL`, `payload TEXT` (JSON NULL), `started_at INTEGER`,
  `finished_at INTEGER NULL`; index on `(run_id, seq)`.

- **`approvals`** — `id TEXT PRIMARY KEY` (ULID), `run_id TEXT NOT NULL
  REFERENCES runs`, `gate TEXT NOT NULL` (`spec|plan|task|diff`),
  `ref TEXT NULL`, `status TEXT NOT NULL` (`pending|approved|rejected|edited`),
  `payload TEXT` (JSON NULL), `decided_by TEXT NULL`, `decided_at INTEGER NULL`,
  `created_at INTEGER NOT NULL`; index on `(run_id, gate)`.

- **`context_snapshots`** — `id TEXT PRIMARY KEY` (ULID), `session_id TEXT NOT
  NULL`, `turn_seq INTEGER NOT NULL`, `sections TEXT NOT NULL` (JSON),
  `total_tokens INTEGER NOT NULL`, `created_at INTEGER NOT NULL`; index on
  `(session_id, turn_seq)`.

- **`chat_prompts`** — `id TEXT PRIMARY KEY` (ULID), `title TEXT NOT NULL`,
  `body TEXT NOT NULL`, `tags TEXT` (JSON NULL), `created_at INTEGER NOT NULL`.

- **`secret_refs`** — `name TEXT PRIMARY KEY`, `keychain_ref TEXT NOT NULL`,
  `provider TEXT NULL`, `created_at INTEGER NOT NULL`. **No secret values.**

## Appendix C: `brain.md` and `session.md` layout

```
$FERGENTS_HOME/
├── agents/
│   ├── researcher.md              ← definition (frontmatter + system prompt)
│   └── researcher/
│       └── brain.md               ← self-improving long-term memory (D6)
└── sessions/
    └── <session_id>/
        └── session.md             ← rolling compaction summary (mirrored in
                                       agent_sessions.summary)
```

`brain.md` is human- and AI-editable, diff-gated, size-capped, secret-scrubbed,
and always backed by shadow git or a `.fergents` backup before a write.

## Appendix D: Capability catalogue (Phase 2 seed)

| Capability | Depends on | Default | Hot |
|---|---|---|---|
| `capability.search` | — | on | yes |
| `capability.graph` | — | on | yes |
| `capability.chat` | `capability.llm` | on | no |
| `capability.llm` | — | on | no |
| `capability.agents` | `capability.llm`, `capability.chat` | on | no |
| `capability.memory` | `capability.agents` | on | yes |
| `capability.mcp` | `capability.agents` | on | yes |
| `capability.sub-agents` | `capability.agents` | on | yes |
| `capability.agent-sync` | `capability.agents` | on | yes |
| `capability.time-travel` | `capability.chat` | on | yes |
| `capability.compare` | `capability.chat` | off | yes |
| `capability.chat-search` | `capability.chat` | on | yes |
| `capability.explain` | `capability.llm` | on | yes |
| `capability.capture` | `capability.chat` | on | yes |
| `capability.spec-workflow` | `capability.agents` | on | yes |
| `capability.self-improve` | `capability.memory` | off | yes |
| `capability.charts` | `capability.chat` | off | yes |
| `capability.mermaid` | `capability.chat` | off | yes |
| `capability.html` | `capability.chat` | off | yes |
| `capability.voice` | `capability.chat` | off | yes |
| `capability.ombra` | `capability.chat` | off | yes |
| `capability.agent-web` | `capability.agents` | off | yes |
| `capability.plugin-factory` | `capability.agents` | off | no |
| `capability.workflow-debugger` | `capability.agents` | off | no |

## Appendix E: Key decisions (recorded as ADRs)

- **ADR-021:** The kernel is the authority for capability state; the UI is a
  projection, and a disabled capability allocates nothing.
- **ADR-022:** Charts/Mermaid/HTML render in a sandboxed iframe
  (`allow-scripts`, no `allow-same-origin`, strict CSP, no network) with
  schema-validated specs and DOMPurify sanitization.
- **ADR-023:** `brain.md` self-improvement is diff-gated, default-`suggest`,
  audited, size-capped, secret-scrubbed, and reversible.
- **ADR-024:** Spec/Plan/Tasks approval gates are first-class persisted run
  states reusing the permission-card pattern.
- **ADR-025:** Context accounting uses the same tokenizer the kernel budgets
  with; the Context Viewer reports those exact counts.
- **ADR-026:** Secrets live only in the OS keychain; the DB stores references.

## Appendix F: Phase 2 delivery boundary for ecosystem items

`BACKLOG.md` §5 lists ecosystem items whose full runtime depends on substrates
scheduled for later phases. Phase 2 SHALL ship their **capability registration
and hooks** and defer the runtime as follows:

- **Voice Mode** — full Phase 2 (local Whisper, capability-gated, off by
  default).
- **Ombra** — Phase 2 overlay shell (global hotkey + quick prompt), capability-
  gated.
- **Agent Web** — Phase 2 kernel-side "read page" tool + extension scaffold;
  the extension ships as a Phase 2 deliverable, capability-gated.
- **Plugin Factory** — capability + registration plumbing in Phase 2; the
  write/compile/test runtime is **deferred to Phase 4** (plugin runtime), and
  `capability.plugin-factory` stays non-hot until then.
- **Workflow Debugger** — capability + entry-point plumbing in Phase 2; the
  debugger runtime is **deferred to Phase 5** (workflow engine), and
  `capability.workflow-debugger` stays non-hot until then.

Everything else in `BACKLOG.md` §0–§4 is fully in Phase 2 scope.

## Appendix G: Out of scope (later phases)

- Monaco / LSP / terminal / git / inline AI in the IDE — **Phase 3**.
- Skills runtime, WASM plugin host, marketplace — **Phase 4**.
- Workflow engine, visual editor, triggers, debugger — **Phase 5**.
- Dashboards & custom widgets — **Phase 6**.
- Sync (P2P/CRDT) & Android companion — **Phase 7**.
