# Stack

> Every technical choice in Fergents, with motivation and rejected alternatives.

**Audience:** developers, AI developers, architect.
**Companion docs:** `ARCHITECTURE.md`, `DATA_MODEL.sql`, `DECISIONS.md`.

---

## Table of contents

1. [How to read this document](#1-how-to-read-this-document)
2. [Language choices](#2-language-choices)
3. [Desktop shell](#3-desktop-shell)
4. [Storage](#4-storage)
5. [Search & embeddings](#5-search--embeddings)
6. [Graph](#6-graph)
7. [LLM](#7-llm)
8. [MCP](#8-mcp)
9. [Agent runtime](#9-agent-runtime)
10. [Skills](#10-skills)
11. [Workflow engine](#11-workflow-engine)
12. [Plugin runtime](#12-plugin-runtime)
13. [UI framework](#13-ui-framework)
14. [Editor & terminal](#14-editor--terminal)
15. [Sync & P2P](#15-sync--p2p)
16. [Crypto](#16-crypto)
17. [Observability](#17-observability)
18. [Build & CI](#18-build--ci)
19. [Packaging & distribution](#19-packaging--distribution)
20. [Mobile](#20-mobile)
21. [Testing](#21-testing)
22. [Development environment](#22-development-environment)
23. [Summary table](#23-summary-table)

---

## 1. How to read this document

Each section follows the same structure:

- **Choice**: what we picked.
- **Why**: the motivation.
- **Alternatives considered**: what we rejected and why.
- **Caveats**: known limitations and mitigations.

The goal is that an AI developer reading this can make consistent decisions
without asking, and a human reviewer can challenge any choice with full context.

---

## 2. Language choices

### 2.1 Kernel language: **Rust**

**Choice:** Rust (edition 2021, MSRV tracked in `rust-toolchain.toml`).

**Why:**

- **Performance** for the indexer, embeddings, graph, and event bus.
- **MCP**: `rmcp` is the most mature SDK, first-party Rust.
- **WASM host**: `wasmtime` is Rust-native; the Component Model is best
  supported there.
- **Tauri**: the desktop shell's backend is Rust, so one language for
  kernel + shell.
- **Safety**: memory safety without GC pauses; critical for a long-running
  daemon.
- **Ecosystem**: `tokio`, `serde`, `sqlx`, `tree-sitter`, `fastembed-rs`,
  `libp2p`, `ratatui`, `sqlcipher` bindings — all mature.
- **Debuggability for a non-Rust author**: the compiler is a reviewer. When
  an AI writes Rust, the compiler catches entire classes of mistakes.

**Alternatives considered:**

| Language | Why rejected |
|---|---|
| **TypeScript/Node** | Slower; no WASM host story; weak local ML; not suited for a long-running daemon; heavy memory footprint. |
| **Go** | Fast to write, but weaker MCP SDK, weaker WASM, weaker FFI to llama.cpp/sqlite-vec; GC pauses hurt interactive latency. |
| **Python** | Great AI ecosystem, terrible for a daemon (GIL, packaging, startup time, distribution). |
| **Zig** | Too young; ecosystem too small; no MCP SDK. |

**Caveats:** Rust has a steep learning curve. Mitigation: the AI writes it,
the compiler reviews it, and we keep the kernel modular so contributors can
work on isolated crates.

### 2.2 UI language: **TypeScript**

**Choice:** TypeScript 5.x, strict mode, `verbatimModuleSyntax`, no `any` in
public APIs.

**Why:**

- **React ecosystem** for UI.
- **Monaco** and **xterm.js** are TypeScript-native.
- **Shared types** with the kernel via generated bindings (JSON Schema →
  TS types).
- **Fast iteration** for UI work.
- **WASM interop** for UI plugins (JS glue + WASM).

**Alternatives considered:**

- **ReScript / Elm / PureScript**: better type systems, but smaller
  ecosystems; Monaco and xterm.js interop would be painful.
- **JavaScript**: rejected — types are non-negotiable for a project of this
  size.

### 2.3 Plugin language: **any → WASM (Component Model)**

**Choice:** plugins compile to WASM via the Component Model. Supported source
languages: Rust (first-class), TypeScript (via `jco`), Go (via TinyGo), Python
(via componentize-py), C/C++ (via clang), Zig.

**Why:**

- **Language freedom** for plugin authors.
- **Sandboxing** by construction.
- **Deterministic** execution.
- **Portable** across OSes and architectures.

**Alternatives considered:**

- **Native dynamic libraries (.so/.dll/.dylib)**: rejected — no sandbox, ABI
  fragility, security nightmare.
- **JS-only plugins**: rejected — limits authors, slower, harder to sandbox
  properly.
- **Lua**: rejected — limited ecosystem, no strong typing, no WASM story.

### 2.4 Scripting language (user code in workflow nodes): **TypeScript + WASM**

**Choice:** users can write workflow node code in TypeScript (compiled to
WASM on the fly via `jco`/`swc`) or Rust (compiled to WASM via a pre-built
toolchain bundle). Both produce a WASM component with a standard interface.

**Why:**

- **Familiar** to most users.
- **Sandboxed**.
- **Fast enough**.

**Alternatives considered:**

- **Python embedded**: rejected — heavy, hard to sandbox, packaging pain.
- **Lua**: rejected — small ecosystem.
- **Deno/QuickJS**: rejected — less safe than WASM, larger footprint.

---

## 3. Desktop shell

### 3.1 Framework: **Tauri v2**

**Choice:** Tauri v2 with the `wry` webview and `tao` windowing.

**Why:**

- **Small binaries** (~10 MB vs 150+ MB for Electron).
- **Rust backend** shares types and code with the kernel.
- **Capability-based security**: excellent for the permission system.
- **Mobile support** (iOS, Android) from the same codebase — critical for
  the Android companion.
- **Auto-update** built-in.
- **Cross-platform**: Windows, macOS, Linux, Android, iOS.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **Electron** | Too heavy (~150 MB), Node backend, no Rust sharing. |
| **Flutter** | Dart only, no Rust kernel sharing, weaker web ecosystem (Monaco, xterm). |
| **Qt / GTK / native** | Huge effort, poor cross-platform, no web ecosystem. |
| **Web-only PWA** | No native FS, no daemon, no OS-level sandbox. |

**Caveats:**

- Webview differences across platforms (WebView2 on Windows, WKWebView on
  macOS, WebKitGTK on Linux). Mitigation: test on all three; avoid
  platform-specific CSS/JS features; ship a Chromium-based fallback if
  needed.
- Linux WebKitGTK can be slower on complex layouts. Mitigation: profile
  early; use virtualization for long lists.

### 3.2 Webview: **system webview**

**Choice:** system webview by default (WebView2 / WKWebView / WebKitGTK).

**Why:** smaller binaries, better OS integration, auto-updates via the OS.

**Caveats:** behavior differences. Mitigation: a comprehensive visual
regression test suite (see Section 21).

---

## 4. Storage

### 4.1 Primary store: **SQLite**

**Choice:** SQLite (bundled, via `rusqlite` with the `bundled` feature, or
`sqlx` for async).

**Why:**

- **Embedded**, no server, single file.
- **ACID**.
- **Ubiquitous** — every platform, every language.
- **Extensible**: FTS5, JSON1, `sqlite-vec`, SQLCipher.
- **Portable**: copy the file, ship the brain.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **Postgres** | Server required; too heavy for local-first. Kept as a future option for the self-hosted server (via the `Storage` trait). |
| **DuckDB** | Great for analytics, but not designed for transactional/OLTP workloads. Could be added later for analytics widgets. |
| **RocksDB / LMDB** | Lower-level; we'd lose SQL, FTS, and ecosystem. |
| **JSON files** | Not scalable; no transactions. Used only for config. |

**Caveats:**

- Write concurrency: SQLite in WAL mode handles multiple readers + one
  writer. Mitigation: a single writer task in the kernel; readers use
  connection pools.
- Cross-process access: SQLite is safe with WAL but we avoid it — the kernel
  is the single writer.

### 4.2 Vector store: **sqlite-vec**

**Choice:** `sqlite-vec` (embedded, from the same author as `sqlite-vss`).

**Why:**

- **Embedded**: no server, lives in the same SQLite file as the brain.
- **Fast enough**: brute-force with SIMD for up to ~1M vectors per brain;
  ANN available for larger.
- **Portable**: works everywhere SQLite works.
- **Simple**: no external process, no network, no schema duplication.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **Qdrant** | External server; adds deployment complexity. |
| **Chroma** | Python-first; heavy; server or embedded with caveats. |
| **LanceDB** | Excellent, embedded, columnar. Kept as a future option for very large brains (>10M vectors). |
| **Faiss** | Library, not a store; we'd need to manage persistence ourselves. |
| **pgvector** | Requires Postgres. |

**Caveats:**

- `sqlite-vec` is relatively young. Mitigation: wrap it behind a `VectorStore`
  trait so we can swap to LanceDB without touching business logic.
- For brains exceeding ~1M chunks, plan a migration path to LanceDB.

### 4.3 Full-text search: **SQLite FTS5**

**Choice:** FTS5 with the `unicode61` tokenizer, porter stemming for English,
and a custom tokenizer for CJK.

**Why:** integrated, fast, BM25 built-in, no external dependency.

**Caveats:** FTS5 tokenization is not as sophisticated as Lucene/Meilisearch.
Mitigation: hybrid search (BM25 + vector + rerank) hides most of the gap.

### 4.4 Event store: **SQLite (append-only table)**

**Choice:** an `events` table in the global DB and per-brain DB, append-only,
with a ULID primary key and a topic index.

**Why:** durable, replayable, queryable, syncable. See `ARCHITECTURE.md §4`.

**Alternatives considered:**

- **Kafka / NATS / Redis Streams**: external, too heavy for local-first.
- **Files on disk**: harder to query; losing correlation IDs.
- **In-memory + WAL**: not durable enough.

### 4.5 Configuration: **TOML**

**Choice:** TOML for `config.toml`, `brain.toml`, `project.toml`,
`plugin.toml`.

**Why:** human-readable, git-friendly, well-supported in Rust (`toml` crate),
supports comments, typed.

**Alternatives considered:**

- **YAML**: more concise, but error-prone (indentation, type coercion).
  Used for user-authored definitions where conciseness matters (workflows,
  frontmatter in markdown).
- **JSON**: no comments, verbose, not human-friendly for config.
- **JSON5/JSONC**: less standard.

### 4.6 User definitions: **Markdown + YAML frontmatter**

**Choice:** agents, skills, notes are Markdown files with YAML frontmatter.

**Why:** human-readable, AI-editable, git-friendly, familiar to the Obsidian
audience. A user can open a `.md` in any editor.

**Caveats:** frontmatter parsing is permissive; we validate against JSON
Schema and surface errors in the UI.

### 4.7 Backups and snapshots

- **Shadow git** per project: automatic commits.
- **`.fergents`** timestamped copies for out-of-repo files.
- **`.fergbrain`** archives: a zip of the brain folder (vault + index +
  config), versioned and portable.

---

## 5. Search & embeddings

### 5.1 Embedding model: **BGE-M3**

**Choice:** `BAAI/bge-m3` as the default, run locally via **fastembed-rs**
(ONNX Runtime).

**Why:**

- **Multilingual**: 100+ languages, including Italian and English.
- **Strong** on MTEB for retrieval.
- **Long context** (8k tokens).
- **Dense + sparse + multi-vector** in one model (we use dense + sparse for
  hybrid).
- **Local**: no API, no cost, privacy.
- **Fast enough**: ~50 docs/s on modern CPU.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **OpenAI text-embedding-3-large** | Cloud, cost, privacy. |
| **Cohere embed-v3** | Cloud, cost, privacy. |
| **Voyage** | Cloud, cost, privacy. |
| **all-MiniLM-L6-v2** | Smaller but weaker; not multilingual enough. |
| **E5-large** | Good, but BGE-M3 is stronger and multilingual. |
| **nomic-embed-text** | Good, but less multilingual than BGE-M3. |

**Caveats:**

- CPU inference is slower than GPU. Mitigation: batch, cache, allow GPU
  acceleration (CUDA/Metal/Vulkan via ORT) if available.
- Model size (~2.3 GB). Mitigation: download on first use, cache in
  `~/.fergents/models/`.

**Fallback:** for users who prefer an API, we support OpenAI, Voyage, and
Cohere embeddings, selectable per brain.

### 5.2 Reranker: **BGE-reranker-v2-m3**

**Choice:** `BAAI/bge-reranker-v2-m3` via fastembed-rs / ONNX.

**Why:** strong cross-encoder, multilingual, local, improves precision
significantly (top-10 → top-3).

**Caveats:** adds latency (~50-100 ms for 20 candidates). Mitigation: only
rerank top-K (default K=20) and only when the query is non-trivial.

### 5.3 Hybrid search

**Choice:** BM25 (FTS5) + dense (sqlite-vec) + sparse (BGE-M3 sparse
output), fused with **Reciprocal Rank Fusion** (RRF), then reranked.

**Why:** hybrid outperforms any single method, especially on mixed
natural-language + code queries. RRF is simple, robust, and doesn't require
score normalization.

### 5.4 Chunking

**Strategy per content type:**

| Content | Chunker |
|---|---|
| Markdown | Heading-aware, sentence-split, target 512 tokens, overlap 64 |
| Code | tree-sitter AST, split by function/class, target 512 tokens |
| PDF | Layout-aware (heading + paragraph), target 512 tokens |
| HTML | Readability extraction, then markdown chunker |
| Plain text | Sentence-split |

**Why:** chunking quality dominates RAG quality. We invest here early.

---

## 6. Graph

### 6.1 Graph engine: **CozoDB (embedded)**

**Choice:** CozoDB with the SQLite backend (or RocksDB later if needed).

**Why:**

- **Embedded**: same deployment story as SQLite.
- **Datalog**: expressive, supports recursive queries naturally.
- **Fast**: columnar, vectorized.
- **Rust-native**: first-class bindings.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **Kuzu** | Excellent, but Cypher-only; Datalog is more expressive for our use cases. |
| **Neo4j** | Server, heavy, license. |
| **TigerGraph / DGraph** | Cloud/server, overkill. |
| **SQL recursive CTEs** | Workable for simple graphs; painful for multi-hop and ranking. |

**Caveats:**

- CozoDB is less known. Mitigation: wrap behind a `GraphStore` trait.
- For simple graphs (backlinks only), SQL is enough; CozoDB is used for
  entity/relation graphs and advanced queries.

### 6.2 Entity extraction

**Choice:** hybrid — regex + spaCy-style NER via a small local model, plus
LLM-based extraction for high-value documents (opt-in, batched, async).

**Why:** regex catches structured entities (dates, URLs, code identifiers);
NER catches named entities; LLM catches semantic relations.

**Caveats:** LLM extraction costs tokens. Mitigation: on-demand, cached,
per-brain toggle.

---

## 7. LLM

### 7.1 Provider abstraction

**Choice:** a `Provider` trait in Rust with implementations for:

- **OpenAI** (chat, responses, tools, vision, audio)
- **Anthropic** (messages, tools, vision)
- **Google** (Gemini)
- **Mistral**
- **Cohere**
- **Kimi / Moonshot**
- **Alibaba / Qwen** (DashScope)
- **TogetherAI**
- **Groq**
- **OpenRouter**
- **Custom OpenAI-compatible** (any base URL)
- **Local**: Ollama, llama.cpp (via `llama-cpp-rs`), MLX (via bindings on
  Apple silicon)

**Why:** model-agnostic, no lock-in, users choose.

**Caveats:** API differences (tool calling, streaming, vision). Mitigation:
a normalization layer that maps every provider to a common internal schema;
feature flags per provider.

### 7.2 Routing

**Choice:** a declarative router (TOML) that maps tasks to models:

```toml
[[route]]
task = "chat.simple"
model = "ollama:llama3.1:8b"
fallback = ["openai:gpt-4o-mini"]

[[route]]
task = "chat.reasoning"
model = "anthropic:claude-sonnet-4.5"

[[route]]
task = "code.edit"
model = "anthropic:claude-sonnet-4.5"
fallback = ["openai:gpt-4o"]

[[route]]
task = "embed"
model = "local:bge-m3"

[[route]]
task = "rerank"
model = "local:bge-reranker-v2-m3"

[[route]]
task = "summarize"
model = "ollama:qwen2.5:7b"
```

**Why:** routing is essential for cost control and for keeping latency low.
Simple tasks go to local models; only complex tasks hit paid APIs. Fergents
ships with a sensible default routing table, but the user owns it entirely
and can add, remove, or override any route. Budgets are configured by the
user in settings — the application does not assume any figure.

> **Note:** the routes above are a starting point, not a policy. Every route
> can be edited, disabled, or replaced. Budget limits are set by the user in
> `settings.toml` and enforced by the kernel per provider, per agent, and
> globally.

### 7.3 Caching

- **Prompt cache** (provider-side): enabled where supported (Anthropic,
  OpenAI).
- **Local cache**: hash of (model, messages, tools, params) → response.
  Stored in SQLite. TTL configurable. Hit rate shown in dashboard.
- **Semantic cache**: for repeated FAQ-like queries, optional, uses
  embeddings.

### 7.4 Cost tracking

Every request records: provider, model, input tokens, output tokens, cost,
latency, task, agent, session. Aggregated per day/week/month. Budget alerts
fire at user-defined thresholds.

### 7.5 Local models

**Choice:** Ollama as the default local runtime; llama.cpp for embedded use;
MLX on Apple silicon for speed.

**Why:** Ollama is user-friendly, has a big model library, and a stable API.

**Caveats:** Ollama is an external process. Mitigation: embed llama.cpp
directly as an alternative for users who want zero external dependencies.

### 7.6 Multimodal

**Choice:** support images (vision models), audio (Whisper local or API),
video (frame extraction + vision), PDFs (text + optional vision).

**Why:** north star requires it.

**Caveats:** video is expensive. Mitigation: on-demand, sampled frames,
cached.

---

## 8. MCP

### 8.1 SDK: **rmcp**

**Choice:** `rmcp`, the official Rust SDK for MCP.

**Why:** official, maintained, supports stdio/SSE/HTTP, sampling, roots.

**Caveats:** MCP is evolving. Mitigation: pin to a spec version, adapt
incrementally, contribute upstream.

### 8.2 Host and server

- **Host**: connects to external MCP servers, registers their tools,
  resources, prompts in the kernel registry.
- **Server**: exposes Fergents' tools (brains, search, workflows, agents,
  dashboard) as MCP tools for external clients.

**Why both:** interoperability in both directions is core to the vision.

### 8.3 Registry

A local registry of known MCP servers, with metadata: name, transport,
command/URL, capabilities, health, permissions. The UI shows them; the AI
can add/remove them (subject to permission).

---

## 9. Agent runtime

### 9.1 Framework: **custom**

**Choice:** build the agent runtime in Rust, not on top of LangChain/LlamaIndex.

**Why:**

- **Control**: we need fine-grained control over tool calls, permissions,
  streaming, cost, and cancellation.
- **Performance**: fewer layers, lower latency.
- **Consistency**: the runtime shares types with the kernel.
- **No Python dependency**: Rust end-to-end.

**Alternatives considered:**

- **LangGraph (Python)**: powerful, but Python runtime, IPC overhead, extra
  dependency.
- **AutoGen / CrewAI**: Python, opinionated, hard to embed.
- **LlamaIndex**: Python, RAG-focused.
- **Burn / Candle**: model-level, not agent-level.

**Caveats:** we reimplement some patterns (ReAct, planner/executor). This is
acceptable — they are well-understood.

### 9.2 Loop

A ReAct-style loop with:

- **Planner**: decomposes the task.
- **Executor**: calls tools, observes results.
- **Reflector**: evaluates progress, decides to continue/stop/ask user.
- **Context manager**: compacts, summarizes, retrieves from memory.
- **Budget enforcer**: tokens, cost, time, tool calls.

**Caveats:** agent loops can be fragile. Mitigation: budget limits, retry
logic, circuit breakers, and a "pause and ask" mechanism when confidence is
low.

### 9.3 Sub-agents

Any agent can spawn a sub-agent with:

- Its own system prompt, tools, skills, budget, and workspace scope.
- A shared channel for messages, or direct parent-child messaging.

**Why:** complex tasks decompose better into specialists.

---

## 10. Skills

### 10.1 Format: **SKILL.md**

**Choice:** each skill is a folder with `SKILL.md` (YAML frontmatter +
instructions) and optional assets (WASM, data files, prompts).

**Why:** human-readable, AI-editable, git-friendly, portable.

### 10.2 Runtime: **WASM (wasmtime)**

**Choice:** skills with code run as WASM components, sandboxed and
permissioned.

**Why:** security, portability, cross-language.

### 10.3 Marketplace (post-v1)

A curated index of skills, fetched from a git repo, verified by signature,
installable with one click. Skills are free by default; paid skills are a
future consideration.

---

## 11. Workflow engine

### 11.1 Engine: **custom (Rust)**

**Choice:** build a DAG engine in Rust, inspired by Temporal but
lightweight.

**Why:**

- **Control** over the node types, triggers, and persistence.
- **Performance** and no external dependency.
- **Integration** with the event bus (triggers, reactions).
- **Embedded** like everything else.

**Alternatives considered:**

| Option | Why rejected |
|---|---|
| **Temporal** | Server, heavy, overkill for local-first. |
| **Prefect / Airflow** | Python, server, batch-oriented. |
| **Node-RED** | Node.js, aging, not typed. |
| **n8n** | Fair-code (not OSI), Node.js, server. |
| **Windmill** | Server, Rust core but Python-first. |

**Caveats:** we reimplement scheduling, retries, checkpointing. Mitigation:
keep the engine small; use `tokio` + a persisted state machine.

### 11.2 Node types

Built-in:

- `llm` — call a model
- `tool` — call a kernel/plugin/MCP tool
- `agent` — run an agent
- `code` — run user code (WASM/TS)
- `http` — HTTP request
- `condition` — branch
- `loop` — iterate
- `subworkflow` — call another workflow
- `human` — pause for approval
- `trigger` — start node (cron, webhook, event, file watch, MCP)
- `transform` — data manipulation (JSONata-like or JS)

Plugin-defined nodes: any plugin can register new node types.

### 11.3 Triggers

- **Cron** (with timezone)
- **Webhook** (HTTP endpoint on localhost or LAN)
- **File watch** (on a path or a brain)
- **Event** (any kernel event topic)
- **MCP** (a tool call from an external MCP client)
- **Manual** (button in the UI)
- **AI** (an agent can trigger a workflow via a tool call)

### 11.4 Debugger

Step-by-step execution, breakpoints, variable inspection, replay of past
runs, re-run from a step.

---

## 12. Plugin runtime

### 12.1 Kernel plugins: **wasmtime + Component Model**

**Choice:** wasmtime with WASI preview 2 and the Component Model.

**Why:** sandboxed, portable, cross-language, fast.

### 12.2 UI plugins: **React in Shadow DOM**

**Choice:** React 19 components loaded dynamically, mounted in a Shadow DOM
root for style isolation.

**Why:** reuse the web ecosystem; isolate plugin styles from the host.

**Caveats:** Shadow DOM has quirks (event retargeting, focus management).
Mitigation: a small bridge library that standardizes the interactions.

### 12.3 Plugin manifest: **plugin.toml**

Declares id, name, version, API version, permissions, entry points (WASM,
UI), slots.

### 12.4 Installation

- Local `.fergplugin` file (zip)
- Local directory (dev)
- Git URL
- Marketplace (post-v1)

Signature verification: every official plugin is signed; unsigned plugins
show a warning and require explicit user consent.

---

## 13. UI framework

### 13.1 Framework: **React 19**

**Choice:** React 19 with concurrent features, Suspense, Server Components
not used (desktop app).

**Why:**

- **Ecosystem**: Monaco, xterm.js, React Flow, Radix, etc.
- **Community**: largest.
- **Type safety** with TypeScript.
- **Concurrent rendering**: good for complex UIs.

**Alternatives considered:**

- **Svelte 5**: elegant, but smaller ecosystem for our needs.
- **Solid**: fast, but smaller ecosystem.
- **Vue**: good, but weaker TS story than React in 2025.

### 13.2 Styling: **CSS Modules + CSS custom properties**

**Choice:** CSS Modules for component styles, CSS custom properties for
theming (as in `/style/tokens.css`).

**Why:** matches the existing styleguide; no CSS-in-JS runtime overhead;
themeable at runtime.

**Alternatives considered:**

- **Tailwind**: fast to write, but conflicts with the token-based approach
  of the styleguide.
- **Styled-components / Emotion**: runtime overhead, not aligned with the
  styleguide.
- **Vanilla Extract**: excellent, but adds a build step and less familiar.

### 13.3 State management

- **Server state** (kernel): TanStack Query over the kernel protocol.
- **Client state** (UI): Zustand for local UI state; React Context for
  theme/i18n.
- **Event subscriptions**: a custom hook that wraps the kernel's event
  stream.

### 13.4 Component library

Base components come from the existing styleguide (`/style/components.css`).
Fergents-specific components (chat, diff, workflow, graph) are built on top
and documented in `STYLEGUIDE_EXTENSIONS.md`.

### 13.5 Accessibility

- WCAG 2.2 AA target.
- Keyboard-first: every action reachable via keyboard.
- Screen reader tested (NVDA, VoiceOver).
- Reduced motion respected.
- High contrast mode.

---

## 14. Editor & terminal

### 14.1 Editor: **Monaco**

**Choice:** Monaco Editor (the editor that powers VS Code).

**Why:** mature, LSP-ready, feature-rich, well-documented, TypeScript-native.

**Alternatives considered:**

- **CodeMirror 6**: lighter, but fewer features out of the box; weaker
  TypeScript story for LSP integration.
- **Ace**: aging.

### 14.2 LSP: **custom client**

**Choice:** implement an LSP client in the kernel (Rust) that spawns language
servers and proxies requests to Monaco.

**Why:** centralized LSP management; works even in CLI mode; plugins can add
language servers.

**Language servers shipped by default:** rust-analyzer, typescript-language-server,
pyright, gopls, clangd, lua-language-server, tailwindcss-language-server.

### 14.3 Terminal: **xterm.js + portable-pty**

**Choice:** xterm.js in the UI, `portable-pty` in the kernel for PTY
management.

**Why:** standard, feature-rich, cross-platform.

**Caveats:** Windows ConPTY quirks. Mitigation: use `portable-pty`'s
abstractions and test on Windows.

### 14.4 Git integration: **libgit2 (via git2-rs) + git CLI fallback**

**Choice:** `git2` for programmatic operations (status, diff, commit,
blame), `git` CLI for operations not exposed by libgit2 (some rebases,
submodule edge cases).

**Why:** performance and programmatic control; CLI fallback for edge cases.

---

## 15. Sync & P2P

### 15.1 Discovery

- **LAN**: mDNS (`_fergents._udp.local`).
- **WAN**: libp2p Kademlia DHT + a small rendezvous service (self-hostable).

### 15.2 Transport: **libp2p**

**Choice:** libp2p with QUIC, Noise, Yamux.

**Why:**

- **QUIC**: fast, multiplexed, encrypted by default.
- **Noise**: authenticated encryption.
- **Yamux**: multiplexing.
- **NAT traversal**: hole punching, relays.
- **Mature** in Rust (`rust-libp2p`).

**Alternatives considered:**

- **Syncthing as a library**: not designed to be embedded.
- **WireGuard**: too low-level, requires root.
- **Custom TCP + TLS**: reinventing, no NAT traversal.
- **WebRTC**: browser-oriented, less control.

### 15.3 Conflict resolution

- **Markdown files**: CRDT via **Automerge** or **Yjs**. Automerge
  preferred for its Rust maturity and file-based sync model.
- **Structured data**: event log merge with vector clocks; last-writer-wins
  for metadata.
- **Binary files**: last-writer-wins with conflict copies.

### 15.4 Encryption

- **Identity**: each device has an X25519 key pair, stored in the OS keychain.
- **Pairing**: QR code or short code; peers must be explicitly paired.
- **Session**: Noise XX handshake, then QUIC.
- **Data at rest**: optional SQLCipher per brain.

### 15.5 Self-hosted server (future)

A Fergents instance with the `--server` flag runs headless, exposes a relay
for P2P, and optionally hosts shared brains for a team. Same codebase, same
protocol.

---

## 16. Crypto

### 16.1 At rest

- **SQLCipher** (optional, per brain): full-database encryption.
- **Key storage**: OS keychain (Windows Credential Manager, macOS Keychain,
  libsecret on Linux).
- **File encryption**: none by default (markdown is plain); users can enable
  per-brain encryption.

### 16.2 In transit

- **Local IPC**: OS-level (named pipes / Unix sockets), no TLS needed.
- **Remote (mobile, P2P)**: TLS 1.3 or QUIC with Noise.
- **API calls**: TLS 1.3, certificate pinning optional.

### 16.3 Primitives

- **libsodium** for symmetric encryption (XChaCha20-Poly1305).
- **X25519** for key exchange.
- **Ed25519** for signatures (plugin signing, device identity).
- **Argon2id** for password-based key derivation.

### 16.4 Secrets

- API keys are never stored in plaintext files. They go to the OS keychain.
- The kernel exposes them to plugins only through a `secrets.get` host
  function, subject to permission.
- The AI never sees raw keys; it can request an operation that uses them.

---

## 17. Observability

### 17.1 Tracing: **OpenTelemetry (local)**

**Choice:** OTel-compatible spans, exported to a local collector by default,
optional export to Jaeger/Tempo.

**Why:** standard, tooling-rich, future-proof.

### 17.2 Metrics: **custom + Prometheus export**

Internal metrics (tokens, cost, latency, errors) stored in SQLite; exposed
via a Prometheus endpoint on localhost for external dashboards.

### 17.3 Logs: **`tracing` crate**

Structured logs, JSON format, rotated files in `~/.fergents/logs/`.

### 17.4 Audit log

Append-only, in SQLite, queryable. Records every permission decision and
every agent action. Exportable.

### 17.5 Dashboard

See `DASHBOARD.md`. Prebuilt panels + custom widgets (YAML).

---

## 18. Build & CI

### 18.1 Monorepo: **Cargo workspace + pnpm workspace**

- **Rust**: Cargo workspace (`Cargo.toml` at root, crates in `crates/`).
- **TypeScript**: pnpm workspace (`pnpm-workspace.yaml`, packages in
  `packages/`).
- **Shared**: `protocol/` (JSON Schema), generated bindings in
  `crates/protocol` and `packages/protocol`.

### 18.2 Build tool: **Cargo + Vite**

- **Rust**: `cargo build`.
- **UI**: Vite 5 for dev and build.
- **Tauri**: `tauri build` for desktop, `tauri android build` for mobile.

### 18.3 CI: **GitHub Actions**

Workflows:

- **ci.yml**: lint (clippy, eslint), test (cargo test, vitest), build
  (cargo build, vite build).
- **release.yml**: tag-triggered, builds all platforms, signs, uploads to
  GitHub Releases.
- **nightly.yml**: builds nightly binaries for early testers.
- **docs.yml**: builds and deploys documentation (mdBook + typedoc).

Matrix: Windows, macOS (x64, arm64), Linux (x64, arm64), Android.

### 18.4 Caching

- **Rust**: `sccache` for build artifacts.
- **Node**: pnpm store cache.
- **WASM**: pre-built toolchains cached.

### 18.5 Quality gates

- **Rust**: `cargo clippy --deny warnings`, `cargo fmt --check`, `cargo
  audit`, `cargo deny`.
- **TS**: `eslint`, `prettier`, `tsc --noEmit`, `vitest`.
- **Coverage**: minimum threshold enforced in CI (target 70% for kernel,
  60% for UI).

---

## 19. Packaging & distribution

### 19.1 Desktop

- **Windows**: `.msi` (WiX) + `.exe` (NSIS) + Microsoft Store (future).
- **macOS**: `.dmg` + Homebrew Cask, notarized.
- **Linux**: `.AppImage`, `.deb`, `.rpm`, Flatpak, Snap, AUR.

### 19.2 CLI

- **Windows**: Scoop, Winget, `.exe`.
- **macOS**: Homebrew.
- **Linux**: `.deb`, `.rpm`, AUR, `cargo install` for source.

### 19.3 Mobile

- **Android**: `.apk` (direct) + Play Store (future).
- **iOS**: TestFlight + App Store (future, if demand exists).

### 19.4 Auto-update

Tauri's updater, with a stable and beta channel. Signed updates.

---

## 20. Mobile

### 20.1 Framework: **Tauri Mobile**

**Choice:** Tauri Mobile (v2) with a shared codebase for the shell, and
React Native Web for some UI, or plain React (Tauri uses a webview).

**Why:** single language (Rust + TS), single protocol, same kernel client.

### 20.2 Scope (companion only)

- Chat with agents
- View brains (read/write notes)
- Monitor runs and dashboards
- Respond to permission requests
- Receive notifications

No heavy compute (no local LLM, no indexing).

### 20.3 Connection

- LAN: mDNS discovery, direct connection to desktop kernel.
- WAN: relay or direct P2P with paired keys.
- Offline: local SQLite cache; sync when reconnected.

---

## 21. Testing

### 21.1 Kernel (Rust)

- **Unit**: `cargo test` per crate.
- **Integration**: `cargo test --test` with real SQLite, real WASM.
- **Property-based**: `proptest` for parsers, chunkers, sync.
- **Fuzzing**: `cargo-fuzz` for protocol, MCP, plugin host.

### 21.2 UI (TypeScript)

- **Unit**: Vitest.
- **Component**: Testing Library.
- **E2E**: Playwright (via Tauri's webdriver, or the browser build).
- **Visual regression**: Playwright screenshots + diff.

### 21.3 Workflow & Agent

- **Scenario tests**: golden files for known agent tasks.
- **Workflow tests**: run workflows with mocked LLMs, assert outputs.
- **Cost tests**: assert budget enforcement.

### 21.4 Sync & P2P

- **Two-kernel integration**: spin two kernels, sync, assert convergence.
- **Network chaos**: simulate packet loss, partitions.
- **CRDT tests**: property-based merge tests.

### 21.5 Performance

- **Benchmarks**: `criterion` for hot paths (indexing, search, agent loop).
- **Load tests**: 100k notes, 1M chunks, 10 agents in parallel.

### 21.6 Visual & accessibility

- **Visual regression**: Playwright + `pixelmatch`, run on every PR.
- **Accessibility**: `axe-core` in CI, manual screen reader testing before
  releases.

---

## 22. Development environment

### 22.1 Primary OS: **Windows + WSL2**

- **Windows**: Tauri build target; sandbox via Job Objects / AppContainer.
- **WSL2 (Ubuntu)**: primary dev environment; Linux sandbox (Landlock) can
  be tested here.
- **Cross-compilation**: build Linux binaries in WSL; Windows binaries on
  Windows; macOS binaries via GitHub Actions (macOS runners).

### 22.2 Tooling

- **Rust**: `rustup`, `cargo`, `clippy`, `rustfmt`, `cargo-nextest`,
  `cargo-watch`, `sccache`.
- **Node**: `pnpm`, `node` (LTS), `vite`.
- **WASM**: `wasm-tools`, `wasmtime`, `jco`, `cargo-component`.
- **Tauri**: `cargo tauri`, `tauri-cli`.
- **SQLite**: `sqlite3` CLI, DB Browser for SQLite.
- **Git**: `git`, `gh` CLI.
- **Devcontainers**: optional, for reproducibility.

### 22.3 Devcontainer (recommended)

A `.devcontainer/` config with all toolchains, usable in VS Code or
JetBrains. This is the reference environment for the AI developer.

### 22.4 Editor

VS Code (or Cursor, or Trae) with:

- `rust-analyzer`
- `eslint`, `prettier`
- `tailwindcss` (if any)
- `sqlite` viewer
- `mdx` / `markdown` preview
- Tauri extension

---

## 23. Summary table

| Concern | Choice | Rejected |
|---|---|---|
| Kernel language | Rust | TS, Go, Python |
| UI language | TypeScript | JS |
| Plugin runtime | WASM (wasmtime, Component Model) | native, JS-only, Lua |
| Desktop shell | Tauri v2 | Electron, Flutter, Qt |
| Storage | SQLite (per brain) | Postgres, DuckDB, RocksDB |
| Vectors | sqlite-vec | Qdrant, Chroma, LanceDB (future) |
| FTS | FTS5 | Meilisearch, Tantivy |
| Graph | CozoDB | Kuzu, Neo4j, SQL |
| Embeddings | BGE-M3 (fastembed) | OpenAI, Cohere, Voyage |
| Rerank | BGE-reranker-v2-m3 | Cohere rerank |
| LLM | provider-agnostic (Rust trait) | single-provider |
| Local LLM | Ollama + llama.cpp | LM Studio, vLLM |
| MCP | rmcp | custom |
| Agent framework | custom (Rust) | LangGraph, AutoGen |
| Workflow | custom (Rust) | Temporal, Prefect, n8n |
| UI framework | React 19 | Svelte, Solid, Vue |
| Styling | CSS Modules + tokens | Tailwind, CSS-in-JS |
| Editor | Monaco | CodeMirror, Ace |
| Terminal | xterm.js + portable-pty | custom |
| Git | git2 + CLI fallback | CLI-only |
| Sync | libp2p (QUIC, Noise) | Syncthing, WireGuard |
| CRDT | Automerge | Yjs |
| Crypto | libsodium, X25519, Ed25519 | OpenSSL |
| Tracing | OpenTelemetry | custom |
| Logs | tracing crate | log crate |
| CI | GitHub Actions | GitLab CI, CircleCI |
| Packaging | Tauri bundles + Homebrew/Scoop/etc. | custom installers |
| Mobile | Tauri Mobile | React Native, Flutter |
| Testing | cargo test, vitest, playwright | jest, cypress |
| Dev env | Windows + WSL2 | macOS, Linux-only |

---

*End of `STACK.md`.*