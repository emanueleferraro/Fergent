# Agents

> The agent runtime: definitions, lifecycle, memory, sub-agents, orchestration,
> permissions, limits, and observability. This is the heart of Fergents.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md`, `LLM.md`, `MCP.md`, `SKILLS.md`,
`WORKFLOW.md`, `SECURITY.md`.

---

## Table of contents

1. [Philosophy](#1-philosophy)
2. [What an agent is](#2-what-an-agent-is)
3. [Anatomy of an agent definition](#3-anatomy-of-an-agent-definition)
4. [The agent file format](#4-the-agent-file-format)
5. [The agent loop](#5-the-agent-loop)
6. [Context management](#6-context-management)
7. [Tools](#7-tools)
8. [Memory](#8-memory)
9. [Sub-agents](#9-sub-agents)
10. [Multi-agent orchestration](#10-multi-agent-orchestration)
11. [Agent-to-agent communication](#11-agent-to-agent-communication)
12. [Permissions](#12-permissions)
13. [Limits and budgets](#13-limits-and-budgets)
14. [Lifecycle](#14-lifecycle)
15. [Persistence](#15-persistence)
16. [Built-in agents](#16-built-in-agents)
17. [Agent creation by AI](#17-agent-creation-by-ai)
18. [Observability](#18-observability)
19. [Failure modes and recovery](#19-failure-modes-and-recovery)
20. [Examples](#20-examples)
21. [Appendix: frontmatter schema](#21-appendix-frontmatter-schema)
22. [Appendix: built-in tools reference](#22-appendix-built-in-tools-reference)

---

## 1. Philosophy

Fergents treats agents as **first-class entities**, not ephemeral prompts.
An agent is:

- **A file** on disk. Human-readable. Git-versionable. AI-editable.
- **A persistent identity** with memory, stats, and history.
- **A capability set**: tools, skills, permissions.
- **A budget**: tokens, cost, time, tool calls.
- **A node in a graph**: it can spawn children, talk to peers, be called by
  workflows.

The design goal is **reversibility, transparency, and control**:

- Every action is logged.
- Every decision can be reviewed.
- Every modification is reversible.
- Every capability is explicit.
- The user can inspect, edit, duplicate, disable, or delete any agent.

And **the AI can do all of the above too** — subject to the same permission
system that governs any other action.

---

## 2. What an agent is

An agent is the combination of:

| Component | What it is | Where it lives |
|---|---|---|
| **Definition** | Identity, model, tools, skills, limits | `agents/<slug>.md` |
| **Session** | A conversation or task execution | `agent_sessions` table |
| **Messages** | The full history of a session | `agent_messages` table |
| **Memory** | Persistent facts, summaries, embeddings | `agent_memory` table |
| **Children** | Sub-agent sessions spawned by this agent | `agent_children` table |
| **Stats** | Token/cost/latency aggregates | `llm_requests` table |
| **Permissions** | What the agent can do | Session + definition + rules |

An agent without an active session is a **definition**. When invoked, it
becomes a **running agent** with a session.

---

## 3. Anatomy of an agent definition

Every agent definition has the following parts:

```
agents/researcher.md
├── YAML frontmatter
│   ├── id              (stable, unique, e.g. "researcher")
│   ├── name            (human-readable)
│   ├── description     (short, one line)
│   ├── version         (semver, for template compatibility)
│   ├── author          (optional)
│   ├── tags            (optional)
│   ├── model           (preferred model or route)
│   ├── model_fallback  (list)
│   ├── temperature
│   ├── max_tokens
│   ├── reasoning       (boolean, use extended thinking if available)
│   ├── tools           (list)
│   ├── skills          (list)
│   ├── mcp             (list of MCP servers this agent can use)
│   ├── permissions     (category overrides)
│   ├── limits          (tokens, cost, time, tool calls)
│   ├── memory          (settings)
│   ├── sub_agents      (allowed/denied, defaults)
│   ├── triggers        (optional: cron, event, webhook)
│   ├── icon            (emoji or path)
│   ├── color           (UI accent)
│   └── ui              (visibility, ordering)
└── Markdown body
    └── The system prompt (this is the agent's identity and instructions)
```

Nothing is hidden. Everything is in the file.

---

## 4. The agent file format

### 4.1 Full example

```markdown
---
id: researcher
name: Researcher
description: Deep research on a topic, with citations from the brain and the web.
version: 1.0.0
author: fergents
tags: [research, web, citations]

model: anthropic:claude-sonnet-4.5
model_fallback:
  - openai:gpt-4o
  - ollama:qwen2.5:14b
temperature: 0.3
max_tokens: 8192
reasoning: true

tools:
  - brain.search
  - brain.read
  - brain.write
  - web.fetch
  - web.search
  - mcp.call
  - code.run

skills:
  - com.fergents.summarize
  - com.fergents.cite

mcp:
  - brave-search
  - arxiv

permissions:
  fs.read: allow
  fs.write: prompt
  net.http: prompt
  shell.exec: deny

limits:
  max_tokens_per_run: 200000
  max_cost_usd_per_run: 2.00
  max_wall_time_s: 900
  max_tool_calls_per_run: 100
  max_sub_agents: 4

memory:
  episodic: true
  semantic: true
  working_window: 128000
  summarize_at: 0.75
  rag_top_k: 8

sub_agents:
  allow: true
  max_depth: 2
  default_tools: [brain.search, web.fetch]

triggers:
  - kind: manual
  - kind: cron
    schedule: "0 8 * * 1"

icon: 🔬
color: "#6B94D9"
ui:
  visible: true
  order: 10
---

# Researcher

You are the Researcher, an agent specialized in deep research.

## Identity

You are rigorous, skeptical, and precise. You prefer primary sources over
secondary ones. You never invent citations. When you are uncertain, you say
so.

## Behavior

1. **Clarify before acting.** If the task is ambiguous, ask one focused
   question before starting.
2. **Plan first.** For any task that will take more than 3 tool calls,
   produce a short plan (2-5 steps) and confirm it if the user is present.
3. **Cite everything.** Every factual claim must reference a source (a note
   in the brain, a URL, a document). Use the `cite` skill.
4. **Prefer the brain.** Before searching the web, search the brain.
5. **Summarize at the end.** Produce a structured summary: findings,
   sources, open questions.

## Output format

- Use Markdown.
- Sections: `## Findings`, `## Sources`, `## Open questions`.
- Cite as `[^1]` with a footnote list at the bottom.
- If the user asked for a note, write it to the brain using the
  `brain.write` tool, with the tag `#research`.

## Constraints

- Never modify files outside `inbox/` or `research/` without explicit
  permission.
- Never run shell commands.
- Never exceed the budget. If you approach it, stop and report.
```

### 4.2 Minimal example

```markdown
---
id: helper
name: Helper
description: A minimal general-purpose assistant.
model: ollama:llama3.1:8b
tools: [brain.search, brain.read]
---

You are a helpful assistant. Be concise.
```

### 4.3 Frontmatter validation

Every field is validated against a JSON Schema
(`schemas/agent.schema.json`). Invalid definitions are rejected at load with
a clear error. The UI surfaces validation errors inline.

---

## 5. The agent loop

### 5.1 Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        AGENT RUN                                 │
│                                                                  │
│  ┌──────────┐                                                    │
│  │  INPUT   │  user message, event, or workflow trigger          │
│  └────┬─────┘                                                    │
│       │                                                          │
│       ▼                                                          │
│  ┌──────────┐                                                    │
│  │ CONTEXT  │  assemble: system prompt + memory + RAG + history  │
│  │ BUILDER  │                                                    │
│  └────┬─────┘                                                    │
│       │                                                          │
│       ▼                                                          │
│  ┌──────────┐       ┌──────────┐       ┌──────────┐             │
│  │ PLANNER  │──────▶│ EXECUTOR │──────▶│ REFLECTOR│             │
│  └──────────┘       └────┬─────┘       └────┬─────┘             │
│       ▲                  │                  │                   │
│       │                  ▼                  │                   │
│       │           ┌──────────────┐          │                   │
│       │           │  TOOL CALL   │          │                   │
│       │           │  + PERMISSION│          │                   │
│       │           │  + EXECUTION │          │                   │
│       │           └──────┬───────┘          │                   │
│       │                  │                  │                   │
│       │                  ▼                  │                   │
│       │           ┌──────────────┐          │                   │
│       │           │  OBSERVATION │          │                   │
│       │           └──────┬───────┘          │                   │
│       │                  │                  │                   │
│       └──────────────────┴──────────────────┘                   │
│                          │                                       │
│                          ▼                                       │
│                    ┌──────────┐                                  │
│                    │  OUTPUT  │  message, side effects, done    │
│                    └──────────┘                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Phases

#### 5.2.1 Input

The run starts from one of:

- A user message in the chat UI.
- An event on the bus (a file changed, a workflow triggered, a timer).
- A workflow node (`agent` node type).
- Another agent (as a sub-agent or via a channel).
- An MCP tool call from an external client.

Every input has a **trigger kind** recorded in the session.

#### 5.2.2 Context builder

Assembles the initial message list:

1. **System prompt** — from the agent definition.
2. **Capabilities summary** — tools, skills, MCP servers available.
3. **Constraints** — permissions, limits, sandbox scope.
4. **Memory** — relevant episodic + semantic memories (see §8).
5. **RAG** — top-k relevant chunks from the brain (see `SEARCH.md`).
6. **History** — recent messages from the session, compacted if needed.
7. **Input** — the current message/event.

The context is tokenized and truncated/summarized to fit the model window
minus the output reserve.

#### 5.2.3 Planner

For non-trivial tasks, the agent produces a plan:

- A list of steps (2-8 typically).
- Each step: a short description + expected tool calls.
- The plan is stored in the session and shown in the UI (collapsible).
- For `reasoning` models, the plan may be implicit (part of the model's
  thinking).

The planner is **optional**. For simple tasks (single-turn, no tools), it's
skipped. A heuristic decides: task length, number of tools, model
capability.

#### 5.2.4 Executor

The core loop:

```
while not done and budget not exhausted:
    response = llm.chat(messages, tools, params)
    if response has tool_calls:
        for each tool_call:
            if needs_permission(tool_call):
                ask_user()  # blocks, may be auto-allowed
            result = execute_tool(tool_call)
            messages.append(tool_result(result))
    else:
        messages.append(response)
        break  # or continue if the agent decides to reflect
```

Key properties:

- **Streaming**: tokens are streamed to the UI as they arrive.
- **Parallel tool calls**: if the model emits multiple independent calls,
  they run concurrently (subject to per-tool concurrency limits).
- **Cancellation**: the user can stop the run at any time.
- **Interruption**: the agent can pause and ask the user a question.

#### 5.2.5 Reflector

After each tool call (or batch), a lightweight reflection step:

- Did the tool succeed?
- Does the result answer the question?
- Should we continue, retry, or stop?

For `reasoning` models, this is part of the model's output. For others, a
short separate LLM call (with a cheap model) can evaluate. The reflector can
also decide to spawn a sub-agent, switch tools, or ask the user.

#### 5.2.6 Output

The run ends with:

- A final message to the user (or the caller).
- Side effects (files written, notes created, workflows triggered).
- A structured summary (for chained runs).
- Metrics recorded (tokens, cost, duration, tool calls).

### 5.3 Loop variants

Fergents supports several loop patterns, selected per agent:

| Pattern | When | Behavior |
|---|---|---|
| **ReAct** | Default | Reason → Act → Observe, repeat |
| **Plan-and-Execute** | Complex tasks | Plan all steps, then execute sequentially |
| **Reflexion** | Hard tasks | After failure, reflect and retry with new strategy |
| **Tree-of-Thought** | Exploratory tasks | Explore multiple branches, prune, pick best |
| **Debate** | Uncertain tasks | Two agents argue, a third decides |
| **Map-Reduce** | Large inputs | Split, process in parallel, merge |

The pattern is set in the agent's frontmatter (`loop: react | plan_execute
| reflexion | ...`). Default: `react`.

### 5.4 Stopping conditions

A run stops when:

- The agent emits a final answer.
- The budget is exhausted (tokens, cost, time, tool calls).
- The user cancels.
- A fatal error occurs.
- A `stop` signal is emitted by a tool or a permission denial.
- The maximum iterations are reached (default: 50).

---

## 6. Context management

### 6.1 The context budget

A model has a context window (e.g. 200k tokens). We reserve:

- **Output reserve**: `max_tokens` from the agent definition.
- **Safety margin**: 5% of the window.

The rest is the **input budget**, split across:

- System prompt: fixed.
- Capabilities + constraints: fixed, small.
- Memory: dynamic (see §8).
- RAG: dynamic (top-k, bounded).
- History: dynamic (compacted if needed).
- Input: fixed per turn.

### 6.2 Compaction strategies

When history exceeds its share:

1. **Drop old tool results** (keep the call, summarize the result).
2. **Summarize old turns** into a "conversation so far" block.
3. **Drop old turns** entirely, keeping only the last N.
4. **Retrieve** relevant old turns via RAG instead of keeping them.

The strategy is configurable per agent (`memory.compaction: aggressive |
balanced | conservative`). Default: `balanced`.

### 6.3 Summarization

Summaries are produced by a cheap model (routed via `summarize` task). Each
summary is stored in `agent_memory` as an episodic entry, so it can be
retrieved later.

### 6.4 RAG injection

For each turn, the context builder runs a hybrid search against the brain
(current project's primary brain, or a specified one) using the current
input + the last user message as the query. Top-k chunks (default 8) are
injected with a header:

```
## Relevant context from your brain

[1] note.md#section (score 0.87)
...chunk text...

[2] ...
```

The agent is instructed to cite chunks by number when using them.

### 6.5 Tool result formatting

Tool results are formatted consistently:

```json
{
  "tool": "brain.search",
  "status": "ok",
  "duration_ms": 42,
  "result": { ... },
  "citations": [ ... ]
}
```

Errors include:

```json
{
  "tool": "shell.exec",
  "status": "error",
  "error": { "code": "permission_denied", "message": "..." }
}
```

The agent sees the same structure the UI does.

---

## 7. Tools

### 7.1 What a tool is

A tool is a callable function exposed to the agent. Tools come from:

- **Kernel built-ins** (see §22).
- **Skills** (see `SKILLS.md`).
- **Plugins** (see `PLUGINS.md`).
- **MCP servers** (see `MCP.md`).
- **Other agents** (an agent can be invoked as a tool).

### 7.2 Tool schema

Every tool declares:

```json
{
  "name": "brain.search",
  "description": "Search the active brain using hybrid search.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string" },
      "top_k": { "type": "integer", "default": 8 },
      "filters": { "type": "object" }
    },
    "required": ["query"]
  },
  "permissions": ["brain.read"],
  "cost_hint": { "tokens": 0, "money_usd": 0, "time_ms": 100 },
  "streaming": false,
  "idempotent": true
}
```

### 7.3 Tool execution

1. **Validate** arguments against the schema.
2. **Check permissions** (category, scope, mode).
3. **Request approval** if needed.
4. **Execute** in the appropriate sandbox.
5. **Record** in the audit log.
6. **Return** the result (streamed if long).

### 7.4 Tool categories

| Category | Examples |
|---|---|
| Brain | `brain.search`, `brain.read`, `brain.write`, `brain.create_note` |
| Files | `fs.read`, `fs.write`, `fs.list`, `fs.move`, `fs.delete` |
| Shell | `shell.exec` (sandboxed), `shell.spawn` |
| Web | `web.fetch`, `web.search`, `web.screenshot` |
| Code | `code.run` (WASM/TS), `code.format`, `code.lint` |
| LLM | `llm.ask`, `llm.embed`, `llm.rerank` |
| MCP | `mcp.call` (any MCP tool) |
| Workflow | `workflow.run`, `workflow.status` |
| Agents | `agent.spawn`, `agent.call`, `agent.message` |
| Skills | `skill.invoke` |
| System | `system.info`, `system.settings` |

### 7.5 Tool selection by the model

The full list of available tools is passed to the model in the request
(`tools` parameter). Modern models handle 20-100 tools fine. Beyond that,
we use **tool retrieval**: embed the tool descriptions, retrieve the top-N
relevant to the current context, and pass those. This is configured per
agent (`tools.mode: all | retrieved`, default `retrieved` when > 30 tools).

---

## 8. Memory

### 8.1 Three tiers

| Tier | Scope | Storage | Retrieval |
|---|---|---|---|
| **Working** | Current run | In-memory + session | Always in context |
| **Episodic** | Session history | `agent_memory` (kind=episodic) | Recent + RAG |
| **Semantic** | Long-term facts | `agent_memory` (kind=fact) | RAG |

Plus the **brain** itself, which is shared semantic memory across agents
(see `BRAIN.md`).

### 8.2 Episodic memory

Each session's messages are stored in `agent_messages`. Older turns are
summarized into episodic entries:

```
[2025-10-14 09:12] User asked to research MCP security.
[2025-10-14 09:18] I searched the brain (3 results) and the web (2 sources).
[2025-10-14 09:35] I wrote a note: research/mcp-security.md.
```

Episodic entries are embedded and retrievable.

### 8.3 Semantic memory (facts)

Facts the agent decides to remember long-term:

```
[fact] The user prefers concise answers, no preamble.
[fact] The main project is "ferraro3d-app", a Tauri + Rust app.
[fact] The user's timezone is Europe/Rome.
```

Facts are:

- Written by the agent via the `memory.remember` tool.
- Retrieved via RAG when relevant.
- Visible and editable by the user in the UI.
- Scoped: global, per-agent, per-project, per-brain.

### 8.4 Memory tools

- `memory.remember(content, kind, scope, importance)`
- `memory.recall(query, k, scope)`
- `memory.forget(id)`
- `memory.list(scope, kind)`

The agent uses these naturally. The user sees all memory in a dedicated
panel.

### 8.5 Memory lifecycle

- **Decay**: low-importance facts age out (configurable).
- **Reinforcement**: retrieved facts get a higher importance score.
- **Consolidation**: periodic job merges duplicates and summarizes clusters.
- **User control**: everything is visible, editable, deletable.

### 8.6 Cross-agent memory

Facts marked `scope: global` are shared across all agents. Facts marked
`scope: agent:<id>` are private to that agent. This enables both shared
knowledge and distinct personalities.

---

## 9. Sub-agents

### 9.1 Concept

Any agent can spawn a **sub-agent** to handle a subtask. The sub-agent:

- Has its own session, context, tools, and budget.
- Receives a focused task and constraints from the parent.
- Returns a structured result.
- Is visible in the UI as a child of the parent session.

### 9.2 Spawning

```json
{
  "tool": "agent.spawn",
  "arguments": {
    "agent_id": "researcher",
    "task": "Find 5 primary sources on MCP security.",
    "context": "Focus on 2024-2025. Prefer academic and official sources.",
    "budget": { "max_cost_usd": 0.50, "max_wall_time_s": 300 },
    "tools": ["web.search", "web.fetch", "brain.write"],
    "return_schema": { "sources": "array" }
  }
}
```

### 9.3 Constraints

- The parent can narrow the child's tools, permissions, and budget, but
  cannot widen them beyond its own.
- The child inherits the parent's permission mode unless overridden.
- Depth is limited (`sub_agents.max_depth`, default 2).
- Total sub-agents per run is limited (`max_sub_agents`, default 4).

### 9.4 Communication

Parent ↔ child communication:

- **Synchronous**: parent waits for the result (`agent.call`).
- **Asynchronous**: parent spawns and continues (`agent.spawn` + polling or
  events).
- **Messaging**: parent and child can exchange messages via the session
  channel.

### 9.5 Result aggregation

When a sub-agent finishes, it returns a structured result:

```json
{
  "status": "ok",
  "summary": "Found 5 sources, 2 primary, 3 secondary.",
  "data": { "sources": [...] },
  "cost_usd": 0.12,
  "duration_ms": 45210,
  "artifacts": ["research/mcp-security.md"]
}
```

The parent integrates this into its own context.

---

## 10. Multi-agent orchestration

### 10.1 Patterns

Fergents supports several orchestration patterns:

| Pattern | Description | Use case |
|---|---|---|
| **Manager** | One agent coordinates specialists | Complex projects |
| **Pipeline** | Agents in sequence, output → input | Data processing |
| **Debate** | Two agents argue, a third decides | Uncertain decisions |
| **Swarm** | Many agents work in parallel, results merged | Large research |
| **Hierarchy** | Tree of managers and workers | Very large tasks |
| **Channel** | Agents talk in a shared channel | Collaborative work |

### 10.2 The Manager pattern

The default orchestration for complex tasks:

```
              ┌──────────┐
              │ MANAGER  │
              └────┬─────┘
                   │ spawns
     ┌─────────────┼─────────────┐
     ▼             ▼             ▼
┌─────────┐   ┌─────────┐   ┌─────────┐
│Research │   │ Coder   │   │Writer   │
└─────────┘   └─────────┘   └─────────┘
```

The manager:

- Decomposes the task.
- Assigns subtasks to specialists.
- Monitors progress.
- Integrates results.
- Reports back.

### 10.3 The Channel pattern

Agents can join a **channel** (like a Slack channel) and exchange messages:

```
#research-channel
  researcher: "Found a paper on MCP security."
  coder:      "Can you extract the attack vectors?"
  researcher: "Done, see the note."
  writer:     "I'll summarize for the report."
```

Channels are persisted in `agent_channels` and `agent_channel_messages`.
The UI shows them as live conversations. Humans can join too.

### 10.4 Orchestration tools

- `orchestrator.decompose(task)` — split into subtasks.
- `orchestrator.assign(task, agent)` — assign.
- `orchestrator.monitor()` — get status of children.
- `orchestrator.merge(results)` — combine.
- `orchestrator.escalate(reason)` — ask the user.

These are exposed as tools to manager agents.

---

## 11. Agent-to-agent communication

### 11.1 Direct messaging

```json
{
  "tool": "agent.message",
  "arguments": {
    "to": "session:01J8...",
    "content": "Please prioritize primary sources."
  }
}
```

### 11.2 Channels

```json
{
  "tool": "channel.post",
  "arguments": {
    "channel": "research-channel",
    "content": "New finding: ..."
  }
}
```

### 11.3 Events

Agents can subscribe to event topics:

```yaml
triggers:
  - kind: event
    topic: file.changed
    filter: { path: "inbox/**" }
```

When the event fires, the agent runs with the event payload as input.

### 11.4 Handoff

An agent can hand off a task to another agent:

```json
{
  "tool": "agent.handoff",
  "arguments": {
    "to": "coder",
    "task": "Implement the fix described in the note.",
    "context_ref": "note:research/mcp-security.md"
  }
}
```

The receiving agent starts a new run with the given context. The handoff is
recorded in the audit log and visible in the UI.

---

## 12. Permissions

Detailed in `SECURITY.md`. Summary:

- **Modes**: manual, auto (risky), full access. Set per session in the
  permission bar.
- **Categories**: fs.read, fs.write, shell.exec, net.http, net.p2p,
  llm.call, mcp.connect, plugin.install, agent.spawn, workflow.edit,
  brain.write, system.config.
- **Policy**: declarative TOML rules, evaluated in order, first match wins.
- **Per-agent overrides**: an agent can require stricter permissions than
  the session default, never looser.
- **Per-tool overrides**: a tool can declare a minimum permission level.
- **Audit**: every decision recorded with actor, action, args, result,
  reason, and reversibility reference.

Example agent override:

```yaml
permissions:
  shell.exec: deny
  fs.write:
    scope: ["$BRAIN/inbox/**", "$BRAIN/research/**"]
    mode: allow
```

---

## 13. Limits and budgets

### 13.1 Per-run limits

```yaml
limits:
  max_tokens_per_run: 200000
  max_cost_usd_per_run: 2.00
  max_wall_time_s: 900
  max_tool_calls_per_run: 100
  max_sub_agents: 4
  max_iterations: 50
```

Enforced by the kernel, not the agent. When a limit is hit:

- The run pauses.
- The user is notified.
- Options: raise the limit, stop, or continue with a different model.

### 13.2 Global and per-provider budgets

Configured in settings. The kernel checks before each LLM call. If a budget
would be exceeded, the call is rejected with a clear error, and the agent is
told to adapt (e.g. use a cheaper model or stop).

### 13.3 Cost estimation

Before a run, the kernel estimates the cost range based on:

- The task length.
- The model.
- Historical averages for similar runs.

The estimate is shown in the UI. After the run, actual cost is recorded.

### 13.4 Rate limits

Per provider, per model, per agent. Configurable. The kernel queues or
rejects requests when limits are reached.

---

## 14. Lifecycle

### 14.1 States

```
created → idle → running → (paused | waiting_permission | done | error | cancelled)
                ↑          │
                └──────────┘  (resume)
```

### 14.2 Transitions

- **created**: the session exists but no run has started.
- **running**: a run is in progress.
- **paused**: the run is paused (user or budget).
- **waiting_permission**: waiting for a permission decision.
- **done**: the run finished successfully.
- **error**: the run failed.
- **cancelled**: the user cancelled the run.

Transitions are recorded as events. The UI shows the state in real time.

### 14.3 Resumption

A paused or waiting run can be resumed:

- From the UI (a "Resume" button).
- Automatically when a permission is granted.
- Via an API call.
- Via a workflow node.

The kernel persists the full session state so resumption is exact.

### 14.4 Timeouts

Per-run wall-clock timeout (default: 15 min, configurable). On timeout:

- The run is paused.
- The user is notified.
- Options: continue, stop, or extend.

---

## 15. Persistence

### 15.1 What is persisted

- Agent definitions: as files.
- Sessions: `agent_sessions`.
- Messages: `agent_messages`.
- Memory: `agent_memory`.
- Children: `agent_children`.
- Channels: `agent_channels`, `agent_channel_messages`.
- Metrics: `llm_requests`.
- Events: `events`.

### 15.2 Replay

Any session can be replayed:

- The message sequence is re-rendered.
- Tool calls and results are shown.
- The state at any point is reconstructible.

### 15.3 Export/import

A session can be exported as JSON (for sharing, debugging, or archiving).
An agent definition can be exported as a `.md` file (its native format).

---

## 16. Built-in agents

Fergents ships with a small set of default agents, all editable and
deletable:

| Agent | Purpose |
|---|---|
| **Assistant** | General-purpose chat, the default. |
| **Researcher** | Deep research with citations. |
| **Coder** | Code writing, editing, review. |
| **Writer** | Long-form writing, editing. |
| **Planner** | Task decomposition, project planning. |
| **Reviewer** | Critical review of work. |
| **Librarian** | Brain organization, tagging, linking. |
| **Orchestrator** | Multi-agent manager. |

These are starting points. Users create their own, and the AI can too.

---

## 17. Agent creation by AI

The AI can create new agents. The flow:

1. User: "Create an agent that monitors my RSS feeds and summarizes them
   every morning."
2. The agent uses `agent.create` with a definition:

```json
{
  "tool": "agent.create",
  "arguments": {
    "id": "rss-digest",
    "name": "RSS Digest",
    "definition": "---\nid: rss-digest\n...\n---\n\nYou are..."
  }
}
```

3. The kernel validates the definition, writes it to `agents/rss-digest.md`,
   and registers it.
4. The UI shows the new agent. The user can edit it.
5. The agent (or the user) can now trigger it, schedule it, or reference it
   in a workflow.

The AI can also:

- Duplicate an existing agent.
- Modify an agent's prompt, tools, or limits.
- Delete an agent (with confirmation).
- Enable/disable an agent.

All subject to permissions.

---

## 18. Observability

### 18.1 Metrics

Per agent, per session, per run:

- Tokens (input, output, cached).
- Cost (USD).
- Duration.
- Tool calls (count, by tool).
- Errors (count, by type).
- Sub-agents spawned.
- Memory reads/writes.

### 18.2 Traces

Every run produces an OpenTelemetry span tree:

```
run (agent=researcher, session=01J8...)
├── context_build
│   ├── memory_retrieve
│   └── rag_search
├── llm_call (model=claude-sonnet-4.5, tokens=1234)
├── tool_call (tool=web.search)
├── tool_call (tool=web.fetch)
├── llm_call (...)
└── output
```

Traces are viewable in the UI and exportable.

### 18.3 UI

The agent status panel shows:

- All active agents with their state.
- Sub-agent trees.
- Live token streaming.
- Cost accumulation.
- Permission requests inline.
- A "stop" button for each.

---

## 19. Failure modes and recovery

| Failure | Detection | Recovery |
|---|---|---|
| LLM API error | HTTP error / timeout | Retry with backoff; fallback to another provider if configured |
| Rate limit | 429 | Queue, backoff, or fallback |
| Invalid tool call | Schema validation | Tell the model, retry |
| Tool execution error | Exception | Return error to model, let it adapt |
| Permission denied | Policy | Tell the model, let it try another way |
| Budget exceeded | Kernel check | Pause, notify user |
| Infinite loop | Iteration limit | Stop, report |
| Hallucinated tool | Unknown tool name | Tell the model, list valid tools |
| Context overflow | Token count | Compact, summarize, retry |
| Sub-agent failure | Child error | Parent decides: retry, replace, abort |
| Network partition (MCP) | Health check | Reconnect, or disable server |
| Model degradation | Quality heuristics | Switch to fallback model |

Every failure is logged with enough context to reproduce.

---

## 20. Examples

### 20.1 A simple Q&A agent

```markdown
---
id: qa
name: Q&A
description: Answers questions using the brain.
model: ollama:qwen2.5:7b
tools: [brain.search, brain.read]
memory:
  episodic: true
  semantic: false
limits:
  max_cost_usd_per_run: 0
---

You answer questions using the brain. Search first, then answer. Cite
sources by note path. If you don't know, say so.
```

### 20.2 A coder agent

```markdown
---
id: coder
name: Coder
description: Writes and edits code in the active project.
model: anthropic:claude-sonnet-4.5
temperature: 0.2
tools:
  - fs.read
  - fs.write
  - fs.list
  - shell.exec
  - code.run
  - brain.search
  - brain.write
permissions:
  fs.write:
    scope: ["$PROJECT/**"]
    mode: prompt
  shell.exec:
    mode: prompt
    allowlist: ["cargo", "pnpm", "git", "rg", "ls", "cat"]
limits:
  max_cost_usd_per_run: 5.00
  max_wall_time_s: 1800
memory:
  episodic: true
  semantic: true
loop: plan_execute
---

You are a senior software engineer working in the active project.

## Workflow

1. **Understand**: read the relevant files, search the brain for context.
2. **Plan**: write a short plan (spec.md, plan.md, tasks.md) for non-trivial
   tasks.
3. **Implement**: make minimal, focused changes. One concern per commit.
4. **Test**: run tests. If they fail, fix.
5. **Review**: read your own diff. Check for edge cases.
6. **Report**: summarize what changed and why.

## Rules

- Never modify files outside the project without permission.
- Always run the test suite before declaring done.
- Prefer existing patterns over new abstractions.
- Leave the code better than you found it.
```

### 20.3 A researcher agent

(see §4.1)

### 20.4 A multi-agent orchestrator

```markdown
---
id: orchestrator
name: Orchestrator
description: Coordinates a team of specialists for complex tasks.
model: anthropic:claude-sonnet-4.5
reasoning: true
tools:
  - agent.spawn
  - agent.message
  - agent.handoff
  - orchestrator.decompose
  - orchestrator.assign
  - orchestrator.monitor
  - orchestrator.merge
  - brain.search
  - brain.write
limits:
  max_sub_agents: 8
  max_cost_usd_per_run: 20.00
  max_wall_time_s: 3600
loop: plan_execute
---

You are the Orchestrator. You coordinate a team of specialist agents.

## Team

- `researcher`: gathers information.
- `coder`: writes and edits code.
- `writer`: produces prose.
- `reviewer`: critically evaluates work.

## Workflow

1. **Understand** the task. Ask one clarifying question if ambiguous.
2. **Decompose** into 2-8 subtasks.
3. **Assign** each subtask to the best specialist.
4. **Monitor** progress. Unblock as needed.
5. **Integrate** results into a coherent whole.
6. **Review** with the reviewer agent.
7. **Report** to the user with a summary.

## Rules

- Prefer fewer, larger subtasks over many small ones.
- Never do the work yourself if a specialist can.
- Escalate to the user if a subtask fails twice.
- Keep total cost under budget.
```

---

## 21. Appendix: frontmatter schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Fergents Agent Definition",
  "type": "object",
  "required": ["id", "name", "model"],
  "properties": {
    "id": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$" },
    "name": { "type": "string", "minLength": 1 },
    "description": { "type": "string" },
    "version": { "type": "string" },
    "author": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } },

    "model": { "type": "string" },
    "model_fallback": { "type": "array", "items": { "type": "string" } },
    "temperature": { "type": "number", "minimum": 0, "maximum": 2 },
    "max_tokens": { "type": "integer", "minimum": 1 },
    "reasoning": { "type": "boolean" },
    "loop": {
      "enum": ["react", "plan_execute", "reflexion", "tot", "debate", "map_reduce"]
    },

    "tools": { "type": "array", "items": { "type": "string" } },
    "skills": { "type": "array", "items": { "type": "string" } },
    "mcp": { "type": "array", "items": { "type": "string" } },

    "permissions": {
      "type": "object",
      "additionalProperties": {
        "oneOf": [
          { "enum": ["allow", "prompt", "deny"] },
          {
            "type": "object",
            "properties": {
              "mode": { "enum": ["allow", "prompt", "deny"] },
              "scope": { "type": "array", "items": { "type": "string" } },
              "allowlist": { "type": "array", "items": { "type": "string" } }
            }
          }
        ]
      }
    },

    "limits": {
      "type": "object",
      "properties": {
        "max_tokens_per_run": { "type": "integer" },
        "max_cost_usd_per_run": { "type": "number" },
        "max_wall_time_s": { "type": "integer" },
        "max_tool_calls_per_run": { "type": "integer" },
        "max_sub_agents": { "type": "integer" },
        "max_iterations": { "type": "integer" }
      }
    },

    "memory": {
      "type": "object",
      "properties": {
        "episodic": { "type": "boolean" },
        "semantic": { "type": "boolean" },
        "working_window": { "type": "integer" },
        "summarize_at": { "type": "number", "minimum": 0, "maximum": 1 },
        "rag_top_k": { "type": "integer" },
        "compaction": { "enum": ["aggressive", "balanced", "conservative"] }
      }
    },

    "sub_agents": {
      "type": "object",
      "properties": {
        "allow": { "type": "boolean" },
        "max_depth": { "type": "integer" },
        "default_tools": { "type": "array", "items": { "type": "string" } }
      }
    },

    "triggers": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "kind": { "enum": ["manual", "cron", "event", "webhook"] },
          "schedule": { "type": "string" },
          "topic": { "type": "string" },
          "filter": { "type": "object" }
        },
        "required": ["kind"]
      }
    },

    "icon": { "type": "string" },
    "color": { "type": "string" },
    "ui": {
      "type": "object",
      "properties": {
        "visible": { "type": "boolean" },
        "order": { "type": "integer" }
      }
    }
  }
}
```

---

## 22. Appendix: built-in tools reference

### Brain
- `brain.search(query, top_k, filters)` → chunks with scores.
- `brain.read(path, range?)` → document content.
- `brain.write(path, content, mode?)` → write or append.
- `brain.create_note(title, body, tags?)` → new note.
- `brain.link(src, dst, anchor?)` → create a link.
- `brain.tag(path, tags)` → add tags.
- `brain.list(path?, tag?)` → list notes.

### Files
- `fs.read(path, range?)`
- `fs.write(path, content)`
- `fs.append(path, content)`
- `fs.list(path, recursive?)`
- `fs.move(src, dst)`
- `fs.delete(path)`
- `fs.exists(path)`
- `fs.stat(path)`

### Shell
- `shell.exec(command, cwd?, env?, timeout?)` → stdout/stderr/exit code.
- `shell.spawn(command, args)` → background process handle.

### Web
- `web.fetch(url, format?)` → markdown, text, or HTML.
- `web.search(query, engine?, top_k?)` → results.
- `web.screenshot(url)` → image.

### Code
- `code.run(language, source, input?)` → output (WASM sandbox).
- `code.format(language, source)` → formatted source.
- `code.lint(language, source)` → diagnostics.

### LLM
- `llm.ask(prompt, model?, system?)` → text.
- `llm.embed(texts, model?)` → vectors.
- `llm.rerank(query, documents, model?)` → scores.

### MCP
- `mcp.call(server, tool, arguments)` → result.
- `mcp.list_tools(server?)` → available tools.
- `mcp.list_resources(server?)` → available resources.

### Workflow
- `workflow.run(id, inputs)` → run id.
- `workflow.status(run_id)` → status.
- `workflow.cancel(run_id)` → cancel.

### Agents
- `agent.spawn(agent_id, task, budget?, tools?)` → child session id.
- `agent.call(agent_id, task, budget?)` → result (synchronous).
- `agent.message(session_id, content)` → send a message.
- `agent.handoff(agent_id, task, context_ref?)` → transfer.
- `agent.list()` → available agents.

### Skills
- `skill.invoke(skill_id, input)` → result.
- `skill.list()` → available skills.

### Memory
- `memory.remember(content, kind, scope, importance?)`
- `memory.recall(query, k, scope?)`
- `memory.forget(id)`
- `memory.list(scope?, kind?)`

### Channels
- `channel.create(name)` → channel id.
- `channel.post(channel, content)` → message id.
- `channel.read(channel, since?)` → messages.

### System
- `system.info()` → version, platform, resources.
- `system.settings.get(key?)`
- `system.settings.set(key, value)`
- `system.notify(title, body)`

---

*End of `AGENTS.md`.*