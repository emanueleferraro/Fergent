# Workflow

> A visual and YAML-based automation engine, integrated with agents, MCP,
> the brain, and the event bus. In v2, workflows can be published as native
> mini-apps.

**Audience:** developers, AI developers.
**Companion docs:** `AGENTS.md`, `MCP.md`, `PLUGINS.md`, `DASHBOARD.md`.

---

## Table of contents

1. [Concept](#1-concept)
2. [The DAG model](#2-the-dag-model)
3. [Node types](#3-node-types)
4. [Edges and control flow](#4-edges-and-control-flow)
5. [Triggers](#5-triggers)
6. [Execution model](#6-execution-model)
7. [State and context](#7-state-and-context)
8. [Error handling](#8-error-handling)
9. [The visual editor](#9-the-visual-editor)
10. [The YAML format](#10-the-yaml-format)
11. [Debugger](#11-debugger)
12. [Run history](#12-run-history)
13. [Versioning](#13-versioning)
14. [Workflow→app (v2)](#14-workflowapp-v2)
15. [Custom nodes](#15-custom-nodes)
16. [AI-managed workflows](#16-ai-managed-workflows)
17. [Observability](#17-observability)
18. [Examples](#18-examples)

---

## 1. Concept

A workflow is a **directed acyclic graph** (DAG) of **nodes** connected by
**edges**, with one or more **triggers**. Nodes do things (call an LLM, run
an agent, fetch a URL, transform data). Edges pass data and control.

Workflows are:

- **Visual**: edited in a drag-and-drop canvas.
- **Textual**: stored as YAML, round-trippable with the visual editor.
- **Integrated**: nodes can call agents, MCP tools, brain operations,
  plugins, and other workflows.
- **Observable**: every run is traced, replayable, and auditable.

---

## 2. The DAG model

### 2.1 Structure

```
trigger → node A → node B ─┬→ node D → node F (end)
                           └→ node E ─┘
```

- **Nodes**: units of work.
- **Edges**: data + control flow.
- **Ports**: named inputs/outputs on each node.
- **Context**: the shared state passed along edges.

### 2.2 Constraints

- Acyclic (no loops as edges; use `loop` nodes instead).
- At least one trigger.
- At least one terminal node.
- No orphan nodes (all reachable from a trigger).

Cycles are allowed **inside** a `loop` node, which is a subgraph.

---

## 3. Node types

### 3.1 Built-in

| Type | Purpose | Inputs | Outputs |
|---|---|---|---|
| `trigger` | Entry point | — | `event` |
| `llm` | Call a model | `prompt`, `context` | `text`, `usage` |
| `agent` | Run an agent | `task`, `context` | `result`, `artifacts` |
| `tool` | Call a tool | `args` | `result` |
| `code` | Run user code (WASM/TS) | `input` | `output` |
| `http` | HTTP request | `url`, `method`, `body` | `status`, `body`, `headers` |
| `condition` | Branch | `value` | `true`, `false` |
| `loop` | Iterate | `items`, `body` | `results` |
| `subworkflow` | Call another workflow | `inputs` | `outputs` |
| `human` | Wait for approval | `prompt` | `decision`, `notes` |
| `transform` | Data manipulation | `input` | `output` |
| `merge` | Combine branches | `inputs` | `output` |
| `delay` | Wait | `duration` | — |
| `end` | Terminal | `result` | — |

### 3.2 Node anatomy

Every node has:

- **id**: unique within the workflow.
- **type**: one of the above.
- **config**: type-specific settings.
- **inputs**: mapping from context to node inputs.
- **outputs**: mapping from node outputs to context keys.
- **retry**: retry policy.
- **timeout**: max duration.
- **on_error**: error handling strategy.

### 3.3 Example node

```yaml
- id: summarize
  type: llm
  config:
    model: ollama:qwen2.5:7b
    prompt: |
      Summarize the following text in 3 bullets:
      {{ context.text }}
  inputs:
    text: "{{ trigger.body }}"
  outputs:
    summary: "{{ result.text }}"
  retry:
    max: 2
    backoff_ms: 500
  timeout_s: 60
  on_error: continue
```

---

## 4. Edges and control flow

### 4.1 Data edges

Edges carry data from one node's output to another's input. The mapping is
explicit in the target node's `inputs`.

### 4.2 Control edges

Control flow is implicit in the DAG: a node runs when all its inputs are
ready. Use `condition` nodes for branching, `loop` for iteration, `merge`
for joining.

### 4.3 Fan-out / fan-in

- **Fan-out**: one node's output feeds multiple nodes.
- **Fan-in**: multiple nodes feed one node (which waits for all).

### 4.4 Conditional branches

```
       ┌─ condition ─┬─ true ─→ node A
node X ┤             └─ false → node B
```

The `condition` node evaluates an expression and activates one branch.

---

## 5. Triggers

### 5.1 Types

| Trigger | Description | Config |
|---|---|---|
| `manual` | Run from the UI | — |
| `cron` | Scheduled | `schedule` (cron), `timezone` |
| `webhook` | HTTP request | `path`, `method`, `auth` |
| `file` | File change | `path`, `events` |
| `event` | Kernel event | `topic`, `filter` |
| `mcp` | MCP tool call | `server`, `tool` |
| `ai` | Agent-invoked | `agent`, `tool_name` |
| `chain` | End of another workflow | `workflow_id` |

### 5.2 Multiple triggers

A workflow can have multiple triggers. Each run records which trigger fired.

### 5.3 Example

```yaml
triggers:
  - kind: cron
    schedule: "0 7 * * *"
    timezone: "Europe/Rome"
  - kind: webhook
    path: "/hooks/rss-digest"
    method: POST
    auth: { token: "$KEYCHAIN:rss_webhook" }
  - kind: manual
```

---

## 6. Execution model

### 6.1 Scheduling

The engine builds a topological order of nodes and executes them as their
dependencies complete. Independent nodes run in parallel (bounded by a
concurrency limit, default 4).

### 6.2 Run lifecycle

```
queued → running → (paused) → done | failed | cancelled
```

Every transition is persisted and emitted as an event.

### 6.3 State

Each run has:

- **Context**: the shared key-value store passed along edges.
- **Node states**: pending, running, done, failed, skipped.
- **Artifacts**: files or data produced by nodes.
- **Metrics**: duration, cost, tokens.

### 6.4 Concurrency

- Node-level: independent nodes run in parallel.
- Run-level: multiple runs of the same workflow can overlap (configurable).
- Global: a max concurrent runs limit per workflow (default 1 for
  stateful workflows, unlimited for stateless).

### 6.5 Cancellation

A run can be cancelled:

- From the UI.
- By a trigger event.
- By a timeout.
- By a `stop` signal from a node.

Cancellation is graceful: running nodes finish, pending nodes are skipped.

### 6.6 Persistence

Every run and every step is persisted in `workflow_runs` and
`workflow_steps`. On kernel restart, in-flight runs are recovered:

- `running` nodes are marked `interrupted`.
- The run is paused.
- The user can resume or cancel.

---

## 7. State and context

### 7.1 The context object

The context is a JSON object shared across nodes:

```json
{
  "trigger": { "body": "..." },
  "summarize": { "summary": "..." },
  "fetch": { "status": 200, "body": "..." }
}
```

Nodes read from it (via `inputs`) and write to it (via `outputs`).

### 7.2 Templating

Values are templated with `{{ }}`:

- `{{ trigger.body }}` — a context value.
- `{{ result.text }}` — the current node's result.
- `{{ env.API_KEY }}` — an environment variable.
- `{{ secrets.github_token }}` — a secret.
- `{{ now }}`, `{{ today }}` — built-ins.
- `{{ json path }}` — JSONPath.

### 7.3 Scoping

By default, node outputs are namespaced under the node id. You can write to
the root context with `outputs: { "$root.key": "..." }`.

### 7.4 Large data

For data larger than a threshold (default 1 MB), the context stores a
reference instead of the value. References are resolved on demand.

---

## 8. Error handling

### 8.1 Per-node

```yaml
on_error: fail | continue | retry | fallback
```

- **fail**: the run fails.
- **continue**: the node is marked failed, but the run continues (downstream
  nodes get an error marker).
- **retry**: retry per the `retry` policy.
- **fallback**: run a fallback node.

### 8.2 Retry policy

```yaml
retry:
  max: 3
  backoff_ms: 500
  backoff_multiplier: 2
  jitter: true
  retry_on: [timeout, rate_limit, network]
```

### 8.3 Compensation

For workflows that mutate external state, nodes can declare a
**compensation** node that runs if the workflow fails after this node:

```yaml
- id: create_issue
  type: tool
  config: { tool: mcp:github:create_issue, ... }
  compensation:
    type: tool
    config: { tool: mcp:github:close_issue, ... }
```

On failure, compensations run in reverse order.

### 8.4 Global error handler

A workflow can have a top-level `on_error` block that runs on any failure:

```yaml
on_error:
  - type: tool
    config: { tool: system.notify, args: { title: "Workflow failed" } }
```

---

## 9. The visual editor

### 9.1 Canvas

Built with React Flow. Features:

- Drag-and-drop nodes from a palette.
- Connect nodes by dragging ports.
- Zoom, pan, minimap.
- Multi-select, copy/paste, undo/redo.
- Snap to grid, alignment guides.

### 9.2 Node inspector

A side panel with schema-driven forms for the selected node's config. Live
validation.

### 9.3 Live preview

The editor can run the workflow in "dry run" mode (LLM calls mocked,
side effects skipped) to validate the DAG.

### 9.4 Keyboard shortcuts

- `Ctrl+S`: save.
- `Ctrl+Z` / `Ctrl+Shift+Z`: undo / redo.
- `Ctrl+Enter`: run.
- `Ctrl+D`: duplicate selection.
- `Delete`: delete selection.

---

## 10. The YAML format

### 10.1 Structure

```yaml
id: rss-digest
name: RSS Digest
description: Fetch RSS feeds and produce a daily digest.
version: 1.0.0

variables:
  feeds:
    - https://example.com/feed.xml
  output_path: "inbox/digest-{{ today }}.md"

triggers:
  - kind: cron
    schedule: "0 7 * * *"
    timezone: Europe/Rome

nodes:
  - id: fetch
    type: http
    config:
      method: GET
      url: "{{ variables.feeds[0] }}"
    outputs:
      body: "{{ result.body }}"

  - id: summarize
    type: llm
    config:
      model: ollama:qwen2.5:7b
      prompt: |
        Summarize these RSS items into a daily digest:
        {{ fetch.body }}
    inputs:
      text: "{{ fetch.body }}"
    outputs:
      summary: "{{ result.text }}"

  - id: save
    type: tool
    config:
      tool: brain.write
      args:
        path: "{{ variables.output_path }}"
        content: "{{ summarize.summary }}"

edges:
  - from: fetch
    to: summarize
  - from: summarize
    to: save
```

### 10.2 Round-tripping

The visual editor and the YAML are two views of the same data. Saving from
either updates both. The YAML is the source of truth; the visual editor is a
renderer.

### 10.3 Validation

The YAML is validated against a JSON Schema. Errors are shown inline in the
editor.

---

## 11. Debugger

### 11.1 Features

- **Breakpoints**: pause before/after a node.
- **Step**: execute one node at a time.
- **Inspect**: view the context at any point.
- **Watch**: monitor specific context keys.
- **Replay**: re-run from a step, with the same or modified context.
- **Dry run**: execute without side effects.

### 11.2 UI

A debug panel shows:

- Current node.
- Context (JSON tree).
- Node inputs/outputs.
- Logs.
- Step controls.

### 11.3 Programmatic

The debugger is also available via the kernel API:

```json
{ "command": "workflow.debug.start", "params": { "workflow_id": "..." } }
{ "command": "workflow.debug.step" }
{ "command": "workflow.debug.inspect", "params": { "path": "context.summarize" } }
{ "command": "workflow.debug.continue" }
```

This lets agents debug workflows too.

---

## 12. Run history

Every run is stored with:

- Trigger, inputs, outputs.
- Per-node timing and status.
- Context snapshots.
- Logs.
- Cost and tokens.

The UI shows a list of runs with filters (status, date, workflow, trigger).
Clicking a run opens the trace view, with a replay option.

---

## 13. Versioning

Workflows are versioned. Every save creates a new version (stored in the
brain's git or a dedicated store). The UI shows a version history with diff
and rollback.

Running workflows use the version they were triggered with. Migrations
between versions are manual (or scripted).

---

## 14. Workflow→app (v2)

### 14.1 Concept

Any workflow can be **published as a mini-app**. A mini-app is:

- A workflow + a UI manifest.
- Rendered in an isolated view (like a plugin panel).
- Accessible from the sidebar.
- Shareable (export/import).

### 14.2 UI manifest

```yaml
app:
  id: rss-digest-app
  name: RSS Digest
  icon: 📰
  description: Daily digest from your RSS feeds.

  inputs:
    - id: feeds
      type: array
      label: Feed URLs
      default: ["https://example.com/feed.xml"]
    - id: time
      type: string
      label: Delivery time
      default: "07:00"

  outputs:
    - id: digest
      type: markdown
      label: Latest digest

  layout:
    - type: form
      fields: [feeds, time]
    - type: button
      label: Run now
      action: run
    - type: output
      ref: digest
    - type: history
      limit: 10
```

### 14.3 Publishing flow

1. User clicks "Publish as app" in the workflow editor.
2. A UI manifest is generated (or edited).
3. The app appears in the sidebar.
4. Users (or the AI) can run it with a click.

### 14.4 Sharing

Apps can be exported as `.fergapp` archives and imported by others. The
marketplace (post-v1) will distribute them.

---

## 15. Custom nodes

### 15.1 Plugin-registered nodes

Plugins can register new node types:

```toml
[plugin]
id = "com.example.rss"
...

[[nodes]]
id = "rss.fetch"
name = "Fetch RSS"
description = "Fetch items from an RSS feed."
category = "Network"

[nodes.input]
type = "object"
properties:
  url: { type = "string" }
  limit: { type = "integer", default = 10 }
required = ["url"]

[nodes.output]
type = "object"
properties:
  items: { type = "array" }
```

The node appears in the palette and can be used like any built-in node.

### 15.2 Code nodes

For one-off logic, the `code` node runs user-provided TypeScript or Rust
(compiled to WASM):

```yaml
- id: transform
  type: code
  config:
    language: typescript
    source: |
      export function run(input: { items: string[] }) {
        return { items: input.items.map(s => s.toUpperCase()) };
      }
```

Sandboxed, no network or FS unless permitted.

---

## 16. AI-managed workflows

The AI can:

- **Create** a workflow from a description.
- **Modify** an existing workflow (with diff preview).
- **Run** a workflow and report results.
- **Debug** a failed run.
- **Suggest** optimizations.

Example:

```
User: "Create a workflow that summarizes my inbox every morning at 8am."
Agent: [creates the workflow YAML]
Agent: [calls workflow.create]
Kernel: [validates, saves]
Agent: "Done. It will run daily at 8am. Here's the graph:"
Agent: [renders a preview]
```

The AI uses the same tools a human would, subject to the same permissions.

---

## 17. Observability

- **Traces**: one span per run, one child per node.
- **Metrics**: per workflow — runs, success rate, avg duration, cost.
- **Logs**: per node, structured.
- **Alerts**: on failure, on cost threshold, on duration threshold.

All visible in the dashboard (see `DASHBOARD.md`).

---

## 18. Examples

### 18.1 RSS digest

(see §10.1)

### 18.2 Daily journal

```yaml
id: daily-journal
name: Daily Journal
triggers:
  - kind: cron
    schedule: "0 21 * * *"
    timezone: Europe/Rome
nodes:
  - id: fetch_today
    type: tool
    config:
      tool: brain.search
      args:
        query: "today"
        filters: { tag: ["daily"] }
    outputs:
      notes: "{{ result.chunks }}"

  - id: summarize
    type: agent
    config:
      agent: writer
      task: "Summarize today's notes into a journal entry."
    inputs:
      context: "{{ fetch_today.notes }}"
    outputs:
      entry: "{{ result.text }}"

  - id: save
    type: tool
    config:
      tool: brain.write
      args:
        path: "journal/{{ today }}.md"
        content: "{{ summarize.entry }}"
```

### 18.3 On file change, review code

```yaml
id: auto-review
name: Auto Code Review
triggers:
  - kind: file
    path: "src/**/*.rs"
    events: [modify]
nodes:
  - id: diff
    type: tool
    config:
      tool: fs.diff
      args: { path: "{{ trigger.path }}" }
    outputs:
      diff: "{{ result.diff }}"

  - id: review
    type: agent
    config:
      agent: reviewer
      task: "Review this diff."
    inputs:
      diff: "{{ diff.diff }}"
    outputs:
      issues: "{{ result.issues }}"

  - id: notify
    type: tool
    config:
      tool: system.notify
      args:
        title: "Review: {{ trigger.path }}"
        body: "{{ review.issues | length }} issues found."
```

### 18.4 Webhook → agent → brain

```yaml
id: webhook-notes
name: Webhook to Notes
triggers:
  - kind: webhook
    path: "/hooks/note"
    method: POST
    auth: { token: "$KEYCHAIN:webhook_token" }
nodes:
  - id: process
    type: agent
    config:
      agent: librarian
      task: "Process this incoming note and file it appropriately."
    inputs:
      note: "{{ trigger.body }}"
    outputs:
      path: "{{ result.path }}"

  - id: respond
    type: tool
    config:
      tool: webhook.respond
      args: { status: 200, body: "Filed to {{ process.path }}" }
```

---

*End of `WORKFLOW.md`.*