# IDE & Codebase — Phase 3 Spec

## Why

Phase 1 delivered the **brain** (markdown vault, index, hybrid search, graph).
Phase 2 (`v0.2.0-phase2`) delivered the **agent runtime and chat UI**: an LLM
router, MCP host, ReAct loop with memory and sub-agents, a permission-governed
tool loop, reversibility, and the chat surface.

What is still missing is the place where the agent's work actually happens:
**the code**. Today Fergents can reason over notes and run tools, but it cannot
open a repository, show a file, ask a language server what a symbol means, run a
command in a real terminal, or turn an agent edit into a reviewed commit.

Phase 3 (`ROADMAP.md` §7) turns Fergents into an **IDE**: Monaco, a kernel-side
LSP client, an integrated terminal, git, inline AI and a Composer, and a split
view that puts the chat next to the code. This completes the MVP.

This spec delivers exactly `ROADMAP.md` §7 and `UI.md` §8, plus the minimum
"codebase awareness" needed for the agent to be useful in a project. It does
not deliver a debugger, notebooks, an extension marketplace, remote
development, or a task/launch configuration system (Appendix G).

## What Changes

Phase 3 is **additive** to the Phase 1 brain and the Phase 2 agent runtime. It
adds a **project** layer (a code workspace, distinct from a brain), new kernel
crates, new capabilities, new protocol commands, and a new desktop area.

### 0. Capability registrations (cross-cutting)

Every Phase 3 feature is a registered **Capability** (Phase 2 D1–D3), off in
the kernel when disabled — not merely hidden in the UI:

| Capability | Default | Depends on | What it allocates when enabled |
|---|---|---|---|
| `capability.ide` | on | — | project layer, file index, editor layout persistence |
| `capability.lsp` | on | `capability.ide` | language-server processes, JSON-RPC pipes, diagnostics store |
| `capability.terminal` | on | `capability.ide` | PTY processes, output rings, shell discovery |
| `capability.git` | on | `capability.ide` | git2 repos, status watchers, shadow-git hooks |
| `capability.git-remote` | **off** | `capability.git` | push/pull/fetch, credential resolution |
| `capability.inline-ai` | **off** | `capability.ide`, `capability.llm` | ghost-text engine, debounce workers, completion cache |
| `capability.composer` | on | `capability.ide`, `capability.agents` | multi-file proposal runs, diff application |

Disabled → the subsystem's processes are never spawned, its commands return
`capability_disabled`, and its events are never published (Phase 2 D3).

### 1. Project model (the codebase layer)

- **Open a folder** as a project (`project.open`), with detection of the VCS
  root, ignore files (`.gitignore`, `.fergentsignore`), and a language map.
- **Workspace trust**: a newly opened project starts **untrusted** (restricted
  mode). LSP servers, terminals, git hooks, and inline AI stay off until the
  user trusts it (`project.trust`). This is a security boundary, not a prompt.
- **File index**: a fuzzy file list and a text search over the project (respect
  ignore rules, binary/large-file guards), used by the quick-open palette and
  by agent context packaging.
- **Projects and brains are separate**: a project may be linked to a brain
  (`project.brain_id`), and the IDE shows both trees in separate sections.

### 2. Editor (Monaco)

- **Monaco** for code, lazily loaded (separate Vite chunk + workers) so the
  chat/brain startup is unaffected. The existing CodeMirror markdown editor
  stays for brain notes; Monaco handles project files.
- Tabs with dirty state, preview vs pinned, close/reorder; breadcrumb;
  minimap; outline (from LSP); find/replace; multi-cursor; go-to-definition;
  peek definition; rename symbol; format on save (configurable).
- **Model per file URI**, one Monaco model per open file, disposed on close.
- **Save** through the kernel (`file.write` with `base_etag`), not the
  filesystem directly: conflicts are detected and surfaced, never silently
  overwritten; every save is reversible (backup + shadow git).
- **External changes** (git checkout, agent edit, another editor) arrive as
  `file.changed`; the open buffer shows a conflict banner with reload/keep.
- **Guards**: binary files are not opened; files above a configurable size
  (default 5 MB) open read-only without LSP features.

### 3. LSP client in the kernel (Rust)

- New crate `crates/lsp`: a JSON-RPC 2.0 client with
  `Content-Length` framing over stdio, per (project, language) server process.
- **Lifecycle**: `initialize` → `initialized` → document sync → `shutdown` →
  `exit`; graceful stop, then kill after a timeout; crash detection with a
  restart policy (max 3 restarts in 5 minutes, then `lsp_server_failed`).
- **Capabilities**: diagnostics (push + pull), completion, hover, definition,
  references, rename, document symbols, workspace symbols, formatting, code
  actions, signature help, inlay hints (rendered where Monaco supports it).
- **Document sync**: incremental (`didChange` with ranged changes), versioned;
  `didOpen` on tab open, `didClose` on close; saves send `didSave` with text.
- **Server discovery**: from `PATH` and an explicit registry
  (`lsp.server.*`); **no automatic downloads or installs**. Missing servers are
  reported as `not_installed` with a docs link. Shipped defaults:
  `rust-analyzer`, `typescript-language-server`, `pyright`, `gopls`, `clangd`,
  `lua-language-server`, `tailwindcss-language-server`.
- **Diagnostics** are stored per project/file and published as
  `lsp.diagnostics`; the Problems panel groups by file and severity.
- **Isolation**: servers are spawned with a scrubbed environment (no secrets),
  `cwd` pinned to the project root, and a working-directory allowlist.

### 4. Integrated terminal

- **xterm.js** in the UI, **`portable-pty`** in the kernel (`crates/terminal`).
- Multiple sessions/tabs, shell selection (pwsh, bash, zsh, fish, cmd), split
  terminal, scrollback ring buffer (bounded, default 1 MB per session).
- Output streams as `term.output` events, batched (~16 ms) to keep the UI
  smooth; input through `term.write`; resize through `term.resize`.
- **The agent can read output and run commands**, always permission-checked:
  - `shell.exec` (Phase 2) remains the non-interactive tool.
  - `term.read` returns a bounded, redacted tail of a session for agent
    context.
  - `term.exec` runs a command in a **visible** terminal tab so the user sees
    what the agent did; it obeys `shell.exec` policy and the shell sandbox.
- Sessions survive window reloads while the kernel lives; they are not
  resurrected after a kernel restart (explicitly out of scope).

### 5. Git integration (Source control)

- **New crate `crates/git`**: `git2` for programmatic operations, with a
  `git` CLI fallback for operations libgit2 does not expose well.
- **Read**: status (branch, ahead/behind, per-file state), diff (working tree,
  staged, single file, unified hunks), log, show, blame, branch list.
- **Write**: stage/unstage (file or hunk), discard (reversible via backup),
  commit (message + optional trailers), branch create/switch. Every mutation
  is permission-checked and audited.
- **Remote** (capability `git-remote`, off by default): fetch/pull/push with
  credential resolution from the OS keychain or SSH agent; **never**
  force-push; never bypass hooks.
- **Agent-authored commits** are labelled: the kernel appends
  `Fergents-Agent: <agent_id>` and `Fergents-Session: <session_id>` trailers
  when the commit originates from an agent or the Composer.
- **Shadow git stays separate**: agent edits are checkpointed to the project's
  `.fergents/shadow.git` continuously; a commit in the user's repository only
  happens on explicit approval. The two histories are never mixed.
- **Conflicts**: detection and a basic three-way resolution UI (choose ours /
  theirs / edit); resolve is a normal file write, hence reversible.

### 6. Inline AI and Composer

- **Ghost text** (capability `inline-ai`, off by default): debounced
  (~300 ms) completion requests at the cursor; ghost text renders via Monaco's
  inline completions; **Tab** accepts, **Esc** dismisses; nothing is inserted
  without an explicit accept. Requests are cancellable and bounded by a token
  budget; completions are suppressed for large files and untrusted projects.
- **Inline quick actions** (`Ctrl+.`): explain, refactor, add tests, document,
  fix. They open a diff preview rather than editing in place.
- **Composer** (capability `composer`, on): a prompt plus explicit context
  (`@file`, `@selection`, the current file, optional brain hits) produces a
  **multi-file proposal** as a run with per-file unified diffs and line stats
  (ADR-032).
  - The user approves per hunk, per file, or all; applying writes through the
    same backup + shadow-git pipeline as any agent edit, then emits
    `composer.applied`.
  - Rejecting discards the run; nothing is written.
- **Agent mode in the IDE** reuses the Phase 2 spec/plan/tasks workflow: the
  artifacts open as editor tabs, gates remain approval cards, and the final
  execution produces a diff preview.
- **Selection → chat**: "Add to chat" attaches the current file and range
  (Phase 2 `chat.attach`), so the conversation is grounded in code.

### 7. Split view (Chat + IDE)

- A **layout tree** (splits and tabs) persisted per project
  (`editor.layout.get` / `editor.layout.set`).
- Default Phase 3 layout: **chat left, IDE right**, resizable, with the
  existing sidebar/rail. Chat and IDE can be split horizontally or vertically
  and nested per `UI.md` §10.
- The splitter is keyboard-accessible (arrow keys move the divider, double
  click resets).
- Tab shortcuts (`Ctrl+Tab`, `Ctrl+W`, `Ctrl+\`) follow `UI.md` §10.2.

### 8. Foundation

- **Global schema**: `projects`, `editor_layouts`, `lsp_servers`,
  `terminal_sessions`, plus a `project_id` column on `runs` (Appendix B).
- **Protocol**: Phase 3 commands and events (Appendix A), generated for Rust
  and TypeScript through the existing pipeline.
- **Reversibility**: every file write, discard, and composer apply goes
  through `.fergents` backups + shadow git (Phase 2), with a restore entry
  point in the UI.
- **Audit**: terminal executions, git mutations, LSP server starts, and
  composer applications are recorded with actor, arguments, result, and a
  reversibility reference.

No **BREAKING** changes: Phase 1/2 commands and events keep working; the chat
and brain surfaces are unchanged; `shell.exec` gains a PTY-backed sibling but
keeps its contract.

## Impact

- **Affected docs (source of truth)**: `ROADMAP.md` §7, §13; `UI.md` §6, §8,
  §9, §10, §11, §13, §19, §20, §26; `STACK.md` §14; `SECURITY.md` §6–§8,
  §11, §15, §16, §22; `ARCHITECTURE.md` §9, §10, §15; `AGENTS.md` §7, §22;
  `DECISIONS.md`; `style/` (MANDATORY design system).
- **Affected code (new)**: `crates/{ide,lsp,terminal,git}`;
  `migrations/global/0012_projects.sql`, `0013_ide.sql`;
  desktop features under `apps/desktop/src/features/{project,ide,terminal,
  source-control,problems,composer}`.
- **Affected code (modified, minimal)**: `crates/core` (service registration +
  capability gates), `crates/api` (dispatch arms + gates), `crates/agents`
  (project context packaging), `crates/reversibility` (shadow-git labels),
  `protocol/schema/protocol.schema.json`, `packages/protocol` (regenerated),
  `apps/desktop/src/App.tsx` (layout tree), `style/components.css`.
- **Out of scope (later phases)**: debugger/DAP, notebooks, extension
  marketplace, remote development, task/launch configurations, multi-window
  tear-off, code review UI beyond the diff preview, semantic code graph
  (Phase 6+).

## Decisions (resolved)

Normative for Phase 3.

| # | Topic | Decision |
|---|---|---|
| D1 | Editor boundary | **Monaco** for project files; CodeMirror remains for brain notes. Monaco is lazy-loaded in its own chunk with dedicated workers; chat/brain startup must not regress. |
| D2 | File I/O authority | The **kernel** performs all project file reads/writes (`file.*`), never the webview. Writes carry `base_etag`; mismatches return `file_conflict` and never overwrite. |
| D3 | Project trust | New projects are **untrusted**: no LSP processes, terminals, git hooks, or inline AI until `project.trust` is granted. Persisted per project and revocable. |
| D4 | LSP placement | One **kernel-side** LSP client per (project, language), spawned on demand and shared by all editor tabs. Servers come from `PATH`/registry only; no auto-install. |
| D5 | LSP failure policy | Restart on crash with backoff, max 3 restarts per 5 minutes, then `lsp_server_failed` and a manual restart button. Diagnostics from a dead server are marked stale. |
| D6 | Terminal placement | PTYs live in the **kernel** (`portable-pty`), xterm.js is a view. Output is event-streamed and ring-buffered; sessions die with the kernel (no resurrection). |
| D7 | Terminal for agents | `shell.exec` stays non-interactive; `term.exec` runs in a visible tab; `term.read` returns a bounded, redacted tail. All three obey the same permission and shell-sandbox policies. |
| D8 | Git library | `git2` (libgit2) with a `git` CLI fallback; **vendored libgit2** for hermetic builds. Operations are read-only by default; mutations require `git.write`. |
| D9 | Commit labelling | Agent/Composer commits append `Fergents-Agent` and `Fergents-Session` trailers. The user's repository is only committed on explicit approval; shadow git is never merged into it. |
| D10 | Remote git | `git-remote` is a **separate, off-by-default** capability; push/pull/fetch require `git.remote` permission; force-push is never offered. |
| D11 | Inline AI | Off by default (per-keystroke token cost). Debounced, cancellable, budgeted; suggestions are never auto-inserted; suppressed in untrusted projects and large files. |
| D12 | Composer application | A Composer run is a Phase 2 **run** (`kind = 'composer'`) whose artifacts are per-file diffs; applying goes through backup + shadow git and is reversible. |
| D13 | Codebase awareness | The agent gets project context through explicit, bounded packaging (open file/selection, fuzzy file list, text search, LSP symbols); no whole-repository embedding in Phase 3. |
| D14 | Layout persistence | A single **layout tree** per project (`editor_layouts.layout` JSON), rendered by one splitter component; defaults restore if the stored tree is invalid. |
| D15 | Diff rendering | Monaco's built-in diff editor in **inline** mode by default, side-by-side on wide panes; hunks are addressable for per-hunk approval (`UI.md` §13). |
| D16 | Testing strategy | Monaco and xterm are wrapped behind kernel-backed adapter interfaces; Vitest tests the adapters and controllers with mocks, Rust tests the LSP framing/PTY/git against local fixtures. No language server or shell is required in CI. |

## ADDED Requirements

### Requirement: Project Layer

The system SHALL open a folder as a project, detect its VCS root and ignore
files, build a file index, and persist trust state.

#### Scenario: Opening a project

- **WHEN** the user opens a folder
- **THEN** the kernel records a project (path, name, VCS root), indexes its
  files respecting `.gitignore`/`.fergentsignore`, and reports it as
  **untrusted**

#### Scenario: Restricted mode

- **WHEN** a project is untrusted
- **THEN** LSP servers, terminals, git hooks, inline AI, and Composer are
  unavailable and their commands return `project_untrusted`

#### Scenario: Trusting a project

- **WHEN** the user trusts a project
- **THEN** the restriction lifts for that project, the decision is audited,
  and revoking trust immediately stops LSP/terminal processes

### Requirement: Kernel File Operations

The system SHALL perform all project file reads and writes in the kernel with
optimistic concurrency and reversibility.

#### Scenario: Conflict on save

- **WHEN** a write's `base_etag` does not match the current file
- **THEN** the kernel returns `file_conflict` with the current etag and makes
  no change

#### Scenario: External change

- **WHEN** a file changes on disk while open
- **THEN** the kernel emits `file.changed` and the editor shows a reload/keep
  banner instead of overwriting

#### Scenario: Reversible write

- **WHEN** a file is written, renamed, or deleted through the IDE
- **THEN** a backup is recorded and a shadow-git checkpoint exists, so the
  action can be reverted

### Requirement: Monaco Editor

The system SHALL provide a Monaco-based editor for project files with tabs,
language support, and LSP-driven features.

#### Scenario: Editing and saving

- **WHEN** the user edits a file and saves
- **THEN** the buffer is written through `file.write`, diagnostics refresh, and
  the tab's dirty marker clears

#### Scenario: Go-to-definition

- **WHEN** the user invokes go-to-definition on a symbol
- **THEN** Monaco requests it from the kernel LSP client and opens the target
  file at the target range

#### Scenario: Large or binary file

- **WHEN** a file exceeds the size limit or is binary
- **THEN** it opens read-only (or is refused) and no LSP features are offered

### Requirement: LSP Client and Server Management

The system SHALL run language servers from the kernel, expose their results to
the editor, and manage their lifecycle.

#### Scenario: Diagnostics

- **WHEN** a language server publishes diagnostics
- **THEN** the kernel stores them, emits `lsp.diagnostics`, and the Problems
  panel and editor squiggles update

#### Scenario: Server missing

- **WHEN** a language server binary is not found
- **THEN** the status is `not_installed` with an actionable message; no
  download is attempted

#### Scenario: Server crash

- **WHEN** a language server exits unexpectedly
- **THEN** it is restarted with backoff up to the limit, then marked failed
  with a manual restart action

#### Scenario: Incremental sync

- **WHEN** the user types in an open file
- **THEN** ranged `didChange` notifications are sent with increasing versions,
  and completion/hover requests see the latest text

### Requirement: Integrated Terminal

The system SHALL provide PTY-backed terminals in the kernel rendered with
xterm.js.

#### Scenario: Interactive shell

- **WHEN** the user opens a terminal in a trusted project
- **THEN** a shell starts in the project root, input/output stream both ways,
  and resizing updates the PTY

#### Scenario: Agent reads output

- **WHEN** the agent calls `term.read`
- **THEN** it receives a bounded, redacted tail of the session's output, never
  the full unbounded buffer

#### Scenario: Agent runs a command visibly

- **WHEN** the agent calls `term.exec` and the permission policy allows it
- **THEN** the command runs in a visible terminal tab, is audited, and its
  output is readable by the agent

### Requirement: Git Integration

The system SHALL expose repository status, diffs, staging, commits, branches,
log, and blame, with agent-authored commits labelled.

#### Scenario: Status and diff

- **WHEN** the user opens Source Control
- **THEN** the branch, ahead/behind, per-file status, and unified diffs are
  shown

#### Scenario: Commit

- **WHEN** the user commits staged changes
- **THEN** the commit is created with the given message, the status refreshes,
  and the operation is audited

#### Scenario: Agent commit labelling

- **WHEN** a commit originates from an agent or the Composer
- **THEN** it carries `Fergents-Agent` and `Fergents-Session` trailers

#### Scenario: Shadow git separation

- **WHEN** the agent edits files without approval
- **THEN** changes are checkpointed only in shadow git; the user's repository
  is untouched until an explicit commit

#### Scenario: Discard is reversible

- **WHEN** the user discards changes
- **THEN** a backup exists and the discard can be reverted

### Requirement: Inline AI (ghost text)

The system SHALL provide opt-in, cancellable completion suggestions rendered as
ghost text.

#### Scenario: Accepting a suggestion

- **WHEN** a suggestion is visible and the user presses Tab
- **THEN** exactly the suggested range is inserted; pressing Esc inserts nothing

#### Scenario: Disabled

- **WHEN** `capability.inline-ai` is off, the project is untrusted, or the file
  is too large
- **THEN** no completion requests are made

#### Scenario: Quick action

- **WHEN** the user runs an inline quick action
- **THEN** the result is presented as a diff preview, not an in-place edit

### Requirement: Composer (multi-file edits)

The system SHALL produce reviewable multi-file edit proposals and apply them
reversibly.

#### Scenario: Proposal

- **WHEN** the user submits a Composer prompt with context
- **THEN** a `kind = 'composer'` run is created whose artifacts are per-file
  unified diffs with line stats

#### Scenario: Partial approval

- **WHEN** the user approves some hunks/files
- **THEN** exactly those changes are written, backups are recorded, and
  `composer.applied` reports the applied set

#### Scenario: Rejection

- **WHEN** the user rejects the proposal
- **THEN** no file is modified and the run is marked rejected

### Requirement: Split View and Layout Persistence

The system SHALL render chat and IDE side by side in a persisted, resizable
layout tree.

#### Scenario: Default layout

- **WHEN** a project is opened for the first time
- **THEN** chat and IDE appear side by side with a draggable divider

#### Scenario: Restoring a layout

- **WHEN** the app restarts
- **THEN** the project's layout tree is restored; an invalid tree falls back to
  the default

#### Scenario: Keyboard resizing

- **WHEN** the divider is focused and arrow keys are pressed
- **THEN** the split resizes and the change is announced to assistive tech

### Requirement: Codebase Awareness and Agent Context

The system SHALL let the agent work with code through explicit, bounded
context and permission-checked tools.

#### Scenario: Selection to chat

- **WHEN** the user chooses "Add to chat" on a selection
- **THEN** the chat attaches that file and range and shows it in the context
  viewer

#### Scenario: Symbol lookup

- **WHEN** the agent needs a symbol
- **THEN** it queries the LSP client (document/workspace symbols) instead of
  reading whole files

#### Scenario: Bounded search

- **WHEN** the agent searches the project
- **THEN** results respect ignore rules, caps, and binary/large-file guards

### Requirement: Phase 3 Protocol Surface

The system SHALL extend the JSON-RPC protocol with the Phase 3 commands and
events (Appendix A), generated into Rust and TypeScript with a freshness test.

#### Scenario: Capability guard

- **WHEN** a Phase 3 command for a disabled capability is dispatched
- **THEN** the kernel returns `capability_disabled` without side effects

#### Scenario: Trust guard

- **WHEN** a command requires a trusted project and the project is untrusted
- **THEN** the kernel returns `project_untrusted` without spawning anything

### Requirement: Phase 3 Schema

The system SHALL add `projects`, `editor_layouts`, `lsp_servers`, and
`terminal_sessions` through new global migrations, and add `project_id` to
`runs` (Appendix B).

#### Scenario: Migration idempotency

- **WHEN** the kernel opens the global store
- **THEN** Phase 3 migrations apply once, are recorded in `schema_version`, and
  an older kernel refuses a newer schema

### Requirement: Desktop IDE UI (design-system compliance)

All Phase 3 UI SHALL be built from `style/` tokens and components, with any new
token/component added to `style/` first and mirrored in `patterns.html`.

#### Scenario: No hardcoded style

- **WHEN** any Phase 3 panel renders
- **THEN** it uses only `style/` tokens (dark-first, glass, accessible)

#### Scenario: Accessibility

- **WHEN** the IDE is used by keyboard or screen reader
- **THEN** tabs, trees, problems, terminal, and splitters are reachable and
  labelled per `UI.md` §19–§20

### Requirement: Testing and CI

Every task SHALL ship tests and keep CI green (existing gates unchanged).
Phase 3 tests SHALL NOT require a language server, a real shell, or a network
connection.

#### Scenario: Kernel tests

- **WHEN** the Rust suite runs
- **THEN** it covers project detection/trust, file etags/conflicts, LSP framing
  and lifecycle against a fake stdio server, PTY echo/read against a local
  shell, git status/diff/commit on temp repositories, and capability/trust
  guards

#### Scenario: Desktop tests

- **WHEN** the Vitest suite runs
- **THEN** it covers the editor host adapter, tabs/dirty state, the problems
  panel, the source-control panel, ghost-text accept/dismiss, the Composer
  diff preview, and layout persistence with mocked Monaco/xterm

## MODIFIED Requirements

### Requirement: Reversibility (Phase 2 §Reversibility)

Shadow git SHALL also cover project files, and every IDE mutation SHALL carry
an actor label so agent, user, and Composer changes are distinguishable.

### Requirement: Permissions (Phase 2 §Permission Bar and Policy Engine)

The policy engine SHALL gain the categories `git.read`, `git.write`,
`git.remote`, `lsp.start`, `term.exec`, and `term.read` (Appendix E), with
defaults that keep reads permissive inside a trusted project and mutations
prompting.

### Requirement: Agent Tools (Phase 2 §Agent Runtime)

The agent tool registry SHALL gain `term.read`, `term.exec`, `code.search`,
`code.symbols`, and `file.read`/`file.write` scoped to the project, all gated
by the same permissions as their command counterparts.

### Requirement: Chat Composer (Phase 2 §Chat Sessions)

The chat composer SHALL accept an `@file`/`@selection` reference from the IDE,
resolved to a bounded file excerpt, and surface it in the context viewer.

## REMOVED Requirements

None.

## Dependencies

New dependencies, each justified (`STACK.md` §7–§9, §14).

| Crate / package | Purpose | Justification |
|---|---|---|
| `lsp-types` | Typed LSP params/results | `STACK.md` §14.2; avoids hand-rolling hundreds of types |
| `portable-pty` | Cross-platform PTY | `STACK.md` §14.3; ConPTY/Unix abstraction |
| `git2` (vendored) | Git operations | `STACK.md` §14.4; hermetic builds |
| `tokio-util` (codec) | Framed stdio for LSP | Already in the workspace, reused |
| `monaco-editor` (npm) | Code editor | `STACK.md` §14.1; decision D1 |
| `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links` (npm) | Terminal view | `STACK.md` §14.3 |
| `react-resizable-panels` (npm) | Accessible split panes | `UI.md` §10; decision D14 |
| `vite-plugin-monaco-editor` (dev, npm) | Monaco workers + lazy chunk | Decision D1; keeps startup cost out of the main bundle |

No dependency shall be added for: debug adapters (DAP), notebooks, extension
hosts, remote development, or code review beyond the diff preview.

## Appendix A: Phase 3 protocol surface

**Command methods (Phase 3)**

- Projects: `project.open`, `project.close`, `project.list`, `project.get`,
  `project.trust`, `project.update`, `project.files` (fuzzy), `project.search`.
- Files: `file.read`, `file.write`, `file.create`, `file.mkdir`, `file.rename`,
  `file.delete`, `file.list`, `file.stat`.
- Editor: `editor.layout.get`, `editor.layout.set`, `editor.tabs.get`,
  `editor.tabs.set`.
- LSP: `lsp.server.list`, `lsp.server.add`, `lsp.server.remove`,
  `lsp.server.start`, `lsp.server.stop`, `lsp.server.restart`,
  `lsp.server.status`, `lsp.document.open`, `lsp.document.change`,
  `lsp.document.close`, `lsp.document.save`, `lsp.completion`, `lsp.hover`,
  `lsp.definition`, `lsp.references`, `lsp.rename`, `lsp.symbols`,
  `lsp.workspace_symbols`, `lsp.format`, `lsp.code_action`,
  `lsp.signature_help`, `lsp.diagnostics.list`.
- Terminal: `term.open`, `term.write`, `term.resize`, `term.close`,
  `term.list`, `term.read`, `term.exec`.
- Git: `git.status`, `git.diff`, `git.stage`, `git.unstage`, `git.discard`,
  `git.commit`, `git.branches`, `git.branch.create`, `git.branch.switch`,
  `git.log`, `git.show`, `git.blame`, `git.conflicts`, `git.resolve`,
  `git.fetch`, `git.pull`, `git.push`.
- Inline AI: `inline.complete`, `inline.cancel`, `inline.action`.
- Composer: `composer.start`, `composer.get`, `composer.apply`,
  `composer.reject`, `composer.cancel`.

**Event kinds (Phase 3)**

```
project.opened / project.closed / project.trusted / project.untrusted
project.index.updated

file.changed / file.conflict / file.saved

lsp.server.starting / lsp.server.ready / lsp.server.failed / lsp.server.stopped
lsp.diagnostics / lsp.log

term.opened / term.output / term.exit / term.closed

git.status.changed / git.commit.created / git.operation.failed / git.conflict

inline.suggested / inline.cancelled

composer.proposed / composer.applied / composer.rejected / composer.failed

layout.updated
```

## Appendix B: Global schema (normative)

New files under `migrations/global/` (numbered after `0011_*.sql`).

- **`projects`** — `id TEXT PRIMARY KEY` (ULID), `path TEXT NOT NULL UNIQUE`,
  `name TEXT NOT NULL`, `vcs TEXT NULL` (`git|none`), `brain_id TEXT NULL`,
  `trust TEXT NOT NULL` (`untrusted|trusted`), `settings TEXT NULL` (JSON),
  `last_opened_at INTEGER NOT NULL`, `created_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`.

- **`editor_layouts`** — `project_id TEXT PRIMARY KEY`, `layout TEXT NOT NULL`
  (JSON layout tree), `updated_at INTEGER NOT NULL`.

- **`lsp_servers`** — `id TEXT PRIMARY KEY`, `language_id TEXT NOT NULL`,
  `command TEXT NOT NULL`, `args TEXT` (JSON array), `env TEXT` (JSON object,
  scrubbed on use), `root_strategy TEXT NOT NULL DEFAULT 'project'`,
  `enabled INTEGER NOT NULL DEFAULT 1`, `source TEXT NOT NULL`
  (`builtin|user|plugin`), `created_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`; unique `(language_id, command)`.

- **`terminal_sessions`** — `id TEXT PRIMARY KEY`, `project_id TEXT NOT NULL`,
  `shell TEXT NOT NULL`, `cwd TEXT NOT NULL`, `cols INTEGER NOT NULL`,
  `rows INTEGER NOT NULL`, `state TEXT NOT NULL` (`running|exited|closed`),
  `exit_code INTEGER NULL`, `actor TEXT NOT NULL` (`user|agent:<id>`),
  `created_at INTEGER NOT NULL`, `closed_at INTEGER NULL`; index
  `(project_id, created_at)`.

- **`runs`** (modified) — add `project_id TEXT NULL` and an index
  `(project_id, created_at)`; `kind` already covers `composer` and `inline`.

## Appendix C: LSP ↔ Monaco mapping

| Editor feature | Kernel command | LSP method |
|---|---|---|
| Diagnostics | `lsp.diagnostics.list` + `lsp.diagnostics` event | `textDocument/publishDiagnostics`, pull diagnostics |
| Completion | `lsp.completion` | `textDocument/completion` + `completionItem/resolve` |
| Hover | `lsp.hover` | `textDocument/hover` |
| Go-to-definition | `lsp.definition` | `textDocument/definition` |
| References | `lsp.references` | `textDocument/references` |
| Rename | `lsp.rename` | `textDocument/prepareRename`, `textDocument/rename` |
| Outline | `lsp.symbols` | `textDocument/documentSymbol` |
| Quick open symbols | `lsp.workspace_symbols` | `workspace/symbol` |
| Format | `lsp.format` | `textDocument/formatting`, `rangeFormatting` |
| Quick fixes | `lsp.code_action` | `textDocument/codeAction` + `codeAction/resolve` |
| Signature help | `lsp.signature_help` | `textDocument/signatureHelp` |
| Document sync | `lsp.document.*` | `didOpen`/`didChange`/`didSave`/`didClose` |

Registration runs once per (project, language): `initialize` with
`rootUri`, workspace folders, client capabilities (incremental sync, snippets,
resolve support), then `initialized`. Requests carry a monotonic id; responses
are matched by id with a per-request timeout (default 10 s, configurable).

## Appendix D: Capability catalogue seed (Phase 3)

| Id | Name | Default | Hot | Dependencies |
|---|---|---|---|---|
| `capability.ide` | IDE | on | yes | — |
| `capability.lsp` | Language servers | on | yes | `capability.ide` |
| `capability.terminal` | Terminal | on | yes | `capability.ide` |
| `capability.git` | Source control | on | yes | `capability.ide` |
| `capability.git-remote` | Git remote | off | yes | `capability.git` |
| `capability.inline-ai` | Inline AI | off | yes | `capability.ide`, `capability.llm` |
| `capability.composer` | Composer | on | yes | `capability.ide`, `capability.agents` |

Defaults follow Phase 2 D15: local, zero-marginal-cost features default on;
networked or token-costly features default off.

## Appendix E: Security constraints

- **Trust gate** (D3): untrusted projects may not start LSP servers, PTYs, git
  hooks, inline AI, or Composer. Trust is per project, persisted, revocable,
  and audited. A trust prompt explains exactly what will be allowed.
- **Terminal**: PTYs obey `SECURITY.md` §8 (Landlock / sandbox-exec / Job
  Objects where available; explicit approval otherwise). Environment is
  scrubbed of secrets; `cwd` is the project root unless the policy allows
  otherwise; every execution is audited with actor, command, exit code, and
  duration.
- **Git**: read commands require `git.read`; mutations require `git.write`;
  remote commands require `git.remote` and the `git-remote` capability.
  Credentials come from the keychain or the SSH agent and are never logged.
  Hooks run normally; force-push is not exposed.
- **LSP**: only executables resolved from `PATH` or the explicit registry;
  environment scrubbed; `cwd` pinned; no shell interpolation of arguments; a
  server that exits is restarted within limits.
- **Inline AI / Composer**: context is bounded and redacted; secrets never
  enter prompts; proposals are diffed and require approval (D11, D12).
- **Files**: `file.*` writes stay inside the project root unless `fs.write`
  policy allows otherwise; symlinks are resolved before containment checks.

## Appendix F: Testing strategy

- **Rust**
  - `crates/lsp`: `Content-Length` framing round-trip, request/response
    correlation, timeout, and lifecycle against an in-repo fake stdio server.
  - `crates/terminal`: PTY open/write/read/resize against a local shell,
    output ring bounds and redaction; skipped when no shell is available.
  - `crates/git`: status/diff/stage/commit/branch/log/blame and trailer
    labelling on temp repositories created by the test (vendored libgit2; no
    network).
  - `crates/ide`: project detection, ignore rules, fuzzy file list, etag
    conflict, path containment, trust transitions.
  - `crates/api`: wired IPC tests for every Phase 3 command, including
    `capability_disabled` and `project_untrusted` guards.
  - `crates/protocol`: round-trip coverage for every Phase 3 type.
- **Desktop (Vitest)**
  - Editor host adapter: model lifecycle, dirty state, save/conflict banner.
  - Problems panel, source-control panel, terminal tab, inline ghost-text
    accept/dismiss, Composer diff preview, layout persistence (invalid tree
    fallback), splitter keyboard behaviour.
  - Monaco and xterm are mocked; no worker or canvas is required in jsdom.
- **Acceptance (manual canary, documented in `checklist.md`)**
  - Open a Rust/TS/Python project; edit, save, see diagnostics.
  - Run a terminal command; the agent reads its output.
  - Agent edits a file; diff shown; approve; commit recorded with trailers.
  - Go-to-definition across the project.
  - Inline AI suggests a completion that respects the project context.
  - Spec/plan/tasks workflow produces usable artifacts.
  - Split view restores across restarts.

## Appendix G: Out of scope (later phases)

- Debugger / DAP, test explorer, and launch/task configurations.
- Notebooks, REPL cells, and Jupyter kernels.
- Extension marketplace and a VS Code-compatible extension host.
- Remote development (SSH containers, devcontainers) and multi-window
  tear-off.
- Full semantic code graph / cross-repo code indexing (revisit in Phase 6).
- Code review beyond the diff preview (PR review UI arrives with Phase 8).
- Terminal session resurrection across kernel restarts.
- Automatic language-server installation.

---

*End of `spec.md` — Phase 3.*
