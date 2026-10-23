# Architecture

> The structural blueprint of Fergents: how the kernel, clients, plugins, agents,
> and storage fit together, and why.

**Audience:** developers, AI developers, and the architect.
**Companion docs:** `STACK.md` (choices), `DATA_MODEL.sql` (schema),
`SECURITY.md` (permissions & sandbox), `AGENTS.md` (agent runtime).

---

## Table of contents

1. [Design goals](#1-design-goals)
2. [The three-layer model](#2-the-three-layer-model)
3. [The kernel daemon](#3-the-kernel-daemon)
4. [The event bus](#4-the-event-bus)
5. [Clients](#5-clients)
6. [The plugin system](#6-the-plugin-system)
7. [Storage architecture](#7-storage-architecture)
8. [The multi-brain model](#8-the-multi-brain-model)
9. [The permission system](#9-the-permission-system)
10. [The sandbox model](#10-the-sandbox-model)
11. [Agent runtime overview](#11-agent-runtime-overview)
12. [MCP as the integration spine](#12-mcp-as-the-integration-spine)
13. [Sync and P2P](#13-sync-and-p2p)
14. [Observability and audit](#14-observability-and-audit)
15. [Reversibility and backups](#15-reversibility-and-backups)
16. [Process lifecycle](#16-process-lifecycle)
17. [Key decisions](#17-key-decisions)

---

## 1. Design goals

The architecture is optimized for:

| Goal | How |
|---|---|
| **Local-first** | SQLite per brain, files on disk, no server required |
| **Multi-client** | Kernel as a daemon; desktop, CLI, mobile as clients |
| **Extensibility** | Plugin-everything: WASM + React UI in Shadow DOM |
| **AI-native** | All capabilities exposed via the same API humans use |
| **Interoperability** | MCP in both directions (host + server) |
| **Reversibility** | Event-sourced, `.fergents` backups, shadow git |
| **Performance** | Rust kernel, embedded indexes, incremental operations |
| **Portability** | Windows, macOS, Linux, Android; headless server later |
| **Security** | Capability permissions, WASM sandbox, OS-level isolation |
| **Longevity** | Files as the substrate (markdown, YAML, TOML, git) |

Non-goals (for v1):

- Real-time collaborative editing (post-v1)
- Cloud-hosted multi-tenant SaaS (post-v1, but architecture supports it)
- A plugin marketplace with payments (post-v1)

---

## 2. The three-layer model

```
┌───────────────────────────────────────────────────────────────┐
│  LAYER 3 — CLIENTS                                            │
│  Desktop (Tauri + React) · CLI (TUI) · Mobile (Android)       │
│  Stateless. Render kernel state. Send commands.               │
└───────────────────────────────────────────────────────────────┘
              │ local socket (JSON-RPC 2.0 over Unix pipe / named pipe)
              │ (WebSocket for remote/mobile)
┌───────────────────────────────────────────────────────────────┐
│  LAYER 2 — KERNEL DAEMON (fergentsd, Rust)                    │
│  Event bus · Registry · Scheduler · Permissions · Agents ·    │
│  LLM router · MCP hub · Workflow engine · Skill runtime ·     │
│  Indexer · Graph engine · Plugin host · Sync engine · Storage │
└───────────────────────────────────────────────────────────────┘
              │ WASM (wasmtime) · SQLite · File system
┌───────────────────────────────────────────────────────────────┐
│  LAYER 1 — SUBSTRATE                                          │
│  SQLite (per brain) · Markdown vault · YAML/TOML config ·     │
│  Shadow git · CozoDB (graph) · sqlite-vec (vectors) · FTS5    │
└───────────────────────────────────────────────────────────────┘
```

**Rules:**

- Layer 3 **never** touches Layer 1 directly. All access goes through Layer 2.
- Layer 2 exposes **one** API surface: the kernel protocol. No subsystems have
  side channels.
- Layer 1 is **portable**: markdown, YAML, SQLite, git. You can read, back up,
  and migrate without Fergents.

---

## 3. The kernel daemon

### 3.1 Why a daemon?

Running the kernel as a separate process (not embedded in the desktop app) is
the single most consequential architectural decision. It unlocks:

- **Multi-client**: desktop + CLI + mobile all share state.
- **Headless mode**: run on a server for self-hosted / multi-user setups.
- **Parallelism**: agent runs continue even if the UI closes.
- **Testability**: the kernel can be tested in isolation.
- **Sync**: two kernels on two machines can talk directly.

The cost is a local IPC layer and lifecycle management. The benefit is
enormous.

### 3.2 Process model

```
fergentsd                     ← one daemon per user (not per brain)
├── core                      ← event bus, registry, scheduler, permissions
├── storage                   ← SQLite pools, vector store, graph, FTS
├── llm                       ← provider clients, router, cost tracking
├── mcp                       ← host (outgoing) + server (incoming)
├── agents                    ← runtime, orchestrator, sub-agents
├── skills                    ← SKILL.md runtime, WASM sandbox
├── workflow                  ← DAG engine, triggers, runs
├── indexer                   ← chunking, embeddings, rerank
├── graph                     ← entity extraction, relations, queries
├── plugins                   ← WASM host, UI bridge, marketplace client
├── sync                      ← mDNS, libp2p, CRDT, event log
├── api                       ← local socket server (JSON-RPC 2.0)
├── observability             ← tracing, metrics, audit log
└── supervisor                ← health, restarts, crash recovery
```

All modules are async tasks on a shared Tokio runtime, coordinating through
the event bus and a set of registries. No module calls another directly
except through well-defined traits.

### 3.3 Lifecycle

1. **Start**: `fergentsd` is launched (by the desktop app, CLI, or manually).
2. **Discovery**: on Windows, named pipe `\\.\pipe\fergentsd`; on Unix,
   `$XDG_RUNTIME_DIR/fergentsd.sock` (or `~/.fergents/fergentsd.sock`).
3. **Handshake**: client sends `hello` with protocol version and capabilities.
4. **Session**: client sends commands, receives events. Multiple clients
   coexist; each has its own session with its own subscriptions.
5. **Shutdown**: explicit `shutdown` command, or idle timeout (configurable).
   Clients can auto-spawn the daemon if not present.

### 3.4 Kernel protocol

A JSON-RPC 2.0 protocol over the local socket. Three primitives:

- **Command**: request from client to kernel (`brain.create`, `agent.run`,
  `workflow.execute`, `plugin.install`, …). Returns a result or error.
- **Event**: kernel pushes to subscribed clients (`agent.token`, `run.step`,
  `file.changed`, `permission.request`, …). Clients filter by topic.
- **Stream**: long-lived bidirectional channel for token streaming, terminal
  I/O, and live collaboration.

Schemas are defined in `protocol/` (JSON Schema) and generated into Rust,
TypeScript, and Kotlin bindings. **The protocol is the contract.**

---

## 4. The event bus

### 4.1 Why an event bus?

Because it decouples everything. When an agent modifies a file, the event
triggers:

- The indexer to re-chunk that file
- The graph engine to re-extract entities
- The UI to refresh
- Any workflow listening on `file.changed` to run
- The audit log to record the action
- The shadow git to commit

None of these components know about each other. They subscribe. This is what
makes plugins and workflows powerful: they hook into the same bus.

### 4.2 Event anatomy

```jsonc
{
  "id": "01J8K...",              // ULID, monotonic
  "ts": 1728892800123,           // ms since epoch
  "kind": "file.changed",        // namespaced topic
  "source": "agent:ferg-01",     // who emitted
  "correlation_id": "01J8J...",  // groups events in one logical operation
  "brain_id": "brain:personal",  // scope (optional)
  "project_id": "proj:ferg",     // scope (optional)
  "payload": { ... },            // topic-specific
  "version": 1                   // schema version
}
```

### 4.3 Persistence

Every event is persisted to SQLite (`events` table) in the relevant brain or
in the global store. Persistence gives:

- **Replay**: rebuild any projection from scratch.
- **Audit**: complete history of every action.
- **Sync**: propagate events to peers.
- **Time travel**: inspect state at any point (dev/debug feature).

Retention policy: configurable per event kind. Default: keep forever for
`agent.*`, `run.*`, `permission.*`, `file.changed`; rolling 30 days for noisy
topics like `telemetry.*`.

### 4.4 Topics (non-exhaustive)

```
kernel.started / kernel.stopping / kernel.error

brain.created / brain.opened / brain.closed / brain.synced
file.created / file.changed / file.deleted / file.renamed
note.created / note.updated / note.linked

index.queued / index.started / index.progress / index.completed
embedding.created / embedding.failed
search.executed / search.ranked

agent.created / agent.started / agent.token / agent.tool_call
agent.thinking / agent.message / agent.finished / agent.error
agent.permission_requested / agent.permission_granted / agent.permission_denied

llm.request / llm.response / llm.error / llm.cost
llm.routed / llm.cached / llm.rate_limited

mcp.server.connected / mcp.server.disconnected / mcp.server.error
mcp.tool.registered / mcp.tool.called / mcp.resource.read
mcp.prompt.requested

workflow.created / workflow.triggered / workflow.started
workflow.step.started / workflow.step.finished / workflow.step.failed
workflow.completed / workflow.failed / workflow.paused

skill.installed / skill.uninstalled / skill.enabled / skill.disabled
skill.invoked / skill.error

plugin.installed / plugin.enabled / plugin.disabled / plugin.error
plugin.permission_requested / plugin.permission_granted

sync.peer.discovered / sync.peer.connected / sync.peer.disconnected
sync.conflict.detected / sync.conflict.resolved
sync.change.pushed / sync.change.pulled

permission.asked / permission.granted / permission.revoked
audit.action / audit.rollback

system.health / system.resource / system.update
```

### 4.5 Backpressure and ordering

- Events on the same `correlation_id` are ordered.
- Subscribers can request replay from a given event ID (catch-up).
- Slow subscribers are buffered up to a limit, then dropped with an
  `events.dropped` notice; they must re-subscribe from the last seen ID.

---

## 5. Clients

### 5.1 Desktop (Tauri + React)

- **Tauri v2** wraps the Rust backend + a webview rendering React.
- The Tauri backend is a **thin client** of the kernel: it does not implement
  business logic. It forwards commands and relays events.
- UI is a **composition of plugins**: each panel (chat, editor, brain,
  workflow, dashboard) is a React component, most of them provided by plugins.
- The shell provides: window management, tab/split system, command palette,
  theme, i18n, keyboard shortcuts, notifications.
- Local secure storage: OS keychain for API keys and P2P identity.

### 5.2 CLI (TUI)

- Built with **Ratatui** (Rust) or **Textual** (Python, alternative).
- Same protocol as desktop. Can run headless.
- Primary uses: scripting, CI, remote sessions over SSH, quick tasks.
- Commands map 1:1 to kernel commands: `fergents chat`, `fergents brain`,
  `fergents agent run <name>`, `fergents workflow exec <name>`.

### 5.3 Mobile (Android companion)

- **Tauri Mobile** (v2) for a single codebase with desktop.
- Connects to the desktop kernel over LAN (mDNS discovery) or WAN (relay).
- Read-only for heavy operations: view brains, chat with agents, monitor
  runs, respond to permission requests, receive notifications.
- Local cache (SQLite) for offline reading.

### 5.4 Protocol binding generation

The kernel protocol schema (JSON Schema) is the source of truth. From it:

- Rust types via `serde` + `schemars`
- TypeScript types via `json-schema-to-typescript`
- Kotlin types via `quicktype` or custom generator
- OpenAPI-style docs generated for contributors

This guarantees the three clients stay in sync with the kernel.

---

## 6. The plugin system

### 6.1 Two kinds of plugins

| Kind | Runtime | Capabilities | Use cases |
|---|---|---|---|
| **Kernel plugin** | WASM (wasmtime, Component Model) | Tools, event subscribers, storage, LLM, MCP | New node types, data sources, indexers, custom search, integrations |
| **UI plugin** | React in Shadow DOM | UI slots, kernel commands, events | Panels, widgets, editors, visualizations, themes |

A single plugin can ship both. They communicate through the kernel: the UI
plugin calls commands, the kernel plugin handles them.

### 6.2 Permission manifest

Every plugin declares its permissions in `plugin.toml`:

```toml
[plugin]
id = "com.example.rss-reader"
name = "RSS Reader"
version = "1.0.0"
api_version = "1"

[permissions]
fs.read = ["$BRAIN/**/*.md", "$CACHE/rss/**"]
fs.write = ["$BRAIN/inbox/**"]
net.http = ["https://*.example.com/feed/*"]
llm.call = ["openai:gpt-4o-mini", "ollama:*"]
mcp.connect = ["stdio:rss-mcp-server"]
events.subscribe = ["file.changed", "workflow.triggered"]
events.emit = ["rss.item.fetched"]
storage.local = true
```

The user approves permissions at install and can audit/revoke them at any
time. The AI can install and configure plugins, but permission grants are
always user-mediated (unless "full access" mode is active).

### 6.3 UI slots

The shell defines named slots where plugins can render:

- `sidebar.rail` — icon-only rail
- `sidebar.panel` — expanded sidebar panel
- `main.tab` — new tab type in the main area
- `editor.toolbar` — editor toolbar additions
- `chat.message.actions` — actions on chat messages
- `chat.composer.extras` — composer additions
- `statusbar.item` — status bar items
- `dashboard.widget` — dashboard widgets
- `command.palette` — command palette entries
- `settings.section` — settings pages
- `graph.overlay` — graph view overlays
- `workflow.node` — custom workflow node types

Slots are versioned. Plugins declare which API version they target.

### 6.4 Installation sources

- Local `.fergplugin` file (WASM + manifest + assets, zipped)
- Local directory (dev mode)
- Git repository URL
- Marketplace (post-v1)

The AI can install from any of these, subject to permissions.

---

## 7. Storage architecture

### 7.1 The per-brain SQLite model

Each **brain** owns its own SQLite database file:

```
~/Fergents/
├── config.toml                    ← global config
├── secrets/                       ← OS keychain references (no plaintext)
├── projects/
│   └── my-project/
│       ├── project.toml           ← project config
│       └── links.toml             ← brain links
├── brains/
│   ├── personal/
│   │   ├── brain.toml             ← brain config
│   │   ├── vault/                 ← markdown files
│   │   │   ├── daily/
│   │   │   ├── notes/
│   │   │   └── attachments/
│   │   ├── index.sqlite           ← chunks, embeddings, FTS, entities
│   │   ├── graph.db               ← CozoDB (relations)
│   │   ├── .git/                  ← optional versioning
│   │   └── .fergents/             ← backups, snapshots, logs
│   └── research/
│       └── ...
├── plugins/
│   ├── installed/
│   └── dev/
├── agents/
│   └── *.md                       ← user-defined agents
├── skills/
│   └── */SKILL.md
├── workflows/
│   └── *.yaml
└── logs/
```

**Why per-brain SQLite?**

- Portability: a brain is a folder. Copy it, ship it, version it.
- Isolation: corruption in one brain doesn't affect others.
- Performance: smaller DBs, smaller indexes, faster queries.
- Multi-attach: a project can open multiple brains; a brain can be in
  multiple projects.

A **global SQLite** (`~/.fergents/global.sqlite`) stores cross-brain state:
users, sessions, installed plugins, global events, cost tracking, audit log.

### 7.2 Storage engines

| Data | Engine | Why |
|---|---|---|
| Markdown files | File system | Portability, git compatibility |
| Structured records | SQLite | ACID, embedded, ubiquitous |
| Full-text search | SQLite FTS5 | Integrated, fast, no server |
| Vector embeddings | `sqlite-vec` | Embedded, fast, works per brain |
| Graph relations | CozoDB (embedded) | Datalog queries, embedded, fast |
| Large binary (media) | File system + DB index | Simple, cacheable |
| Event log | SQLite (append-only) | Durable, replayable, syncable |
| Config | TOML | Human-readable, git-friendly |
| Agent/skill/workflow defs | Markdown + YAML frontmatter | Human-readable, AI-editable |

### 7.3 The Storage trait

The kernel defines a `Storage` trait to abstract the engines:

```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn get(&self, key: &Key) -> Result<Option<Value>>;
    async fn put(&self, key: Key, value: Value) -> Result<()>;
    async fn delete(&self, key: &Key) -> Result<()>;
    async fn query(&self, q: Query) -> Result<Vec<Row>>;
    async fn transaction<F, T>(&self, f: F) -> Result<T>
        where F: FnOnce(&mut Transaction) -> Result<T> + Send;
}
```

Implementations:

- `SqliteStorage` (default, per brain)
- `PostgresStorage` (future, for self-hosted multi-user)

This is what allows the architecture to scale to a server deployment later
without rewriting business logic.

### 7.4 Migrations

Migrations live in `migrations/` as numbered SQL files. The kernel applies
them on brain open, tracks versions in a `schema_version` table, and refuses
to open a brain with a newer schema than it understands.

---

## 8. The multi-brain model

### 8.1 Concepts

- **Brain**: a knowledge base. Files, index, graph, config. Portable.
- **Project**: a workspace. UI state, active agents, workflows, and links to
  one or more brains.
- **Link**: an association between a project and a brain, with a role
  (`primary`, `read-only`, `scratch`).

### 8.2 Operations

- **Create** a brain from a template (`blank`, `research`, `coding`,
  `personal`, or user-defined).
- **Attach** a brain to a project.
- **Detach** a brain from a project (files untouched).
- **Duplicate** a brain (full copy, new ID).
- **Move** a brain (change path, update links).
- **Export/import** as a `.fergbrain` archive (zip of the folder).
- **Template** a brain: any brain can be saved as a template.

### 8.3 Cross-brain operations

- **Search across brains** (multi-brain query).
- **Link a note in brain A to a note in brain B** (soft link by ID).
- **Agent reads from multiple brains** if the project links them.
- **Workflow nodes** can specify which brain to operate on.

### 8.4 Brain identity

Every brain has a stable UUID (`brain_id`) stored in `brain.toml`. This ID is
used for sync, event correlation, and cross-brain references. Path is
incidental.

---

## 9. The permission system

### 9.1 Three modes (the permission bar)

In the chat, the user selects one of:

| Mode | Behavior |
|---|---|
| **Manual** | Every tool call requires approval. Safest. |
| **Auto (risky)** | Safe operations run automatically; risky ones (shell, network, file write outside workspace, cost > threshold) prompt. Default. |
| **Full access** | Everything runs automatically. For power users in trusted contexts. |

The mode is **per session**, can be overridden per tool, and is recorded in
the audit log.

### 9.2 Permission categories

Every tool declares its category and risk:

| Category | Examples | Default in Auto mode |
|---|---|---|
| `fs.read` | Read file, list dir | Allow |
| `fs.write` | Write file, delete file | Prompt if outside workspace |
| `shell.exec` | Run command | Prompt (allowlist configurable) |
| `net.http` | Fetch URL | Prompt for new domains |
| `net.p2p` | Connect to peer | Allow on LAN, prompt on WAN |
| `llm.call` | Call model | Allow if within budget |
| `mcp.connect` | Connect MCP server | Prompt for new servers |
| `plugin.install` | Install plugin | Prompt |
| `agent.spawn` | Create sub-agent | Allow (with limits) |
| `workflow.edit` | Modify workflow | Allow |
| `brain.write` | Modify brain | Allow if within workspace |
| `system.config` | Change system settings | Prompt |

### 9.3 Permission flow

```
Agent wants to run `rm -rf build/`
  → emits `agent.permission_requested`
  → kernel checks current mode + policy
  → if Auto: category is `shell.exec` → prompt
  → UI shows a permission card in the chat
  → user clicks "Allow once" / "Allow for session" / "Deny"
  → kernel records decision, emits `agent.permission_granted/denied`
  → agent proceeds or adapts
```

Every decision is persisted in the audit log with:
- Who (user or auto)
- What (tool, args, hash)
- When
- Why (mode, policy, matching rule)

### 9.4 Policy engine

Rules are declarative (TOML):

```toml
[[rule]]
id = "allow-safe-shell"
match = { category = "shell.exec", command = ["ls", "cat", "grep", "find"] }
action = "allow"

[[rule]]
id = "deny-rm-root"
match = { category = "shell.exec", command_starts_with = "rm -rf /" }
action = "deny"
reason = "destructive"

[[rule]]
id = "prompt-new-domain"
match = { category = "net.http", domain_not_in = "$SESSION_DOMAINS" }
action = "prompt"
```

Rules are evaluated in order. First match wins. Users can edit rules in
settings, and the AI can propose new rules (subject to approval).

---

## 10. The sandbox model

### 10.1 Layers

Sandboxing is layered. Each layer is independent; a compromise in one doesn't
break the others.

| Layer | What it protects | Mechanism |
|---|---|---|
| **Plugin sandbox** | The host from malicious plugins | WASM (wasmtime) + capability permissions |
| **Agent sandbox** | Files outside the workspace | Workspace root + permission prompts + `.fergents` backups |
| **Shell sandbox** | The OS from agent-run commands | Landlock (Linux/WSL), sandbox-exec (macOS), AppContainer/Job Objects (Windows) |
| **Network sandbox** | Data exfiltration | Per-plugin/agent domain allowlists |
| **Crypto** | Data at rest and in transit | SQLCipher (optional), TLS, E2E for P2P |
| **Resource limits** | Runaway agents | Token/cost/time budgets per run |

### 10.2 WASM plugin isolation

- Plugins run in **wasmtime** with the **Component Model**.
- No ambient authority: no file system, no network, no env vars unless
  explicitly granted.
- Host functions implement every capability (fs, net, llm, mcp, storage,
  events). Each host function checks the plugin's granted permissions.
- WASI preview 2 used for portable host interfaces.
- Memory and CPU limits enforced per plugin.

### 10.3 Agent file modifications

When an agent writes to a file:

1. If the file is inside a project workspace **and** a shadow git repo exists:
   - Stage, commit with message `[fergents] <agent> <action>`.
2. Otherwise:
   - Copy the original to `<file>.<timestamp>.fergents` in a
     `.fergents/backups/` folder (outside the workspace, or in it, configurable).
   - Then write.

This guarantees every modification is reversible via:
- `git revert <sha>` (shadow git case), or
- restore from `.fergents` backup (non-repo case).

The UI shows a "Restore" button on every modified file, and a global
"Rollback last agent session" action.

### 10.4 Shell sandbox

When an agent runs a shell command:

- **Linux / WSL**: run under **Landlock** LSM, restricting file system to
  declared paths; network restricted by nftables rules scoped to the process
  (via `unshare` + `slirp4netns` or simpler techniques).
- **macOS**: `sandbox-exec` profile with declared paths.
- **Windows**: Job Objects + restricted token + AppContainer where possible.

If OS-level sandboxing isn't available, the command runs normally but is
logged and requires explicit user approval in Manual and Auto modes.

### 10.5 Network sandbox

- Plugins and agents declare allowed domains.
- The kernel proxies outbound HTTP and enforces the allowlist.
- TLS is always verified.
- For local models (Ollama, llama.cpp), traffic stays on localhost.

---

## 11. Agent runtime overview

Detailed in `AGENTS.md`. Summary:

- **Agent = Markdown file** (`agents/<name>.md`) with YAML frontmatter:
  model, tools, skills, permissions, memory settings, limits.
- **Runtime**: a ReAct-style loop with a planner/executor split, streaming
  tokens, structured tool calls, and a context manager.
- **Memory**: three tiers —
  - Working: the current context window (managed, compacted).
  - Episodic: session history (SQLite).
  - Semantic: the brain (RAG over the vault).
- **Sub-agents**: any agent can spawn a child with a narrower scope and its
  own budget. Communication via a shared channel or direct messages.
- **Multi-agent orchestration**: a "manager" pattern, where one agent
  coordinates several specialists. Also supports peer-to-peer agents (a
  shared channel, like a Slack for agents).
- **Limits**: per-run token, cost, wall-clock, and tool-call limits. Enforced
  by the kernel, not the agent.
- **Persistence**: agents have stable identity, memory, and stats across
  sessions.

---

## 12. MCP as the integration spine

Detailed in `MCP.md`. Summary:

- Fergents is an **MCP host**: it connects to any MCP server (stdio, SSE,
  HTTP) and exposes their tools/resources/prompts to agents and workflows.
- Fergents is an **MCP server**: it exposes its own capabilities (brains,
  search, workflows, agents, dashboard) as MCP tools, so external clients
  (Claude Desktop, Cursor, other Fergents instances) can use Fergents.
- The MCP hub maintains server registrations, health, capabilities, and
  per-server permissions.
- **Sampling** and **roots** are supported.
- **AI can manage MCP servers**: add, remove, configure, test — subject to
  permission.

MCP is the reason Fergents can be a **hub** rather than a **silo**.

---

## 13. Sync and P2P

Detailed in `SYNC.md`. Summary:

- **Discovery**: mDNS on LAN, DHT/rendezvous on WAN.
- **Transport**: libp2p (QUIC + Noise + Yamux). Optional relay for NAT
  traversal.
- **Conflict resolution**: CRDTs for markdown (Automerge or Yjs), event log
  merge for structured data, last-writer-wins with vector clocks for
  metadata.
- **Scope**: per-brain or per-project sync. You choose what to share.
- **Security**: E2E encryption with device keys (X25519). Peers must be
  explicitly paired.
- **LAN-first**: if two devices are on the same Wi-Fi, they sync directly,
  without the internet. This is a hard requirement from the north star.
- **Self-hosted server** (future): a Fergents instance can act as a relay
  and sync hub for a team, replacing P2P when convenient.

---

## 14. Observability and audit

- **Tracing**: OpenTelemetry-compatible, local collector, optional export.
- **Metrics**: token usage, cost, latency, tool calls, error rates, per
  agent/model/brain/plugin.
- **Audit log**: every permission decision, every agent action, every file
  modification. Append-only. Queryable. Exportable.
- **Cost tracking**: per session, per agent, per model, per provider. Budget
  alerts.
- **Run history**: workflows and agent runs, with full trace, replayable.
- **Dashboard**: prebuilt panels + custom YAML widgets (see `DASHBOARD.md`).

All observability data lives **locally**. Nothing is sent anywhere unless
you explicitly configure it.

---

## 15. Reversibility and backups

- **Shadow git**: every project has a `.fergents/shadow.git` repo. Every
  agent action that modifies files commits automatically.
- **`.fergents` backups**: for files outside a repo, or when git isn't
  desired, a timestamped copy is created before modification.
- **Event log**: full history of every state change. Replayable.
- **Brain snapshots**: scheduled or manual full snapshots (`.fergbrain`
  archives).
- **Settings backups**: every settings file modified by an agent gets a
  `.fergents` copy.
- **Rollback UI**: per-file, per-session, per-run rollback. Global rollback
  of "everything this agent did in the last N minutes".

---

## 16. Process lifecycle

```
User launches Desktop App
  → Tauri shell starts
  → checks for fergentsd (named pipe / socket)
  → if absent: spawns fergentsd (detached)
  → handshake, version check
  → loads UI plugins
  → renders shell
  → subscribes to events

User runs `fergents chat` in terminal
  → CLI checks for fergentsd
  → if present: connect
  → if absent: spawn or error (configurable)
  → handshake
  → renders TUI
  → subscribes to events

Kernel idle timeout (default: never; configurable)
  → graceful shutdown
  → flush event log, close SQLite, save state
```

---

## 17. Key decisions

A summary of the most consequential architectural decisions. Full ADRs in
`DECISIONS.md`.

| # | Decision | Rationale |
|---|---|---|
| ADR-001 | Kernel as a separate daemon | Multi-client, headless, testable, syncable |
| ADR-002 | Rust for the kernel | Performance, MCP SDK, WASM, Tauri, safety |
| ADR-003 | Event-sourced with SQLite persistence | Audit, replay, sync, plugin reactivity |
| ADR-004 | Per-brain SQLite, not global | Portability, isolation, multi-attach |
| ADR-005 | MCP as the only external integration surface | Interoperability, no bespoke protocols |
| ADR-006 | WASM (Component Model) for plugins | Security, cross-language, sandboxing |
| ADR-007 | React in Shadow DOM for UI plugins | Isolation, reuse of web ecosystem |
| ADR-008 | Markdown + YAML/TOML for all definitions | Human-readable, AI-editable, git-friendly |
| ADR-009 | LAN-first P2P with libp2p | Requirement from north star, no cloud dependency |
| ADR-010 | `.fergents` backups + shadow git | Reversibility by default |
| ADR-011 | AGPL-3.0 | Protect the commons, allow self-hosting |
| ADR-012 | Tauri v2 for desktop + mobile | Small binaries, Rust backend, single codebase |
| ADR-013 | sqlite-vec + BGE-M3 for embeddings | Embedded, multilingual, offline |
| ADR-014 | No telemetry by default | Privacy-first |
| ADR-015 | AI as a first-class user, no privileged APIs | Consistency, security, auditability |
| ADR-016 | CodeMirror 6 for the Phase 1 markdown editor | Lightweight, extensible; Monaco deferred to Phase 3 |
| ADR-017 | React Flow for Phase 1 graph visualization | Covers pan/zoom/layout; custom WebGL only if it degrades |
| ADR-018 | Swappable `VectorStore`/`Reranker`/`EntityExtractor` traits | Swappable engines; deterministic stubs keep models out of CI |
| ADR-019 | Per-brain FTS5 tokenizer: `unicode61 remove_diacritics 2` | Language-neutral, predictable; configurable per brain |
| ADR-020 | Phase 0 gaps fixed before Phase 1 implementation | Phase 1 builds on the daemon lifecycle and logging |
| ADR-021 | Kernel-authoritative capability gating | One authority; "off" means unallocated; plugins participate |
| ADR-022 | Sandboxed rich outputs (charts, Mermaid, HTML) | Model output is untrusted; the frame plus sanitization contains it |
| ADR-023 | Diff-gated, file-backed `brain.md` | Inspectable, git-versionable, AI-editable, user-controlled |
| ADR-024 | Persisted spec/plan/tasks approval runs | Gates survive restarts; edits are re-read on resume |
| ADR-025 | Secrets in the OS keychain, references in the DB | Keys must not travel with the database |
| ADR-026 | Compare as parallel sessions; scoped time travel | Reuses sessions; restores stay within their blast radius |

---

*End of `ARCHITECTURE.md`.*