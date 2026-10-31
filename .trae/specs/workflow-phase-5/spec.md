# Workflow Engine — Phase 5 Spec

**Status:** Draft — awaiting approval. No implementation starts until approved.
**Roadmap:** `ROADMAP.md` §9.
**Source of truth for behaviour:** `WORKFLOW.md`; `UI.md` §3, §5.4, §6, §11,
§16, §19, §22; `style/readme.md` and `style/patterns.html` pattern 06.

---

## Why

Phases 1–4 gave Fergents a brain, an agent runtime, an IDE, and an extension
system. Every one of those is an *interactive* surface: a person, or an agent
answering a person, drives it one step at a time.

Phase 5 adds the missing layer: **automation that composes the existing
surfaces without a human in the loop**. A workflow calls an LLM, runs an agent,
invokes a tool or an MCP server, executes sandboxed code, branches, loops, and
waits for approval — on a schedule, from a webhook, on a file change, on a
kernel event, or because the AI decided to.

It also closes the gap between *automation* and *product*: any workflow can be
published as a **mini-app** with a declarative YAML UI manifest
(`WORKFLOW.md` §14), and every workflow is editable in a visual DAG canvas and
debuggable step by step.

The design constraints are the ones that already govern the kernel:

- **Local-first.** No hosted scheduler; the daemon is the runtime.
- **Kernel-authoritative.** A workflow is a kernel object; the UI is a client.
- **Permissioned and auditable.** A run is a chain of ordinary, individually
  permissioned actions.
- **Reversible and observable.** Every run and step is persisted, replayable,
  and attributable.
- **File-first.** The YAML is the source of truth; the canvas is a renderer.

---

## Scope at a glance

| Area | Delivered in Phase 5 |
|---|---|
| DAG engine | Typed nodes, validated graph, templated data flow, custom node types |
| Triggers | Cron, webhook, file watch, event bus, manual, AI |
| Execution | Persisted runs, checkpoints, retries, cancellation, crash recovery |
| Visual editor | React Flow canvas, node inspector, debugger, app surfaces |
| Workflow to app | YAML app manifest, app surface, run/publish, `.fergapp` export/import |
| Capability | New `capability.workflow`; upgraded `capability.workflow-debugger` |

Not in Phase 5: marketplace distribution of apps, hosted/remote execution,
sync, mobile, dashboard widgets, MCP and chain triggers, `merge`/`delay` nodes
(Appendix G).

---

## What Changes

Phase 5 is additive. No Phase 1–4 command, event, or schema changes meaning.

New pieces:

- **`crates/workflow`** (package `fergents-workflow`): document model and
  validation, template and expression engines, scheduler, trigger runtime,
  run store, app manifest.
- **`migrations/global/0018_workflow.sql`**: the workflow, version, run, step,
  checkpoint, and trigger tables (Appendix B).
- **Protocol surface**: `workflow.*` commands (Appendix A), regenerated into
  `protocol/schema/protocol.schema.json`, `crates/protocol`, and
  `packages/protocol`.
- **Desktop surfaces**: `apps/desktop/src/features/workflows` (list, canvas,
  inspector, debugger, run trace, versions, app runner).
- **New permission category** `workflow.run`; `workflow.edit` already exists.
- **Capability registrations**: `capability.workflow` (new) and
  `capability.workflow-debugger` (upgraded).

---

## 0. Capability registrations (cross-cutting)

Every Phase 5 surface is a registered **capability** (Phase 2 D1–D3). Disabled
means the kernel refuses the commands (`capability_disabled`), arms no trigger,
allocates no engine, publishes no workflow events, and the UI renders no
workflow surface.

| Capability | Default | Hot | Depends on | Allocated when enabled |
|---|---|---|---|---|
| `capability.workflow` | **off** | yes | `capability.agents` | run store, scheduler, trigger registry, loopback webhook listener, app registry, engine |
| `capability.workflow-debugger` | **off** (existing) | **yes** (was restart-required) | `capability.workflow` | debug sessions, step/continue/inspect, replay-from-step |

Defaults follow Phase 2 §14.1: a workflow can execute shell, network, and
code-executing nodes, so it is **off by default** and must be switched on
deliberately. The debugger is a strict subset, so it is off and depends on the
engine.

`capability.workflow-debugger` already exists in
`crates/capabilities/src/catalogue.rs` (`default_enabled = false`,
`hot_toggle = false`, description "runtime in Phase 5"). Phase 5 upgrades it
(MODIFIED Requirements).

**Capability routing.** `crates/api/src/server.rs::required_capability` gains:

```text
method if method.starts_with("workflow.debug.") => Some("capability.workflow-debugger"),
method if method.starts_with("workflow.")       => Some("capability.workflow"),
```

The debug prefix **must** precede the generic prefix.

---

## 1. The DAG engine

### 1.1 Graph model and validation

A workflow is a directed acyclic graph with:

- **Nodes** — units of work, unique `id` within the document.
- **Edges** — typed data/control connections, `from`/`to` plus optional ports.
- **Ports** — named inputs and outputs declared by each node type.
- **Context** — the JSON object carried through the run (§1.5).
- **Triggers** — one or more entry points (§2).

Validation runs on load, save, import, and run, and fails with
`workflow_invalid` plus a machine-readable issue list (path, node, code,
message). A document is valid only when:

1. The graph is acyclic at the top level. Cycles are permitted only *inside* a
   `loop` body (which is itself validated as a DAG).
2. Every node is reachable from a trigger.
3. There is at least one `trigger` and one `end` node.
4. Node ids are unique and match `^[a-z][a-z0-9_]*$`.
5. Every edge references existing nodes and declared ports.
6. Every required config field is present and type-correct.
7. Every template compiles and every referenced context path is writable by an
   upstream node (the "unknown-source" lint, §1.9).
8. No `subworkflow` cycle exists and nesting depth is ≤ 5.

### 1.2 Node set (v1)

| Type | Purpose | Config | Inputs | Outputs |
|---|---|---|---|---|
| `trigger` | Entry point | `trigger_id` | — | `event` |
| `llm` | Call the LLM router | `model`, `prompt`, `system?`, `temperature?`, `max_tokens?`, `json?` | `prompt`, `context` | `text`, `usage` |
| `tool` | Call a kernel/plugin/MCP tool | `tool`, `args` | `args` | `result` |
| `agent` | Run an agent to completion | `agent`, `task`, `budget?`, `tools?` | `task`, `context` | `result`, `artifacts` |
| `code` | Run sandboxed code | `language`, `source`, `memory_mb?`, `fuel?` | `input` | `output` |
| `http` | HTTP request | `method`, `url`, `headers?`, `body?`, `max_bytes?` | `url`, `body` | `status`, `body`, `headers` |
| `condition` | Branch | `expr` | `value` | `true`, `false` |
| `loop` | Iterate a subgraph | `items` or `count`, `max_iterations`, `parallel` | `items` | `results` |
| `subworkflow` | Run another workflow | `workflow`, `version` (`latest`), `inputs` | `inputs` | `outputs` |
| `human` | Wait for a decision | `prompt`, `options`, `timeout_s`, `on_timeout` | `prompt` | `decision`, `notes` |
| `transform` | Declarative data mapping | `ops` (`pick`/`map`/`filter`/`merge`/`sort`), `template` | `input` | `output` |
| `end` | Terminal node | `result` | `result` | — |

`merge` and `delay` (`WORKFLOW.md` §3.1) are deferred; fan-in is implicit
(§1.4) and a delay can be expressed with a `code` node. See Appendix G.

### 1.3 Node anatomy

Every node has `id`, `type`, `config`, optional `inputs`, optional `outputs`,
and the common execution controls:

```yaml
- id: summarize
  type: llm
  config: { model: "ollama:qwen2.5:7b", prompt: "Summarize:\n{{ fetch.body }}" }
  inputs: { text: "{{ fetch.body }}" }
  outputs: { summary: "{{ result.text }}" }
  retry:
    max: 2
    backoff_ms: 500
    backoff_multiplier: 2
    jitter: true
    retry_on: [timeout, rate_limit, network]
  timeout_s: 60
  on_error: retry      # fail | continue | retry | compensate
  resume: skip         # skip | retry | fail (interrupted-run policy)
```

Defaults: `retry.max = 0`, `timeout_s = 60` (node), `on_error = fail`,
`resume = fail` for nodes with side effects and `retry` for pure nodes.

#### 1.3.1 Execution envelope

Each node executes as: **resolve inputs → permission check → execute →
validate outputs → persist step**. A failure at any step is handled by
`on_error`; a validation failure is never retried silently.

### 1.4 Edges and data passing

```yaml
edges:
  - { from: trigger, to: fetch }
  - { from: condition, from_port: "true", to: deploy }
  - { from: condition, from_port: "false", to: notify }
```

- An edge activates when its source port produces a value; the `false` port and
  skipped nodes propagate `skipped` to targets that have no other active
  inbound edge.
- **Fan-out** duplicates the value; **fan-in** waits for all inbound edges that
  can still activate, then runs once.
- Data is not pushed along edges implicitly: the target reads it from the
  context with `inputs` (`{ text: "{{ fetch.body }}" }`). Edges define control
  and readiness; context defines values. This keeps the graph readable and
  templates explicit (`WORKFLOW.md` §4.1).
- A node's outputs are written under its id (`context.summarize.summary`).
  `outputs: { "$root.key": "..." }` writes to the root context, subject to the
  same validation as any other write.

### 1.5 Context and templating

The run context is a JSON object:

```json
{
  "trigger": { "body": "..." },
  "variables": { "feeds": ["..."] },
  "fetch": { "status": 200, "body": "..." },
  "summarize": { "summary": "..." }
}
```

Templates use `{{ … }}` and support exactly (Appendix D):

- Context paths: `{{ trigger.body }}`, `{{ fetch.items[0].title }}`.
- The current node result: `{{ result.text }}`.
- Variables: `{{ variables.feeds }}`.
- Built-ins: `{{ now }}`, `{{ today }}`, `{{ run.id }}`, `{{ workflow.id }}`.
- Secrets: `{{ secrets.github_token }}` — resolved through `crates/secrets`,
  requiring the secret-read permission, and **never** written to logs, events,
  step records, or run history.
- Filters: `| length`, `| upper`, `| lower`, `| join(",")`, `| default(x)`,
  `| json`, `| date(fmt)`.

Rules:

- Templates are compiled and syntax-checked at save; a bad template blocks the
  save (`workflow_invalid`).
- Resolution is **strict**: a missing path fails the node with
  `workflow_template_missing` unless a `default` filter is present.
- There is no `eval`, no function calls, and no arbitrary expressions in
  templates. `condition` uses the separate expression language (§1.6).
- Values larger than **1 MiB** are spilled to
  `$FERGENTS_HOME/workflows/runs/<run-id>/artifacts/` and referenced as
  `{ "$ref": "<name>", "bytes": <n>, "sha256": "<hex>" }`; templates resolve
  references on demand. Live context is capped at **32 MiB**.

### 1.6 Conditions, loops, and transforms

- **`condition`** evaluates a closed expression AST — comparisons
  (`eq`, `ne`, `gt`, `gte`, `lt`, `lte`), membership (`in`, `contains`,
  `matches`), boolean composition (`and`, `or`, `not`) — never text eval.
  The result selects the `true` or `false` port.
- **`loop`** declares a body DAG and iterates over `items` or `count`.
  Iteration scope: `{{ item }}`, `{{ index }}`. `parallel` defaults to 1
  (max 4). Hard cap `max_iterations = 1000`, default 100. Results aggregate
  into `results[]`.
- **`transform`** applies declarative operations (`pick`, `map`, `filter`,
  `merge`, `sort`) and/or an output template. Anything more complex is a
  `code` node, not an expression.
- **`http`** is a normal permissioned call: it requires `net.http` with a
  domain scope, caps the response at 8 MiB, rejects redirects outside the
  granted domains, and never follows a redirect carrying a secret header.
- **`tool`** resolves through the existing tool registry, including
  `plugin.tool.call` and `mcp.call`; the tool's own permission category
  applies.
- **`agent`** spawns a normal agent session with an inherited (never widened)
  budget; the run records the child session id.
- **`code`** runs in `crates/wasm` (Phase 4 D2/D7): TypeScript or Rust is
  compiled at save time through the factory toolchain, cached by source hash,
  and executed with 32 MiB / 50 M fuel / 5 s unless the node declares more
  within the Phase 4 caps. No filesystem or network unless declared.

### 1.7 Custom node types from plugins

A plugin may declare node types in its manifest (`WORKFLOW.md` §15.1,
`PLUGINS.md`):

```toml
[[nodes]]
id = "rss.fetch"
name = "Fetch RSS"
category = "Network"
```

- Node ids are namespaced `plugin:<plugin-id>:<node-id>` internally and shown
  as `rss.fetch` in the palette.
- Input and output schemas are **closed** JSON Schemas; the inspector renders
  them like built-ins.
- Execution calls the plugin through the Phase 4 sandbox and host functions;
  every call is permissioned exactly like a plugin tool call.
- Nodes disappear from the palette when the plugin is disabled; existing
  workflows that use them fail validation with `workflow_node_unavailable`
  rather than running blank nodes.
- The canvas renders plugin nodes through the `workflow.node` slot
  (`UI.md` §22) when the plugin ships a UI contribution, and through the
  generic typed node otherwise.

### 1.8 Document format

YAML (`WORKFLOW.md` §10.1), with the additions Phase 5 needs:

```yaml
id: rss-digest
name: RSS Digest
description: Fetch RSS feeds and produce a daily digest.
version: 1.0.0
tags: [rss, digest]

permissions:            # requested by the workflow; reviewed on enable/publish
  net.http: ["https://example.com/**"]
  llm.call: true

variables: { feeds: ["https://example.com/feed.xml"] }

triggers:
  - id: nightly
    kind: cron
    schedule: "0 7 * * *"
    timezone: Europe/Rome

nodes: [...]
edges: [...]
app: {...}              # optional, §5
```

Validation uses `workflows/schema/workflow.schema.json` plus the semantic
rules of §1.1. The schema ships with the product, is referenced by Monaco, and
is regenerated into `packages/protocol` if it becomes part of the wire surface.

### 1.9 Validation, lints, and dry run

- **Errors** block save/run: cycles, orphans, missing ports, unknown node
  types, bad templates, invalid configs, missing triggers.
- **Warnings** do not block: a referenced context key no upstream node can
  write, an unused node, an unreachable port, a retry whose backoff exceeds
  the run timeout.
- **Dry run** (`workflow.run` with `dry_run: true`) validates the graph and
  resolves templates with placeholder data: `llm` and `agent` return synthetic
  results, `tool`/`http`/`code` perform no side effects, `human` auto-approves.
  The run is recorded as a dry run and never fires compensations.

---

## 2. Triggers

### 2.1 Kinds

| Kind | Fires when | Config | Notes |
|---|---|---|---|
| `manual` | UI or `workflow.run` | — | Always available when the capability is on |
| `ai` | An agent calls `workflow.run` | `description` | Same path as manual; actor `agent:<id>` |
| `cron` | Schedule | `schedule`, `timezone` | In-process scheduler, persisted next-fire time |
| `webhook` | Loopback HTTP request | `path`, `method`, `auth` | Bound to `127.0.0.1` only |
| `file` | File change under a project | `path` (glob), `events` | `notify` watcher, 300 ms debounce |
| `event` | Kernel event bus topic | `topic`, `filter` (JSON pointer match) | Subscribed only while enabled |

`mcp` and `chain` triggers (`WORKFLOW.md` §5.1) are deferred to Phase 5.1
(Appendix G); MCP remains callable from `tool` nodes.

A workflow may declare several triggers; every run records `trigger_kind` and
`trigger_id`.

### 2.2 Cron

- Parsed with a strict 5-field cron parser; invalid schedules block save.
- `timezone` defaults to the user's configured timezone; DST gaps fire at the
  next valid instant, overlaps fire once.
- Next-fire time is persisted (`workflow_triggers.next_fire_at`), so restarts
  do not drift.
- **Missed fires** (`catch_up`): `skip` (default), `run_once`, `run_all`
  (bounded by a 24 h window and the queue cap).
- Minimum interval 60 s; sub-minute schedules are rejected.

### 2.3 Webhook

- Listener binds `127.0.0.1:<port>` (port from settings, default `8787`); it is
  never exposed on `0.0.0.0`.
- Route: `POST /hooks/<workflow-id>/<trigger-id>` (method from config).
- Auth: a per-trigger bearer token stored in the keychain; constant-time
  comparison; a missing or incorrect token returns `404` (no existence
  oracle).
- Body ≤ 1 MiB, JSON or text; the request becomes `trigger.body` plus
  `trigger.headers`.
- Idempotency: an optional `Idempotency-Key` header deduplicates within a
  10-minute window.
- Rate limit: 60 requests/minute per trigger; excess returns `429` without a
  run.
- On success the listener returns `202` with `{ run_id }`; execution is
  asynchronous.
- Starting the listener requires `net.http` permission; rotating the token
  emits `workflow.updated` and never the token itself.

### 2.4 File watch

- Reuses the Phase 4 `notify` watcher pattern: 300 ms debounce, per-workflow
  glob, ignore list (`.git`, `node_modules`, `target`, trash).
- Canonicalised containment: a path resolving outside the project root never
  fires.
- Events map to `trigger.path` and `trigger.event` (`create`, `modify`,
  `delete`, `rename`).
- Bursts coalesce into one run per 2 s window by default.

### 2.5 Event bus

- Subscribes to a topic (`file.changed`, `plugin.installed`, …) with an
  optional JSON-pointer filter.
- Deduplicates by event id within 1 s.
- A workflow cannot subscribe to its own `workflow.*` events, to avoid
  unbounded self-triggering; the validator rejects it.

### 2.6 Manual and AI

- `manual` runs originate from the canvas, the app surface, the CLI, or
  `workflow.run`.
- `ai` runs are identical, but the actor is the agent session; the agent tools
  (`workflow.run`) are subject to `workflow.run` permission and to the
  workflow's reviewed permission set.

### 2.7 Backpressure and concurrency

- Per-workflow queue cap 32 runs; beyond it, triggers are rejected with
  `workflow_queue_full` (webhooks return `429`).
- `concurrency` per workflow defaults to 1; stateless workflows may declare
  more (cap 4).
- A global cap of 8 concurrent node executions protects the kernel; the
  scheduler queues ready nodes.

---

## 3. Execution model

### 3.1 Scheduler

- At run start the engine computes a **topological rank** and an execution
  plan. Ready nodes are dispatched as their active inbound edges complete.
- Independent nodes run in parallel (per-run default 4, global 8).
- Dispatch order is deterministic: rank, then node id. This makes traces and
  tests reproducible.
- Each node execution is a cancellable async task holding a cancellation
  token; `code`, `llm`, `tool`, `agent`, and `http` all accept it.

### 3.2 Run lifecycle

```text
queued → running → (paused | waiting_human | interrupted) → done | failed | cancelled
```

- Every transition is persisted (`workflow_runs.state`) and emitted
  (`workflow.*` events).
- `paused` is user- or budget-initiated; `waiting_human` is the `human` node;
  `interrupted` is crash recovery.
- A run records: trigger, inputs, actor (`user`, `agent:<id>`,
  `trigger:<id>`), workflow version, budget, context snapshots, per-node
  timings, cost, and tokens.

### 3.3 Persistence and checkpoints

- `workflows` mirrors on-disk YAML (`path`, `digest`); YAML remains the source
  of truth.
- `workflow_versions` stores every saved YAML snapshot; a run pins the version
  it started with.
- `workflow_runs` and `workflow_steps` store one row per run and per node
  attempt (with the iteration index for `loop`).
- A **checkpoint** is written after each completed step: node id, step
  sequence, and a context snapshot (with artifact references for large
  values). Checkpoints are pruned to the last 50 per run.
- External edits to YAML are detected by digest; `workflow.changed` is emitted
  and the canvas offers reload or diff.

### 3.4 Retries and timeouts

- `retry`: `max`, `backoff_ms` (default 500), `backoff_multiplier` (2),
  `jitter`, `retry_on` (`timeout`, `rate_limit`, `network`, `transient`;
  permission errors are never retried).
- Node timeout (`timeout_s`, default 60) cancels the task and marks the step
  timed out.
- Run timeout (`timeout_s`, default 1800) cancels the run (`cancelled`, reason
  `timeout`).
- Retry decisions and attempts are persisted on the step.

### 3.5 Cancellation

`workflow.run.cancel` requests cancellation:

1. Running nodes receive the token and stop cooperatively (5 s grace).
2. Pending nodes are marked `skipped`.
3. The run becomes `cancelled` (reason `user`).
4. Compensations do **not** run on cancellation unless the workflow declares
   `on_cancel: compensate`.

Cancellation is idempotent; a second call is a no-op.

### 3.6 Crash recovery

On kernel boot, with `capability.workflow` enabled:

- Runs in `running` become `interrupted`; steps in `running` become
  `interrupted`.
- The run is **not** resumed automatically. It is paused pending a decision.
- `workflow.run.resume` resumes from the last checkpoint: completed nodes are
  skipped, the interrupted node follows its `resume` policy (`skip`, `retry`,
  `fail`), and downstream nodes re-evaluate their inputs.
- Triggers re-arm: cron uses the persisted next-fire time and the catch-up
  policy, webhook routes are re-registered, file watchers are recreated.
- A resumed run keeps its original version and budget.

### 3.7 Human nodes

- A `human` node pauses the run in `waiting_human` and emits
  `workflow.human.requested` with a review payload (`prompt`, `options`,
  node).
- The UI renders a review card (the Phase 4 `ReviewCard` pattern) in the run
  trace and in the chat; the agent can surface it too.
- The decision arrives via `workflow.run.approve` / `workflow.run.reject`
  (`decision`, `notes`), is persisted on the step, and is audited.
- `timeout_s` and `on_timeout` (`fail`, `continue:<option>`) bound the wait;
  the default is `on_timeout: fail` after 24 h.

### 3.8 Compensation and errors

- `on_error: compensate` runs the node's `compensation` action; a failing run
  runs compensations for completed nodes in **reverse order**.
- `continue` marks the step failed and writes an error marker into the
  context; downstream nodes may inspect it.
- A top-level `on_error` action runs once on failure and can notify, write a
  note, or open a human node.
- Every failure is persisted with node, attempt, error code, and message.

### 3.9 Budgets, concurrency, and idempotency

- Per-run budget: `max_wall_time_s`, `max_cost_usd?`, `max_tokens?`,
  `max_node_calls?`. Exceeding any budget pauses the run with
  `workflow_budget_exceeded`; the user can raise and resume.
- Cost and token usage aggregate from `llm_requests` by `run_id`.
- Per-workflow concurrency and the global node cap follow §2.7.
- Trigger runs carry an idempotency key; a duplicate within the window returns
  the original `run_id`.

### 3.10 The security boundary of a run

- A run is **not** a privilege escalation. Its effective permissions are the
  intersection of:
  1. the invoking actor's grants (user session, agent session, trigger actor),
  2. the workflow's reviewed `permissions` block,
  3. the kernel's global policy and capability state.
- Trigger runs execute as `trigger:<workflow-id>` with exactly the reviewed
  permission set; any node needing more fails with `permission_denied`.
- Authoring or importing a workflow that requests permissions shows the review
  card before it can be enabled or triggered. AI-authored workflows follow the
  same path; the AI cannot approve on the user's behalf.
- Secrets are resolved at execution time and redacted from every persisted
  record.
- `code` runs in the Phase 4 sandbox; `http` respects domain scopes; `tool`
  and `agent` apply their own permission categories.

---

## 4. The visual editor

### 4.1 Surfaces and navigation

Per `UI.md`:

- **Sidebar** (§5.4): workflow list, recent runs, triggers, and the node
  palette while editing.
- **Main area** (§6): a **Workflow** tab (canvas) and a **Workflow run** tab
  (trace). Both are normal tabs and can be split.
- **Preset** (§3.3): `Workflow` = canvas + inspector.
- **Palette** (§11): "New workflow", "Run workflow", "Run with dry run",
  "Publish as app", "Open debugger", "Show run history", "Import app…".

### 4.2 Canvas

Built with **React Flow** (`@xyflow/react`, MIT). `WORKFLOW.md` §9.1 defines
the interactions; `style/patterns.html` pattern 06 defines the look.

- Dotted grid (22 px), translucent node cards (`--color-glass-strong`),
  rounded `--r-md`, `--sh-md`, hover `--color-border-3`, selection
  `--color-accent-line` plus an accent halo.
- Ports: 9 px circles, input neutral, output accent; edges are SVG curves,
  active edges `--color-accent` at 2 px, inactive `--color-border-2`.
- Minimap and inspector float inside the canvas frame and never overlap the
  node under edit.
- Interactions: drag from the palette, connect ports, multi-select, box
  select, copy/paste, duplicate, delete, undo/redo (50-step command stack),
  snap to grid, alignment guides, zoom/pan/fit, keyboard nudging.
- Undo/redo covers graph edits, inspector edits, and YAML-pane edits; saves
  are not coalesced with edits.
- Large graphs: virtualise off-screen nodes beyond 300; the minimap stays
  live.

### 4.3 Node palette

- Categories: Triggers, Models, Agents, Tools, Data, Flow, External, Custom.
- Search by name and description; drag to the canvas or click to insert at the
  centre.
- Plugin nodes come from the `workflow.node` slot/manifest and are grouped
  under Custom with the plugin name.
- Disabled capabilities hide their node types; a workflow that uses a node
  type whose capability is off fails validation with a clear message rather
  than silently hiding it.

### 4.4 Node inspector

- Right panel, schema-driven forms generated from the node type's JSON Schema
  (built-in or plugin): strings, numbers, booleans, enums, arrays, objects,
  prompt textareas, JSON editors.
- Template fields get autocomplete over context keys, variables, built-ins,
  and secrets (secrets by name only).
- Live validation with inline errors; saving is blocked while errors exist.
- Multi-select shows the common subset and applies changes to all.
- Node actions: duplicate, delete, disable (comment out), "run up to here"
  (debug).

### 4.5 YAML and canvas round-trip

- YAML is the source of truth; the canvas is a renderer (`WORKFLOW.md` §10.2).
- A split pane shows YAML with schema validation and autocompletion (Monaco,
  already in the desktop shell).
- Saving from either view validates, writes the file, increments the version,
  and refreshes the other view.
- External edits show a non-blocking banner with diff, reload, and discard.
- A YAML edit that would produce an invalid graph cannot be saved; the pane
  marks the offending line.

### 4.6 Debugger

`WORKFLOW.md` §11.

- Breakpoints on nodes (before/after); the run pauses in `paused` with a debug
  session id.
- Controls: start debug run, step node, continue, stop, inspect, replay.
- Context inspector: JSON tree with search, watch list, copy path; large
  values show as artifact references with a preview.
- Step view: inputs, outputs, logs, timing, attempt, error.
- Replay from a step: choose a checkpoint, optionally edit the context, and
  re-run the remainder. Replay creates a new run linked by `parent_run_id`.
- Dry-run mode is available from the debugger and from the toolbar.
- Commands: `workflow.debug.*` (Appendix A), gated by
  `capability.workflow-debugger`.

### 4.7 Run history and trace

- Run list with filters (workflow, status, trigger, date), searchable and
  paged.
- Trace view: the graph with per-node status colours (pending, running, done,
  failed, skipped), elapsed time on each node, an inspector for the selected
  node, logs, aggregated cost and tokens, and a replay action.
- Live updates while a run executes, driven by `workflow.*` events.

### 4.8 Versioning

- Every save creates a version; the history lists author, time, digest, and an
  optional message.
- Diff uses the Phase 3 diff viewer (YAML text); the canvas shows the
  structural diff of added, removed, and changed nodes.
- Restore creates a new version (history is never rewritten).
- Runs pin their version; "re-run with latest" is explicit.

### 4.9 Design-system law, accessibility, i18n, shortcuts

- **The styleguide is law.** New canvas components are added to
  `style/components.css` **first**, then mirrored byte-compatibly into
  `apps/desktop/src/styles/components.css`. Pattern 06's `.workflow`,
  `.wf-node*`, `.wf-port`, `.wf-minimap`, `.wf-inspector*` are promoted into
  the canonical component set with tokens only — no literal colours, spacing,
  radii, or timings.
- Every element uses `style/` tokens (`--color-*`, `--r-*`, `--sh-*`, `--t`).
  React Flow's default theme is overridden; no vendor CSS leaks.
- **Accessibility:** nodes are focusable with roles and labels; arrow keys
  nudge, Enter selects, Delete removes, Escape cancels a connection; focus is
  restored after panel changes; `prefers-reduced-motion` disables edge
  animations.
- **i18n:** every string goes through `t()`; new keys live under `workflows.*`
  in `apps/desktop/src/i18n/en.ts` and the `MessageKey` union.
- **Shortcuts:** `Ctrl+S` save, `Ctrl+Z` / `Ctrl+Shift+Z` undo/redo,
  `Ctrl+Enter` run, `Ctrl+Shift+Enter` dry run, `Ctrl+D` duplicate,
  `Delete` delete, `F5` continue (debugger), `F10` step.
- **Tests:** Vitest and Testing Library for every surface, with a mock kernel
  (the Phase 4 pattern); no real React Flow measurement APIs in jsdom.

---

## 5. Workflow → app

### 5.1 The app manifest

An optional `app:` block in the workflow YAML (`WORKFLOW.md` §14.2):

```yaml
app:
  id: rss-digest-app
  name: RSS Digest
  icon: newspaper
  description: Daily digest from your RSS feeds.
  inputs:
    - { id: feeds, type: array, label: Feed URLs, default: ["https://example.com/feed.xml"] }
    - { id: time, type: string, label: Delivery time, default: "07:00" }
  outputs:
    - { id: digest, type: markdown, label: Latest digest }
  layout:
    - { type: form, fields: [feeds, time] }
    - { type: button, label: Run now, action: run }
    - { type: output, ref: digest }
    - { type: history, limit: 10 }
```

- Input types: `string`, `number`, `boolean`, `array`, `select`, `markdown`;
  each has `label`, `default`, and optional `required` and `options`.
- Output types: `markdown`, `text`, `json`, `table`, `link`.
- Layout blocks: `form`, `button`, `output`, `history`, `markdown`, `text`,
  `divider`, `columns`.
- The manifest is validated against a JSON Schema; an unknown block or type
  fails save with `workflow_invalid`.
- Icons use the shell's icon set names, never arbitrary URLs.

### 5.2 Rendering and isolation

- An app renders in an isolated surface using the Phase 4 **shadow-root host**
  (tokens and components injected), with the virtual id
  `workflow-app:<app-id>`.
- **No code executes in an app.** The renderer interprets the manifest only:
  declarative layout, validated inputs, outputs, history. There is no plugin
  JS and no HTML injection; markdown is sanitised, and `html` output requires
  the existing `capability.html`.
- The app appears in the rail/sidebar app list and opens as a tab; deep link
  `fergents://app/<id>`.
- Live run progress uses workflow events; outputs stream into the `output`
  block when the run completes.

### 5.3 Publish, run, unpublish

- **Publish** (`workflow.app.publish`): validates the manifest, records the
  published version, emits `workflow.app.published`, and shows the app in the
  sidebar. It requires `workflow.edit`; if the workflow requests new
  permissions, the review card appears first.
- **Run** (`workflow.app.run`): validates inputs against the manifest, then
  starts a manual run with `trigger = { kind: "app", inputs }`, returning
  `{ run_id }`. The last inputs are remembered per app.
- **Unpublish** (`workflow.app.unpublish`): removes the sidebar entry, keeps
  runs and versions, and emits `workflow.app.unpublished`.
- A failure surfaces the error in the app with an "open run" action.

### 5.4 Export and import

- `workflow.app.export` produces a `.fergapp` archive via `crates/packages`:
  safe zip rules, deterministic digest, optional Ed25519 signature.
- `workflow.app.import` validates the archive, shows a review (permissions,
  trigger kinds, id collision), and creates a new workflow or a new version
  when the id exists.
- Marketplace distribution of apps is out of scope; Phase 4's marketplace
  covers plugins and skills, and an `app` kind is a later increment.

### 5.5 Permissions

- Apps never widen permissions: a run uses the app owner's reviewed set.
- Publishing an app from an AI-authored workflow still requires user approval.
- Exporting includes the reviewed permission block; importing re-reviews it.

---

## 6. Capabilities

### 6.1 `capability.workflow`

Registered in `crates/capabilities/src/catalogue.rs` with the same helper used
by every other entry:

| Field | Value |
|---|---|
| id | `capability.workflow` |
| name | Workflow |
| description | The DAG engine, triggers, runs, and workflow apps. |
| dependencies | `["capability.agents"]` |
| default enabled | `false` |
| hot toggle | `true` |

Disabling stops trigger listeners and schedulers, refuses new runs, pauses
active runs at the next checkpoint, refuses all `workflow.*` commands with
`capability_disabled`, and hides workflow surfaces. Files, versions, and run
history remain untouched. Re-enabling re-arms triggers (respecting the
catch-up policy) and leaves paused runs paused until the user resumes them.

### 6.2 `capability.workflow-debugger` (modified)

- Dependencies change from `["capability.agents"]` to
  `["capability.workflow"]`.
- `hot_toggle` becomes `true`.
- Description becomes "Breakpoints, step execution, context inspection, and
  replay for workflow runs."
- Disabling ends debug sessions; runs stay paused and can be resumed normally.

### 6.3 Gating and routing

- `required_capability` maps `workflow.debug.*` to
  `capability.workflow-debugger` and `workflow.*` to `capability.workflow`
  (order matters).
- The kernel refuses a command when the required capability or one of its
  dependencies is disabled.
- Workflow-provided agent tools follow the same routing.
- Capability changes are audited and hot-applied.

### 6.4 Agent tools

`crates/agents` gains tools backed by the same services:

| Tool | Permission | Capability |
|---|---|---|
| `workflow.list`, `workflow.get`, `workflow.schema` | `workflow.run` (read) | `capability.workflow` |
| `workflow.create`, `workflow.update`, `workflow.delete`, `workflow.duplicate` | `workflow.edit` | `capability.workflow` |
| `workflow.run`, `workflow.run.status`, `workflow.run.cancel` | `workflow.run` | `capability.workflow` |
| `workflow.debug.start`, `workflow.debug.step`, `workflow.debug.inspect` | `workflow.run` | `capability.workflow-debugger` |
| `workflow.app.publish`, `workflow.app.run` | `workflow.edit` / `workflow.run` | `capability.workflow` |

Read-only tools must not prompt in `Auto` mode; mutating tools map to
`workflow.edit` and keep the existing risk profile. A new `workflow.run`
permission category is registered (Appendix E).

The AI can therefore author, validate, run, and debug workflows
(`WORKFLOW.md` §16) under the same permission engine and review flow as a
human.

### 6.5 Settings

- **Features**: `Workflow` and `Workflow debugger` toggles, with dependency
  notes and a confirm dialog when disabling with active runs.
- **Workflows** (`UI.md` §16): default timezone, webhook port, default node
  concurrency, default run timeout, catch-up policy, artifact retention.
- **Permissions**: the `workflow.run` category appears in the policy editor.
- Settings changes are validated (port range, cron defaults) and applied
  without a restart where possible (a port change restarts the listener only).

---

## Impact

- **New crate**: `crates/workflow` (`fergents-workflow`), added to the
  workspace members and workspace dependencies.
- **New migration**: `migrations/global/0018_workflow.sql`.
- **Modified crates**: `capabilities` (two registrations), `permissions`
  (`workflow.run`), `api` (dispatch arms and routing), `agents` (tools),
  `core` (service wiring), `protocol` (regenerated), `fergentsd` (scheduler
  lifecycle), `supervisor` (recovery hook).
- **New protocol surface**: Appendix A; `protocol/schema/protocol.schema.json`
  and both protocol packages regenerated.
- **Desktop**: `apps/desktop/src/features/workflows` (list, canvas,
  inspector, palette, debugger, trace, versions, apps), `App.tsx` wiring,
  `i18n/en.ts` keys, `style/components.css` plus the mirrored app copy.
- **Docs**: `DATA_MODEL.md` (workflow tables), `SECURITY.md` (run identity,
  webhook auth, secret redaction), `STACK.md` (new crate and dependencies),
  `ARCHITECTURE.md` §9 (event topics), `UI.md` (workflow surfaces if more
  detail is needed), `CHANGELOG.md`, `ROADMAP.md` §9 checkboxes.
- **No breaking changes** to Phase 1–4 commands, events, or schemas.

---

## Decisions (resolved)

| # | Topic | Decision |
|---|---|---|
| D1 | Source of truth | YAML on disk is canonical; SQLite indexes metadata, versions, runs, steps, and triggers. External edits are reconciled by digest. |
| D2 | Crate | One new crate `crates/workflow`; no workflow code in `core` beyond wiring. |
| D3 | Engine model | In-process async scheduler, topological dispatch, deterministic ordering (rank, id), bounded parallelism. No external queue or broker. |
| D4 | Trigger scope | Six kinds in Phase 5. `mcp` and `chain` triggers deferred. |
| D5 | Webhook exposure | Loopback only, bearer token from the keychain, 404 on auth failure, 1 MiB body, 60 rpm, idempotency key. |
| D6 | Run identity | A run executes under the intersection of actor grants, workflow permissions, and kernel policy; trigger runs use `trigger:<workflow-id>`. |
| D7 | Context | One JSON context, node outputs namespaced by id, `$root` writes allowed; over 1 MiB spills to artifacts; 32 MiB live cap. |
| D8 | Templates | Closed `{{ }}` grammar with a documented filter set; strict missing-key failure; no eval. Conditions use a closed AST. |
| D9 | Node code | `code` compiles at save through the Phase 4 factory toolchain and runs in `crates/wasm` with Phase 4 caps. |
| D10 | Retries | Per-node policy with exponential backoff and jitter; retry classes; permission and validation errors are never retried. |
| D11 | Recovery | Boot marks in-flight runs `interrupted`; the user resumes from the last checkpoint; per-node `resume` policy. |
| D12 | Versioning | Every save creates an immutable version; runs pin a version; restore creates a new version. |
| D13 | Apps | Declarative manifest only, rendered in the Phase 4 shadow host; no app code; `.fergapp` export/import via `crates/packages`. |
| D14 | Capability defaults | `capability.workflow` off and hot; debugger off, hot, depends on workflow. |
| D15 | Permissions | Reuse `workflow.edit`; add `workflow.run`; reads never prompt in Auto; triggers are reviewed like installs. |
| D16 | Events | `workflow.*` topics only, extending the `ARCHITECTURE.md` §9 list; no self-subscription. |
| D17 | UI stack | `@xyflow/react` (MIT), wrapped by our own node and edge components styled only with tokens; no vendor theme. |
| D18 | Testing | No network, real LLM, or wall-clock waiting in CI: mock providers, a local HTTP server, paused Tokio time, fixture workflows. |

---

## ADDED Requirements

### Requirement: Phase 5 Capability Gating

The system SHALL register `capability.workflow` (off, hot, depends on
`capability.agents`) and gate every workflow command behind it.

#### Scenario: Workflow disabled

- **WHEN** `capability.workflow` is disabled
- **THEN** no trigger is armed, `workflow.*` returns `capability_disabled`,
  the canvas is unavailable, and existing runs stay persisted

#### Scenario: Debugger without engine

- **WHEN** `capability.workflow` is enabled but `capability.workflow-debugger`
  is disabled
- **THEN** `workflow.debug.*` returns `capability_disabled` while normal runs
  still work

#### Scenario: Disable with active runs

- **WHEN** the user disables `capability.workflow` while runs are active
- **THEN** new triggers are refused, running nodes finish, runs pause at the
  next checkpoint, and re-enabling does not auto-resume them

### Requirement: Workflow Document and Validation

The system SHALL parse, validate, and round-trip the YAML workflow format.

#### Scenario: Cycle rejected

- **WHEN** a document contains a top-level cycle
- **THEN** validation fails with `workflow_invalid` and an issue naming the
  participating nodes

#### Scenario: Unreachable node

- **WHEN** a node cannot be reached from any trigger
- **THEN** validation fails and the canvas highlights the node

#### Scenario: Unknown plugin node

- **WHEN** a workflow references a node type from a disabled plugin
- **THEN** validation fails with `workflow_node_unavailable` and the run does
  not start

### Requirement: Typed Node Execution

The system SHALL execute each node through resolve, permission, execute,
validate, and persist.

#### Scenario: Missing template key

- **WHEN** an input template references a path no upstream node wrote
- **THEN** the node fails with `workflow_template_missing` and the step records
  the path

#### Scenario: Tool permission denied

- **WHEN** a `tool` node calls a tool outside the run's permissions
- **THEN** the step fails with `permission_denied`, the run follows `on_error`,
  and an audit entry is written

#### Scenario: Loop bound

- **WHEN** a loop would exceed `max_iterations`
- **THEN** the node fails with `workflow_iteration_limit` instead of spinning

### Requirement: Trigger Runtime

The system SHALL arm, fire, and disarm cron, webhook, file, event, manual, and
AI triggers for enabled workflows.

#### Scenario: Cron after restart

- **WHEN** the kernel restarts around a scheduled fire
- **THEN** the persisted next-fire time is respected and the catch-up policy
  decides whether to skip, run once, or run all missed fires

#### Scenario: Bad webhook token

- **WHEN** a request presents a missing or incorrect bearer token
- **THEN** the listener returns 404, creates no run, and emits no
  `workflow.triggered`

#### Scenario: File change outside the project

- **WHEN** a watched glob resolves through a symlink outside the project root
- **THEN** no run fires

### Requirement: Durable Execution

The system SHALL persist runs, steps, and checkpoints, and recover them after
a crash.

#### Scenario: Crash mid-run

- **WHEN** the kernel stops while a node is running
- **THEN** on boot the run is `interrupted`, the step is `interrupted`, and the
  run waits for an explicit resume

#### Scenario: Resume from checkpoint

- **WHEN** the user resumes an interrupted run
- **THEN** completed nodes are skipped, the interrupted node follows its
  `resume` policy, and the run keeps its version and budget

#### Scenario: Cancellation

- **WHEN** the user cancels a run
- **THEN** running nodes stop within the grace period, pending nodes are
  skipped, the run becomes `cancelled`, and no compensation runs unless
  configured

### Requirement: Visual Editor

The system SHALL provide a React Flow canvas, a schema-driven inspector, and a
YAML pane that remain in sync.

#### Scenario: Drag a node and connect it

- **WHEN** the user drags a node from the palette and connects a port
- **THEN** the graph updates, validation runs, and saving writes YAML and a new
  version

#### Scenario: Inspector validation

- **WHEN** the user enters an invalid cron expression in the inspector
- **THEN** the field shows an inline error and saving is blocked

#### Scenario: External edit

- **WHEN** the YAML file changes on disk while the canvas is open
- **THEN** a banner offers diff, reload, or discard without losing unsaved
  edits

### Requirement: Debugger and Trace

The system SHALL support breakpoints, step execution, context inspection, and
replay.

#### Scenario: Step through a run

- **WHEN** a debug run pauses at a breakpoint
- **THEN** the panel shows the current node, context tree, inputs, outputs, and
  logs, and `workflow.debug.step` executes exactly one node

#### Scenario: Replay from a step

- **WHEN** the user replays a failed run from a checkpoint with an edited
  context
- **THEN** a new run is created with `parent_run_id`, pinned to the same
  version

### Requirement: Workflow Apps

The system SHALL publish a workflow as a declarative mini-app and run it with
validated inputs.

#### Scenario: Publish

- **WHEN** a workflow with a valid `app:` manifest is published
- **THEN** it appears in the sidebar, `workflow.app.published` is emitted, and
  the app renders in the shadow host

#### Scenario: Invalid input

- **WHEN** an app form submits a value that violates the manifest schema
- **THEN** the form blocks submission with an inline error and no run starts

#### Scenario: App cannot escalate

- **WHEN** an app run reaches a node outside the reviewed permissions
- **THEN** the node fails with `permission_denied` and the app shows the error
  with a link to the run

### Requirement: Phase 5 Protocol and Schema

The system SHALL expose the commands of Appendix A and the schema of
Appendix B.

#### Scenario: Unknown command

- **WHEN** a client calls an unimplemented `workflow.*` method
- **THEN** the API returns `method_not_found` without side effects

#### Scenario: Fresh database

- **WHEN** the kernel starts on a database without `0018`
- **THEN** the migration applies idempotently and no workflow data is lost

---

## MODIFIED Requirements

### Requirement: Capability Registry (Phase 2)

`capability.workflow` is added; `capability.workflow-debugger` changes its
dependencies, hot-toggle behaviour, and description (Phase 5 §6).

### Requirement: Permission Engine (Phase 2)

`workflow.run` joins the category set, `for_tool` refines the workflow tool
mapping, and trigger runs resolve an explicit `trigger:<workflow-id>`
identity.

### Requirement: Agent Runtime and Tools (Phase 2)

The runtime exposes the workflow tools of §6.4 and includes the workflow
summary (count, running runs) in the context summary.

### Requirement: Protocol Dispatch (Phase 3)

`required_capability` gains the `workflow.debug.*` and `workflow.*` routes in
that order.

### Requirement: Settings (Phase 3)

Settings gains the **Workflows** category and the two feature toggles.

### Requirement: Command Palette (Phase 3)

Palette entries for creating, running, publishing, and debugging workflows,
grouped under `palette.group.workflow`.

### Requirement: Reversibility and Audit (Phase 2)

Workflow create, update, delete, permission review, run control, and trigger
changes are audit entries; deleting a workflow moves its file to trash.

---

## REMOVED Requirements

None.

---

## Dependencies

| Dependency | Use | Notes |
|---|---|---|
| `serde_yaml` | workflow parsing | already a workspace dependency |
| `serde_json` | context, steps, events | existing |
| `chrono` | timestamps | existing |
| `chrono-tz` | cron time zones | add if not already present |
| `cron` | 5-field schedule parsing | pick one MIT/Apache crate and pin it |
| `notify` | file triggers | existing (Phase 4 dev reload) |
| `reqwest` | `http` node | existing workspace dependency |
| `wasmtime` | `code` node | existing `crates/wasm` |
| `jsonschema` | document and app validation | reuse the Phase 4 validation approach |
| `uuid` | run and step ids | existing |
| `@xyflow/react` | canvas | MIT; pin the major version |

No new hosted service, database, or broker is introduced.

---

## Milestones

| # | Milestone | Exit criterion |
|---|---|---|
| 5.1 | Document model, schema, validation, templates, expressions | A YAML workflow validates and a dry run resolves every template |
| 5.2 | Engine and run store | Manual runs execute, persist, and trace |
| 5.3 | Triggers (cron, webhook, file, event, manual, AI) | Each trigger fires a run, with auth and limits tested |
| 5.4 | Recovery, retries, cancellation, budgets, human nodes | Interrupted runs resume; budgets pause; approvals work |
| 5.5 | Protocol and capability wiring | Capability on/off gates every command; UI reads state |
| 5.6 | Canvas and inspector | Build and save a graph visually; YAML round-trips |
| 5.7 | Debugger and trace | Step, inspect, replay from a step |
| 5.8 | Versioning and run history | Diff, restore, and pinned runs |
| 5.9 | Workflow apps | Publish, run, unpublish, export/import |
| 5.10 | Custom nodes and agent tools | Plugin node types render and execute; AI tools gated |
| 5.11 | Polish and docs | Styleguide, a11y, i18n, `WORKFLOW.md`/`DATA_MODEL.md` aligned |

---

## Acceptance criteria (ROADMAP §9)

- [ ] Build a workflow visually, run it, and see the trace.
- [ ] Trigger a workflow via cron and via webhook.
- [ ] A workflow calls an agent, which calls a tool, which calls MCP.
- [ ] Replay a failed run from a step.
- [ ] Plugins can add custom node types.
- [ ] `WORKFLOW.md` matches the implementation.

---

## Appendix A: Phase 5 protocol surface

| Group | Commands |
|---|---|
| Document | `workflow.list`, `workflow.get`, `workflow.schema`, `workflow.validate`, `workflow.create`, `workflow.update`, `workflow.delete`, `workflow.duplicate`, `workflow.reload` |
| Versions | `workflow.version.list`, `workflow.version.get`, `workflow.version.restore` |
| Runs | `workflow.run`, `workflow.run.get`, `workflow.run.list`, `workflow.run.steps`, `workflow.run.cancel`, `workflow.run.pause`, `workflow.run.resume`, `workflow.run.approve`, `workflow.run.reject`, `workflow.run.replay`, `workflow.run.export` |
| Debug | `workflow.debug.start`, `workflow.debug.step`, `workflow.debug.continue`, `workflow.debug.inspect`, `workflow.debug.stop` |
| Triggers | `workflow.trigger.list`, `workflow.trigger.enable`, `workflow.trigger.disable`, `workflow.trigger.test`, `workflow.trigger.rotate_secret` |
| Apps | `workflow.app.list`, `workflow.app.publish`, `workflow.app.unpublish`, `workflow.app.run`, `workflow.app.export`, `workflow.app.import` |

Conventions follow Phase 4: `{ run_id }` for asynchronous work, structured
errors with a stable `code`, and events for progress. `workflow.run` returns as
soon as the run is persisted.

Error codes: `workflow_invalid`, `workflow_not_found`, `workflow_conflict`,
`workflow_version_conflict`, `workflow_queue_full`, `workflow_node_unavailable`,
`workflow_template_missing`, `workflow_template_invalid`, `workflow_cycle`,
`workflow_iteration_limit`, `workflow_budget_exceeded`, `workflow_interrupted`,
`workflow_trigger_auth`, `workflow_run_not_resumable`.

---

## Appendix B: Global schema (normative)

`migrations/global/0018_workflow.sql`, idempotent DDL:

```sql
CREATE TABLE IF NOT EXISTS workflows (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    version     TEXT    NOT NULL DEFAULT '0.1.0',
    path        TEXT    NOT NULL,
    digest      TEXT    NOT NULL,
    app         TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_versions (
    id          TEXT    PRIMARY KEY,
    workflow_id TEXT    NOT NULL,
    version     INTEGER NOT NULL,
    yaml        TEXT    NOT NULL,
    digest      TEXT    NOT NULL,
    author      TEXT,
    message     TEXT,
    created_at  INTEGER NOT NULL,
    UNIQUE (workflow_id, version)
);

CREATE TABLE IF NOT EXISTS workflow_runs (
    id                 TEXT    PRIMARY KEY,
    workflow_id        TEXT    NOT NULL,
    workflow_version   INTEGER NOT NULL,
    parent_run_id      TEXT,
    trigger_kind       TEXT    NOT NULL,
    trigger_id         TEXT,
    state              TEXT    NOT NULL,
    actor              TEXT    NOT NULL,
    dry_run            INTEGER NOT NULL DEFAULT 0,
    inputs             TEXT,
    context            TEXT,
    error              TEXT,
    started_at         INTEGER,
    finished_at        INTEGER,
    created_at         INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_steps (
    id          TEXT    PRIMARY KEY,
    run_id      TEXT    NOT NULL,
    node_id     TEXT    NOT NULL,
    iteration   INTEGER,
    state       TEXT    NOT NULL,
    attempt     INTEGER NOT NULL DEFAULT 1,
    inputs      TEXT,
    outputs     TEXT,
    error       TEXT,
    started_at  INTEGER,
    finished_at INTEGER
);

CREATE TABLE IF NOT EXISTS workflow_checkpoints (
    run_id     TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    node_id    TEXT,
    context    TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (run_id, seq)
);

CREATE TABLE IF NOT EXISTS workflow_triggers (
    workflow_id   TEXT    NOT NULL,
    trigger_id    TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_fired_at INTEGER,
    next_fire_at  INTEGER,
    last_error    TEXT,
    PRIMARY KEY (workflow_id, trigger_id)
);

CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow
    ON workflow_runs (workflow_id, created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_run
    ON workflow_steps (run_id, node_id);
CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow
    ON workflow_versions (workflow_id, version);
```

`workflows.app` is the JSON app manifest. Run context snapshots live in
`workflow_checkpoints`; large values are artifact references.

---

## Appendix C: Node config schemas (summary)

- `trigger`: `trigger_id` (required).
- `llm`: `model`, `prompt` (required); `system`, `temperature`, `max_tokens`,
  `json` (optional).
- `tool`: `tool` (registry id), `args` (object; templates allowed).
- `agent`: `agent`, `task` (required); `budget`, `tools` (optional, narrowed).
- `code`: `language` (`typescript`/`rust`), `source`, `memory_mb`, `fuel`.
- `http`: `method`, `url` (required); `headers`, `body`, `max_bytes`.
- `condition`: `expr` (closed AST / expression string).
- `loop`: `items` or `count`; `max_iterations`; `parallel`; `body` (node ids).
- `subworkflow`: `workflow`, `version` (`latest` or integer), `inputs`.
- `human`: `prompt`, `options`, `timeout_s`, `on_timeout`.
- `transform`: `ops` (`pick`/`map`/`filter`/`merge`/`sort`), `template`.
- `end`: `result` (template).

The full JSON Schemas live in `workflows/schema/nodes/<type>.schema.json` and
are the source for the inspector forms.

---

## Appendix D: Template grammar

```text
template := text | "{{" expr "}}"
expr     := path ("|" filter)*
path     := (ident | "$root") ("." ident | "[" integer "]")*
ident    := [a-z][a-z0-9_]*
filter   := "length" | "upper" | "lower" | "json"
          | "join(" string ")" | "default(" literal ")" | "date(" string ")"
```

- No arithmetic, no function calls, no indexing beyond integer array access.
- `{{ secrets.<name> }}` is allowed only in node configs and is redacted from
  logs and persisted payloads.
- A template containing an unknown filter fails validation at save.

---

## Appendix E: Security constraints

- **Secrets**: resolved at execution time; never persisted, never emitted,
  never included in step inputs/outputs (redacted placeholders instead).
- **Webhook**: loopback only; bearer token; 404 on failure; 1 MiB body;
  60 rpm; idempotency key; audit on every accepted request.
- **Run identity**: intersection of actor grants, workflow permissions, and
  kernel policy; no widening; trigger runs use `trigger:<workflow-id>`.
- **Filesystem**: `file` triggers are confined to the project root after
  canonicalisation; `code` and `http` nodes have no ambient FS or network.
- **Code**: compiled at save via the Phase 4 toolchain, executed in
  `crates/wasm` with Phase 4 caps; no dynamic download at run time.
- **Apps**: declarative only; sanitised markdown; HTML output gated by
  `capability.html`; no URL-driven icons.
- **Audit**: create/update/delete, permission review, run start/stop,
  trigger enable/disable/secret rotation, and app publish/unpublish are
  audited with actor, workflow, run, and outcome.

---

## Appendix F: Testing strategy

**Rust (`crates/workflow`)**

- Parser and validator: cycles, orphans, ports, ids, unknown types, plugin
  node availability, subworkflow depth and cycles.
- Templates: path resolution, filters, strict missing keys, secret redaction,
  artifact spill.
- Expressions: every operator, precedence, type errors.
- Scheduler: deterministic order, parallelism bounds, fan-in, skipped
  propagation.
- Retries and timeouts with paused Tokio time.
- Checkpoints: crash simulation, resume policies, idempotent triggers.
- Triggers: cron across DST and restarts, webhook auth/size/rate/idempotency
  against a local server, file watcher debounce and containment, event
  filters.
- Human nodes: approve, reject, timeout policies.
- Apps: manifest validation, input mapping, export/import digest and review.
- Migration: apply `0018` twice, assert idempotence and indexes.

**Desktop (Vitest + Testing Library, mock kernel)**

- Workflow list, canvas render from YAML, drag/drop, connect, undo/redo.
- Inspector validation and schema forms; template autocomplete.
- YAML round-trip and external-change banner.
- Run trace, debugger step, context tree, replay dialog.
- App form validation, output render, history.
- Capability-off states for the surface and the palette.
- Styling: no literal colours in new component tests (class token check).

**End to end (CI, no network)**

- Author the RSS digest workflow, dry run it, run it manually, inspect the
  trace.
- Cron trigger with an injected clock; webhook against the loopback listener;
  file trigger against a temp project.
- Fail a node, replay from the previous step, assert the linked run.
- Publish the workflow as an app, run it with inputs, export and re-import.

---

## Appendix G: Out of scope (later phases)

- Marketplace listing, ratings, payments, or hosted distribution of apps.
- Remote or distributed execution, cloud scheduling, multi-user workspaces.
- `mcp` and `chain` triggers.
- `merge` and `delay` node types (fan-in is implicit; delays can use `code`).
- Full JSONPath, arbitrary JavaScript in templates or expressions.
- Visual dashboard widgets (Phase 6).
- Sync and mobile surfaces (Phase 7).
- Workflow-level collaborative editing.

---

*End of Phase 5 spec.*
