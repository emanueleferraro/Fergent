# UI

> The user interface: layout, chat-first paradigm, IDE in parallel, command
> palette, plugin slots, keyboard model, accessibility.

**Audience:** developers, AI developers, designers.
**Companion docs:** `ARCHITECTURE.md`, `PLUGINS.md`, `DASHBOARD.md`,
`/style/readme.md`, `STYLEGUIDE_EXTENSIONS.md`.

---

## Table of contents

1. [Design philosophy](#1-design-philosophy)
2. [The chat-first paradigm](#2-the-chat-first-paradigm)
3. [The layout system](#3-the-layout-system)
4. [The rail](#4-the-rail)
5. [The sidebar](#5-the-sidebar)
6. [The main area](#6-the-main-area)
7. [The chat panel](#7-the-chat-panel)
8. [The IDE panel](#8-the-ide-panel)
9. [The status bar](#9-the-status-bar)
10. [Tabs and splits](#10-tabs-and-splits)
11. [The command palette](#11-the-command-palette)
12. [The permission bar](#12-the-permission-bar)
13. [The diff viewer](#13-the-diff-viewer)
14. [The agent status panel](#14-the-agent-status-panel)
15. [Notifications](#15-notifications)
16. [Settings](#16-settings)
17. [Themes](#17-themes)
18. [Internationalization](#18-internationalization)
19. [Keyboard model](#19-keyboard-model)
20. [Accessibility](#20-accessibility)
21. [Motion](#21-motion)
22. [Plugin UI slots](#22-plugin-ui-slots)
23. [The desktop shell](#23-the-desktop-shell)
24. [The CLI](#24-the-cli)
25. [The mobile companion](#25-the-mobile-companion)
26. [Performance](#26-performance)
27. [Observability](#27-observability)

---

## 1. Design philosophy

- **Chat-first, not chat-only.** The chat is the primary interface, but the
  IDE, brain, workflow, and dashboard are parallel, first-class surfaces.
- **Everything is reachable from the keyboard.** No mouse required.
- **Everything is discoverable from the command palette.** No hidden
  features.
- **Dense but calm.** Information-rich, never cluttered.
- **Dark-first, glass, minimal.** Enterprise-grade, following `/style/`
  religiously.
- **Composable.** Every panel is a plugin; the shell only orchestrates.
- **The AI is visible.** The user always sees what the AI is doing, why, and
  at what cost.
- **Reversible by default.** Every action has an undo affordance.
- **Accessible.** WCAG 2.2 AA. Keyboard, screen reader, contrast, reduced
  motion.

The styleguide (`/style/styleguide.html`) is the source of truth for visual
decisions. This document is the source of truth for layout and interaction.

---

## 2. The chat-first paradigm

Fergents opens on the chat. The chat is not a sidebar widget: it is the
primary surface, always present unless explicitly hidden.

### 2.1 Why chat-first

- The AI is the primary way to interact with a local-first, agent-native
  workspace.
- Chat is universal: it works for questions, commands, workflows, code,
  notes, and automation.
- Everything else (IDE, brain, workflow, dashboard) is reachable from the
  chat, either by asking or by opening a panel.

### 2.2 What the chat is not

- It is not the only way to do things. Every action available in chat is
  also available through the UI.
- It is not modal. The user can work in the IDE while the agent runs in the
  chat.
- It is not the source of truth. Sessions are persisted, replayable, and
  exportable.

### 2.3 Parallel surfaces

The chat and the IDE are parallel. The user can:

- Chat in one pane, code in another.
- Resize the split.
- Pop the chat into its own window.
- Have multiple chats (multi-model, parallel tasks).

---

## 3. The layout system

### 3.1 Default layout

```
┌──────┬─────────────────────────────────────────────────────────────┐
│      │  Tabs  [Chat] [main.rs] [notes/MCP.md] [+]                  │
│      ├───────────────────────────────────┬─────────────────────────┤
│      │                                   │                         │
│ Rail │                                   │                         │
│      │           MAIN AREA               │      RIGHT PANEL        │
│      │      (chat, editor, graph,        │   (context, backlinks,  │
│      │       workflow, dashboard)        │    outline, inspector)  │
│      │                                   │                         │
│      │                                   │                         │
│      ├───────────────────────────────────┴─────────────────────────┤
│      │  Status bar: model · tokens · cost · sync · agents · perms  │
└──────┴─────────────────────────────────────────────────────────────┘
```

### 3.2 Regions

| Region | Width | Collapsible | Contents |
|---|---|---|---|
| **Rail** | 48 px | yes | Icons for top-level sections |
| **Sidebar** | 240-320 px | yes | Files, tags, backlinks, agents, workflows |
| **Main area** | flexible | — | Tabs (chat, editor, graph, workflow, dashboard) |
| **Right panel** | 280-400 px | yes | Context-sensitive info |
| **Status bar** | 28 px | no | Live system state |

### 3.3 Presets

The user can switch between layout presets:

- **Chat**: chat centered, sidebar collapsed.
- **IDE**: editor full, chat on the right.
- **Brain**: notes + graph + backlinks.
- **Workflow**: canvas + inspector.
- **Dashboard**: grid of widgets.
- **Focus**: single pane, everything else hidden.

Presets are saved per project.

---

## 4. The rail

The rail is the leftmost 48 px column. It contains:

- **Chat** (default)
- **Brain** (notes, graph)
- **IDE** (code)
- **Workflow**
- **Dashboard**
- **Agents**
- **Plugins**
- **Settings**

The active section is highlighted with an accent bar. Clicking an icon
switches the sidebar and the main area.

The rail is collapsible (Ctrl+B). When collapsed, only icons remain.

Plugins can add rail items.

---

## 5. The sidebar

The sidebar shows the contents of the active section. It is context-sensitive.

### 5.1 Chat

- Session list (searchable, sortable).
- New session button.
- Folders for sessions (optional).
- Recent agents.

### 5.2 Brain

- File tree (folders, notes).
- Tag list.
- Backlinks panel (for the active note).
- Outline (for the active note).
- Search box (inline).

### 5.3 IDE

- File tree (project).
- Open editors.
- Outline.
- Source control (git).
- Problems (diagnostics).

### 5.4 Workflow

- Workflow list.
- Runs (recent).
- Triggers.
- Node palette (when editing).

### 5.5 Dashboard

- Dashboard list.
- Widget library (drag to add).

### 5.6 Agents

- Agent list (with status).
- Active sessions.
- Sub-agent trees.
- Channels.

### 5.7 Plugins

- Installed plugins.
- Available updates.
- Marketplace (post-v1).

### 5.8 Settings

- Categories (General, Models, Permissions, MCP, Sync, Appearance, Advanced).

The sidebar is resizable (drag the edge). Width is persisted per section.

---

## 6. The main area

The main area hosts tabs. Each tab is a **view**. Views are:

- **Chat** (default)
- **Editor** (Monaco, for a file)
- **Markdown** (for notes)
- **Graph** (brain graph)
- **Canvas** (Obsidian-style canvas)
- **Workflow** (visual editor)
- **Workflow run** (trace view)
- **Dashboard** (widgets)
- **Settings** (opened in a tab, not a modal)
- **Diff** (side-by-side or inline)

Tabs can be:

- **Pinned** (always visible).
- **Preview** (replaced by the next preview).
- **Split** (vertical or horizontal).
- **Floating** (in a separate window).

### 6.1 Tab bar

- Left: tabs, with icons.
- Right: split controls, close-all, tab overflow menu.

### 6.2 Tab actions

- Close (Ctrl+W).
- Close others.
- Close to the right.
- Pin/unpin.
- Duplicate.
- Move to a new window.
- Rename (for chat sessions).

### 6.3 Splits

- Split horizontal (Ctrl+\).
- Split vertical (Ctrl+Shift+\).
- Resize by dragging the gutter.
- Close a split by closing all its tabs.

Splits are a tree. The layout is persisted per project.

---

## 7. The chat panel

### 7.1 Anatomy

```
┌───────────────────────────────────────────────────────────────────┐
│ Session: "MCP research"  ·  Agent: Researcher  ·  Model: Sonnet   │
├───────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─ user ──────────────────────────────────────────────────────┐  │
│  │ Research MCP security and write a note.                      │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                   │
│  ┌─ assistant ─────────────────────────────────────────────────┐  │
│  │ I'll plan this in 3 steps:                                  │  │
│  │ 1. Search the brain for existing notes.                     │  │
│  │ 2. Search the web for recent sources.                       │  │
│  │ 3. Write a note.                                            │  │
│  │                                                             │  │
│  │ ▸ Thinking (collapsed)                                      │  │
│  │                                                             │  │
│  │ ⚙ brain.search(query="MCP security")           [OK · 42ms]  │  │
│  │ ⚙ web.search(query="MCP security 2025")        [OK · 1.2s]  │  │
│  │ ⚙ brain.write(path="research/mcp-security.md") [OK · 18ms]  │  │
│  │                                                             │  │
│  │ Done. I wrote research/mcp-security.md with 5 sources.      │  │
│  │                                                             │  │
│  │ [+12 lines] [−0 lines]  ·  2,341 tokens  ·  $0.03          │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                   │
├───────────────────────────────────────────────────────────────────┤
│ Permission:  [ Manual ] [ Auto ● ] [ Full ]      Model: Sonnet ▾  │
├───────────────────────────────────────────────────────────────────┤
│ [📎] Ask anything…                                        [↑ send]│
└───────────────────────────────────────────────────────────────────┘
```

### 7.2 Message types

- **user**: right-aligned or full-width, plain.
- **assistant**: full-width, markdown, with collapsible thinking.
- **tool**: inline card with icon, name, args, status, duration.
- **system**: subtle, centered, for notices.
- **error**: highlighted, with a retry button.

### 7.3 Thinking

For reasoning models, the thinking stream is shown in a collapsible block.
By default collapsed; the user can expand it. It is not part of the
conversation context by default (only the final answer is).

### 7.4 Tool calls

Each tool call is a card:

- Icon (by category).
- Tool name.
- Arguments (collapsible JSON).
- Status (running, ok, error, denied).
- Duration.
- Result preview (expandable).

Clicking opens a detail panel with the full input/output.

### 7.5 Streaming

- Tokens stream in real time.
- A cursor blinks at the end.
- The "stop" button is always visible during a run.
- Scrolling auto-follows unless the user scrolls up.

### 7.6 Diff preview

When the agent proposes a file edit, a diff is shown inline:

- Inline diff (default).
- Side-by-side (toggle).
- Approve / Reject / Edit buttons.

Approval can be per-file or per-batch.

### 7.7 Attachments

The composer supports:

- Drag-and-drop files.
- Paste images.
- `@` mentions (files, notes, agents, workflows).
- `/` commands (slash commands).
- Voice input (Whisper local).

### 7.8 Multi-model

A session has a primary model but can run **parallel branches** with
different models. The UI shows them side by side. The user picks the best
or merges.

### 7.9 Session list

Sessions are listed in the sidebar. Each has:

- Title (auto-generated or manual).
- Agent, model.
- Last activity.
- Cost, tokens.
- Status.

Search, rename, delete, export.

---

## 8. The IDE panel

### 8.1 Editor

Monaco with:

- Full syntax highlighting.
- LSP diagnostics (squiggles, hover, completion).
- Multi-cursor, column selection.
- Find/replace (regex, case, whole word).
- Go-to-definition, go-to-symbol.
- Peek definition.
- Rename symbol.
- Format on save (configurable).
- Minimap.
- Breadcrumb.
- Outline.

### 8.2 File tree

- Project files + brain files (in separate sections).
- Git status indicators (modified, added, untracked).
- Context menu: new file, new folder, rename, delete, copy path.
- Drag to move.
- Filter/search.

### 8.3 Terminal

- xterm.js.
- Multiple tabs.
- Shell selector (bash, zsh, pwsh, fish).
- Split terminal.
- The agent can read the output and run commands (with permissions).

### 8.4 Source control

- Git status.
- Diff view.
- Stage, unstage, commit, push, pull.
- Branch switcher.
- Commit history.
- Blame (inline).

### 8.5 Problems

- Diagnostics from LSP.
- Grouped by file, severity.
- Click to jump.

### 8.6 Inline AI

- Ghost text completions (Tab to accept).
- Inline quick actions (Ctrl+.: explain, refactor, add tests).
- Composer (multi-file edit with a prompt).
- Agent mode (spec.md → plan.md → tasks.md → execute).

### 8.7 Diff preview

When the agent edits a file, a diff tab opens:

- Side-by-side or inline.
- Per-hunk approve/reject.
- "Accept all" / "Reject all".
- Line stats (+/-).

---

## 9. The status bar

The bottom 28 px bar shows:

- **Left**: active project, active brain, git branch.
- **Center**: model, tokens (this session), cost (this session).
- **Right**: sync status, running agents, permission mode, notifications.

Every item is clickable and opens a relevant panel.

The bar is customizable; plugins can add items.

---

## 10. Tabs and splits

### 10.1 Tab model

- Each tab is a view (chat, editor, graph, etc.).
- Tabs can be grouped in splits.
- Splits can be nested (a tree).
- Layout is persisted per project.

### 10.2 Navigation

- Ctrl+Tab: next tab.
- Ctrl+Shift+Tab: previous tab.
- Ctrl+1..9: jump to tab N.
- Ctrl+W: close tab.
- Ctrl+K Ctrl+W: close all tabs.
- Ctrl+\: split horizontal.
- Ctrl+Shift+\: split vertical.

### 10.3 Drag and drop

Tabs can be:

- Reordered.
- Moved between splits.
- Moved to a new window.
- Tear-off (drag out of the window).

---

## 11. The command palette

### 11.1 Trigger

- Ctrl+K / Ctrl+Shift+P.
- Type `>` for commands, `@` for symbols, `#` for tags, `:` for files.

### 11.2 Contents

- Commands (all registered actions).
- Files (fuzzy).
- Notes (fuzzy + semantic).
- Agents.
- Workflows.
- Settings.
- Plugins.

### 11.3 Actions

Each entry has:

- Icon.
- Label.
- Optional shortcut.
- Category.

### 11.4 Recent and frequent

Recently used commands and files appear first. Frequency is learned.

### 11.5 Extensibility

Plugins can register commands, providers, and modes.

### 11.6 Nested

The palette supports nested modes (e.g. `>theme` → list of themes).

---

## 12. The permission bar

The permission bar sits just above the composer:

```
Permission:  [ Manual ]  [ Auto ● ]  [ Full ]
```

- **Manual**: everything prompts.
- **Auto** (default): safe operations run; risky prompt.
- **Full**: everything runs.

The selected mode is per-session. A tooltip explains the current mode and
its implications.

When a permission is requested, a card appears in the chat:

```
┌─────────────────────────────────────────────────────┐
│ ⚠ Permission requested                              │
│                                                     │
│ Action:  fs.write                                   │
│ Target:  src/main.rs                                │
│ Reason:  Apply the fix described in plan.md         │
│                                                     │
│ [ Deny ]  [ Allow once ]  [ Allow for session ]     │
└─────────────────────────────────────────────────────┘
```

The decision is recorded in the audit log.

---

## 13. The diff viewer

Used for:

- Agent file edits.
- Git diffs.
- Version comparisons.
- Workflow version diffs.

Modes:

- **Inline**: single column, with +/- markers.
- **Side-by-side**: two columns.
- **Unified**: single column with context.

Features:

- Syntax highlighting.
- Word-level diff.
- Per-hunk actions (approve, reject).
- Line stats.
- Copy either side.

---

## 14. The agent status panel

A dedicated panel (accessible from the status bar or the rail) shows:

- All active agents, grouped by session.
- Sub-agent trees.
- Each agent's state (idle, running, waiting, done, error).
- Current tool call.
- Tokens, cost, duration.
- Stop button per agent.

Clicking an agent opens its session.

---

## 15. Notifications

Types:

- **Toast** (bottom-right, auto-dismiss): quick feedback.
- **Inline** (in the chat): permission requests, errors.
- **System** (OS-level): when the app is in the background.
- **Persistent** (in a notifications panel): things the user should review.

Rules:

- Never spam.
- Group related notifications.
- Every notification is actionable or dismissible.
- Silent by default for non-critical events.

---

## 16. Settings

Settings is a full-screen panel, not a modal. Categories:

- **General**: language, theme, startup.
- **Models**: providers, API keys, routing.
- **Permissions**: modes, policies, overrides.
- **MCP**: servers, permissions, health.
- **Sync**: peers, scope, encryption.
- **Appearance**: theme, layout, fonts.
- **Editor**: Monaco settings, LSP servers.
- **Brain**: defaults, templates, indexing.
- **Workflows**: defaults, concurrency.
- **Plugins**: installed, marketplace.
- **Skills**: installed, marketplace.
- **Advanced**: logs, cache, reset.

Settings are searchable. Each setting has a description and a "reset"
button. Some settings are per-project; the UI shows the scope.

---

## 17. Themes

- **Dark** (default).
- **Light**.
- **High contrast** (dark and light).
- **System** (follows OS).

Themes are token-driven (see `/style/tokens.css`). Users can create custom
themes as plugins.

---

## 18. Internationalization

- **UI language**: English, Italian, and more via plugins.
- **Content language**: detected per note; no forced language.
- **Model language**: agents respond in the user's language by default.
- **Number/date formats**: locale-aware.
- **RTL**: supported (future).

Strings are extracted to JSON catalogs. Contributions via Weblate (future).

---

## 19. Keyboard model

### 19.1 Principles

- Every action is reachable via keyboard.
- Shortcuts are discoverable (shown in menus and the palette).
- Shortcuts are customizable.
- Conflicts are detected.

### 19.2 Default shortcuts

| Action | Shortcut |
|---|---|
| Command palette | `Ctrl+K` |
| Quick open file | `Ctrl+P` |
| Search in files | `Ctrl+Shift+F` |
| Search in brain | `Ctrl+Shift+B` |
| Toggle sidebar | `Ctrl+B` |
| Toggle right panel | `Ctrl+Alt+B` |
| New chat | `Ctrl+N` |
| New note | `Ctrl+Shift+N` |
| New window | `Ctrl+Shift+W` |
| Split editor | `Ctrl+\` |
| Close tab | `Ctrl+W` |
| Next tab | `Ctrl+Tab` |
| Go to tab N | `Ctrl+1..9` |
| Run agent | `Ctrl+Enter` |
| Stop agent | `Esc` |
| Approve permission | `Ctrl+Enter` |
| Deny permission | `Esc` |
| Undo | `Ctrl+Z` |
| Redo | `Ctrl+Shift+Z` |
| Settings | `Ctrl+,` |

### 19.3 Modal editing

Vim keybindings are available as an optional plugin.

---

## 20. Accessibility

- **Keyboard**: everything reachable without a mouse.
- **Screen reader**: tested with NVDA and VoiceOver. ARIA labels on all
  interactive elements.
- **Contrast**: WCAG 2.2 AA minimum; AAA where possible. High contrast theme.
- **Focus**: visible focus rings; focus traps in modals; focus restored on
  close.
- **Motion**: reduced motion respected (see `/style/`).
- **Text scaling**: up to 200% without breaking layout.
- **Color independence**: no information conveyed by color alone.
- **Timing**: no time-limited interactions without an option to extend.

---

## 21. Motion

Follows `/style/`. Summary:

- **Fast** (140 ms): hover, focus, small state changes.
- **Base** (220 ms): panel transitions, dropdowns.
- **Slow** (380 ms): modals, layout changes.

Easing: `cubic-bezier(0.22, 1, 0.36, 1)`.

Rules:

- Motion is informative, not decorative.
- No motion for motion's sake.
- Reduced motion is respected.

---

## 22. Plugin UI slots

Every panel and region exposes named slots where plugins can render:

| Slot | Where | Typical use |
|---|---|---|
| `rail.item` | Rail | Add a top-level section |
| `sidebar.section` | Sidebar | Add a section |
| `main.tab` | Main area | Add a tab type |
| `editor.toolbar` | Editor top | Add buttons |
| `editor.gutter` | Editor left | Add gutter decorations |
| `chat.message.actions` | Chat messages | Add actions |
| `chat.composer.extras` | Composer | Add buttons |
| `chat.permission.card` | Permission cards | Add info |
| `statusbar.item` | Status bar | Add items |
| `rightpanel.section` | Right panel | Add sections |
| `dashboard.widget` | Dashboard | Add widgets |
| `command.palette` | Command palette | Add commands |
| `settings.section` | Settings | Add pages |
| `graph.overlay` | Graph | Add overlays |
| `workflow.node` | Workflow editor | Add node types |
| `theme.tokens` | Theming | Override tokens |

Slots are versioned. Plugins declare which API version they target.

---

## 23. The desktop shell

- **Tauri v2**: small binaries, native performance, Rust backend.
- **Window management**: multi-window, tear-off tabs.
- **Native menus**: macOS menu bar, Windows/Linux window menus.
- **Tray**: optional, with quick actions.
- **Global shortcuts**: OS-level (e.g. quick capture).
- **Auto-update**: signed, stable + beta channels.
- **Deep links**: `fergents://` for opening notes, sessions, workflows.
- **File associations**: `.md`, `.canvas`, `.fergbrain`, `.fergplugin`.

---

## 24. The CLI

A TUI built with Ratatui. It is a first-class client, not a stripped-down
version.

### 24.1 Layout

```
┌─ Fergents ──────────────────────────────────────────┬─ Info ──────┐
│                                                     │             │
│  Session: MCP research                              │ Model:      │
│                                                     │ Sonnet      │
│  ▸ You: Research MCP security.                      │             │
│                                                     │ Tokens:     │
│  ▸ Researcher: I'll plan this in 3 steps...         │ 2,341       │
│                                                     │             │
│    ⚙ brain.search (OK, 42ms)                        │ Cost:       │
│    ⚙ web.search (OK, 1.2s)                          │ $0.03       │
│                                                     │             │
│  ▸ Researcher: Done.                                │ Perm: Auto  │
│                                                     │             │
├─────────────────────────────────────────────────────┴─────────────┤
│ > _                                                               │
└───────────────────────────────────────────────────────────────────┘
```

### 24.2 Commands

- `fergents chat [agent]` — interactive chat.
- `fergents ask "..."` — one-shot question.
- `fergents brain ...` — brain operations.
- `fergents search "..."` — search.
- `fergents agent run <name> --task "..."` — run an agent.
- `fergents workflow run <name>` — run a workflow.
- `fergents mcp ...` — MCP operations.
- `fergents plugin ...` — plugin operations.
- `fergents daemon start|stop|status` — kernel control.

### 24.3 Scripting

Every command supports `--json` for machine-readable output. The CLI can be
used in CI, scripts, and pipelines.

---

## 25. The mobile companion

Android app built with Tauri Mobile.

### 25.1 Scope

- Chat with agents (streaming).
- Read/write notes.
- Monitor runs and dashboards.
- Respond to permission requests.
- Notifications.
- Local cache, offline reading.

No heavy compute (no local LLM, no indexing).

### 25.2 Layout

- Bottom navigation: Chat, Brain, Dashboard, Settings.
- Chat is the default view.
- Notes are readable and editable with a lightweight editor.

### 25.3 Connection

- LAN: mDNS discovery, direct to desktop.
- WAN: paired P2P or relay.
- Offline: local SQLite cache; sync on reconnect.

---

## 26. Performance

### 26.1 Targets

| Metric | Target |
|---|---|
| App cold start | < 2 s |
| Tab switch | < 50 ms |
| Chat token render | < 16 ms per token |
| Editor open file | < 100 ms |
| Graph render (1k nodes) | 60 fps |
| Search (hybrid) | < 200 ms |

### 26.2 Techniques

- Virtualized lists.
- Lazy-loaded panels.
- Web workers for heavy work.
- WebGL for graphs.
- Efficient diffing.

---

## 27. Observability

The UI exposes:

- A "Performance" panel (frame rate, memory, IPC latency).
- A "Network" panel (MCP, API calls, sync).
- A "Kernel" panel (event bus, queues, health).

These are for power users and debugging.

---

*End of `UI.md`.*