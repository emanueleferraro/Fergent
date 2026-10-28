<p align="center">
  <img src="https://raw.githubusercontent.com/emanueleferraro/Fergent/main/apps/desktop/src-tauri/icons/icon.png" alt="Fergents" width="96" height="96">
</p>

<h1 align="center">Fergents</h1>

<p align="center">
  A local-first, agent-native workspace that unifies knowledge management, software development, and autonomous AI in a single extensible application.
</p>

<p align="center">
  <a href="https://github.com/emanueleferraro/Fergent/actions/workflows/ci.yml">
    <img src="https://github.com/emanueleferraro/Fergent/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/emanueleferraro/Fergent/releases">
    <img src="https://img.shields.io/badge/version-0.4.0-blue" alt="Version">
  </a>
  <a href="https://github.com/emanueleferraro/Fergent/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
  </a>
  <img src="https://img.shields.io/badge/stack-Rust%20%2B%20React-orange" alt="Stack">
</p>

<p align="center">
  <a href="#overview">Overview</a> ·
  <a href="#features">Features</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#getting-started">Getting Started</a> ·
  <a href="#roadmap">Roadmap</a> ·
  <a href="#contributing">Contributing</a> ·
  <a href="#license">License</a>
</p>

---

## Overview

Modern knowledge work is fragmented across tools that do not share context: notes live in one application, source code in another, AI assistance in a third, and automation in yet another. Every boundary between them costs time and loses information.

Fergents consolidates these workflows into a single workspace in which the agent is a first-class component of the runtime rather than an auxiliary panel. It can read and write notes, edit source code, drive a terminal, orchestrate workflows, and extend its own capabilities through plugins — all within the same application and, by default, on the user's own machine.

Design principles:

- **Local-first.** Data resides on the user's machine. Cloud services are opt-in and optional.
- **Agent-native.** The agent runtime is part of the kernel, not a bolted-on widget.
- **Extensible by design.** Everything above the kernel is a toggleable capability or a sandboxed plugin.

---

## Features

Fergents is developed in modular phases. The current release (v0.4.0) provides the following.

### Knowledge base (Phase 1)

- **Markdown vault.** Plain files on disk, compatible with Obsidian.
- **Hybrid search.** BM25 keyword matching combined with vector-based semantic retrieval.
- **Entity graph.** Relationship visualization across notes, powered by CozoDB.
- **Multiple vaults.** Separate work, personal, and research knowledge bases.

### Agent runtime (Phase 2)

- **Agentic chat.** A minimal conversational interface with streaming responses and inline tool calls.
- **Custom agents.** Personalities, system prompts, and permissions defined in Markdown files.
- **Persistent memory.** Long-term memory (`brain.md`) and session compaction (`session.md`).
- **MCP host.** Connectivity to Model Context Protocol servers such as GitHub and Brave Search.
- **Conversation forking.** Branch conversations and roll back state to any prior point.

### IDE (Phase 3)

- **Monaco editor.** The editor engine used by Visual Studio Code.
- **Language Server Protocol.** Full LSP support (Rust, TypeScript, Python, and others) running in the kernel.
- **Integrated terminal.** xterm.js front end backed by kernel-owned PTYs.
- **Git integration.** Stage, commit, and review diffs without leaving the application.
- **Workspace trust.** New projects are untrusted by default; no code executes until explicitly authorized.

### Plugin Runtime & Skills (Phase 4)

- **WASM sandbox.** Secure, permission-gated execution of third-party plugins.
- **UI extensions.** Plugins can inject React components into 16 defined slots (sidebar, chat, status bar, etc.).
- **Skills runtime.** Reusable, composable capabilities (prompt, tool, code) defined via `SKILL.md`.
- **Plugin Factory.** The AI can scaffold, build, test, and install its own plugins and skills inside the app.
- **Marketplace plumbing.** Support for signed extension indexes and remote installation.

### Core runtime

- **Capabilities.** Features can be disabled at the kernel level to reduce resource usage.
- **Secret vault.** API keys stored in the operating system keychain.
- **Kernel daemon.** A Rust backend (`fergentsd`) shared by the desktop application, CLI, and future mobile clients.

---

## Architecture

Fergents is organized in three layers, separating presentation, execution, and storage.

```text
┌─────────────────────────────────────────────────────────┐
│  CLIENTS  (Desktop UI · CLI TUI · Mobile)               │
│  React 19 + Tauri v2 + Ratatui                          │
└──────────────────────────┬──────────────────────────────┘
                           │ JSON-RPC 2.0 over a local socket
┌──────────────────────────▼──────────────────────────────┐
│  KERNEL DAEMON (fergentsd) — Rust                       │
│  Event bus · LLM router · MCP host · Agent runtime      │
│  LSP client · Git engine · WASM Plugin Host             │
│  Hybrid search · PTY / terminal · Capabilities          │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│  SUBSTRATE                                              │
│  SQLite (per vault) · CozoDB (graph) · Local files      │
└─────────────────────────────────────────────────────────┘
```

- **Rust kernel.** Owns concurrency, performance-critical paths, and the security boundary (including WASM sandboxing).
- **Tauri front end.** A lightweight native webview shell with a Shadow DOM host for UI plugins.
- **Local substrate.** Embedded databases and plain files; no cloud database is required.

---

## Getting Started

### Prerequisites

- **Rust** (stable) and **Node.js** (LTS) with `pnpm`.
- **Windows:** Microsoft C++ Build Tools (with the *Desktop development with C++* workload) and WebView2.
- **Linux / macOS:** the standard Tauri system dependencies.

### 1. Build the kernel

```bash
git clone https://github.com/emanueleferraro/Fergent.git
cd Fergent
cargo build --release -p fergentsd
```

### 2. Run the desktop application

```bash
# On Windows, stage the sidecar binary for Tauri before building the app:
# Copy-Item target\release\fergentsd.exe apps\desktop\src-tauri\binaries\fergentsd-x86_64-pc-windows-msvc.exe

cd apps/desktop
pnpm install
pnpm tauri dev
```

The desktop application spawns `fergentsd` automatically. Closing the window leaves the kernel running in the background; use the CLI or the tray menu to stop it.

### 3. Use the CLI (optional)

```bash
# Check kernel status
cargo run -p fergents -- status

# Create a new vault
cargo run -p fergents -- brain create "My Project" --template coding

# Search
cargo run -p fergents -- search "authentication flow"
```

---

## Roadmap

| Phase | Status     | Scope                                                        |
| :---- | :--------- | :----------------------------------------------------------- |
| 0     | Complete   | Monorepo, kernel skeleton, JSON-RPC transport, CI.           |
| 1     | Complete   | Knowledge base, Markdown, hybrid search, entity graph.       |
| 2     | Complete   | LLM router, MCP, agent runtime, chat UI, memory.             |
| 3     | Complete   | IDE, Monaco, LSP, terminal, Git integration.                 |
| 4     | Complete   | Plugin runtime (WASM), skills, plugin SDK, factory.          |
| 5     | Next       | Visual workflow engine, triggers, debugger.                  |
| 6     | Planned    | Dashboard and custom widgets.                                |
| 7     | Planned    | Peer-to-peer sync, Android companion application.            |

---

## Contributing

Fergents is an open-source project. Contributions, bug reports, and feature requests are welcome.

Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

### Quality gates

Every pull request must pass:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test --workspace`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`

---

## License

Fergents is released under the **MIT License**.

You may use, copy, modify, merge, publish, distribute, sublicense, and sell
copies of the software, provided the copyright notice and this permission
notice are included in all copies or substantial portions of the software.

See [LICENSE](LICENSE) for the full text.
