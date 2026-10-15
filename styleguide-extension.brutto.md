# Styleguide Extensions

> Fergents-specific components that extend the base styleguide in `/style/`.
> These are the pieces the base styleguide does not cover: chat, diff,
> permission bar, agent status, workflow canvas, graph, command palette,
> plugin cards, workspace tabs, split panes.

**Audience:** developers, designers, AI developers.
**Companion docs:** `/style/readme.md`, `/style/styleguide.html`,
`/style/tokens.css`, `/style/components.css`, `/style/patterns.html`.

---

## How to use this document

The base styleguide (`/style/`) defines tokens and generic components.
This document specifies **additional components** using the same tokens,
same conventions, same patterns.

Every component here:

- Uses only tokens from `/style/tokens.css`.
- Follows the spacing scale (`--s-*`).
- Follows the radii scale (`--r-*`).
- Follows the motion scale (`--t-*`).
- Is accessible (keyboard, focus, ARIA).
- Is dark-first, glass, minimal.

When implementing, add these to `/style/patterns.html` and
`/style/components.css` following the existing structure.

---

## Table of contents

1. [Chat](#1-chat)
2. [Diff viewer](#2-diff-viewer)
3. [Permission bar](#3-permission-bar)
4. [Permission card](#4-permission-card)
5. [Agent status](#5-agent-status)
6. [Tool call card](#6-tool-call-card)
7. [Composer](#7-composer)
8. [Attachments](#8-attachments)
9. [Editor panels](#9-editor-panels)
10. [Terminal](#10-terminal)
11. [Workflow canvas](#11-workflow-canvas)
12. [Workflow node](#12-workflow-node)
13. [Graph view](#13-graph-view)
14. [Command palette](#14-command-palette)
15. [Rail](#15-rail)
16. [Workspace tabs](#16-workspace-tabs)
17. [Split panes](#17-split-panes)
18. [Status bar](#18-status-bar)
19. [Plugin card](#19-plugin-card)
20. [Dashboard widget](#20-dashboard-widget)
21. [Widget grid](#21-widget-grid)
22. [Empty states (Fergents-specific)](#22-empty-states-fergents-specific)
23. [Tokens extension](#23-tokens-extension)

---

## 1. Chat

### 1.1 Message bubbles

```
┌──────────────────────────────────────────────────────────────┐
│  ┌─ user ─────────────────────────────────────────────────┐  │
│  │  bg: transparent                                       │  │
│  │  padding: --s-3 --s-4                                  │  │
│  │  border-left: 2px solid var(--accent)                  │  │
│  │  radius: --r-sm                                        │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─ assistant ────────────────────────────────────────────┐  │
│  │  bg: var(--glass)                                      │  │
│  │  border: 1px solid var(--border)                       │  │
│  │  padding: --s-4                                        │  │
│  │  radius: --r-lg                                        │  │
│  │  backdrop-filter: blur(20px) saturate(150%)            │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─ tool ─────────────────────────────────────────────────┐  │
│  │  bg: rgba(255,255,255,0.02)                            │  │
│  │  border-left: 2px solid var(--text-3)                  │  │
│  │  padding: --s-2 --s-3                                  │  │
│  │  radius: --r-md                                        │  │
│  │  font-size: 12.5px                                     │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─ system ───────────────────────────────────────────────┐  │
│  │  centered, italic, color: var(--text-3)                │  │
│  │  font-size: 11.5px                                     │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### 1.2 Streaming cursor

A blinking 2px-wide vertical bar after the last token, using
`var(--accent)`. Blinks at 1s intervals.

### 1.3 Thinking block

A collapsible section, collapsed by default:

```
▸ Thinking (2,341 tokens)
```

When expanded:

```
▾ Thinking (2,341 tokens)
  bg: rgba(255,255,255,0.015)
  border-left: 2px solid var(--border-2)
  padding: --s-3
  font-style: italic
  color: var(--text-3)
```

### 1.4 Message metadata

Below each assistant message:

```
2,341 tokens · $0.03 · 4.2s · [+12 lines] [−0 lines]
```

Style: `--text-3`, 11px, mono for numbers.

### 1.5 Message actions

On hover, right-aligned icons:

- Copy
- Regenerate
- Edit
- Branch (start a new session from here)
- Rate (👍/👎)

---

## 2. Diff viewer

### 2.1 Inline diff

```
┌────────────────────────────────────────────────────────────┐
│  1  fn main() {                                            │
│  2 -    let x = 1;                                         │
│  2 +    let x = 2;                                         │
│  3      println!("{}", x);                                 │
│  4  }                                                      │
└────────────────────────────────────────────────────────────┘
```

Colors:

- Removed lines: `background: var(--red-soft)`, `color: var(--red)`.
- Added lines: `background: var(--green-soft)`, `color: var(--green)`.
- Unchanged: default.

### 2.2 Side-by-side diff

Two columns with a center divider. Left: original. Right: modified.

### 2.3 Diff header

```
┌────────────────────────────────────────────────────────────┐
│ 📄 src/main.rs       [+3] [−2]      [Inline] [Side-by-side]│
│                                     [Approve] [Reject]     │
└────────────────────────────────────────────────────────────┘
```

- File icon + path.
- Line stats (green/red badges).
- Toggle between modes.
- Approve/reject buttons.

### 2.4 Hunk actions

Each hunk has hover actions:

- Accept hunk.
- Reject hunk.
- Copy hunk.

---

## 3. Permission bar

```
Permission:  [ Manual ]  [ Auto ● ]  [ Full ]
```

- Three pill buttons.
- Active state: `background: var(--active)`, `color: var(--text)`.
- Inactive: `color: var(--text-3)`.
- A small dot indicator above the active one in `var(--accent)`.

Tooltip on hover explains each mode.

---

## 4. Permission card

```
┌────────────────────────────────────────────────────────────┐
│ ⚠  Permission requested                                     │
│                                                             │
│  Action:   fs.write                                         │
│  Target:   src/main.rs                                      │
│  Reason:   Apply the fix described in plan.md               │
│  Cost:     ~$0.01                                           │
│                                                             │
│  [ Deny ]  [ Allow once ]  [ Allow for session ]            │
└────────────────────────────────────────────────────────────┘
```

Style:

- Border: `1px solid var(--yellow)`.
- Background: `var(--yellow-soft)`.
- Icon: `var(--yellow)`.
- Buttons: default styling, with the "Deny" in `var(--red-soft)`.

When a permission is denied, the card turns `var(--red-soft)` and is
collapsed.

---

## 5. Agent status

### 5.1 Compact (status bar)

A row of small dots:

```
● ● ● ○ ○    3 agents running
```

Each dot: 8px circle. Running: `var(--accent)`, pulsing. Idle:
`var(--text-3)`.

### 5.2 Panel

```
┌────────────────────────────────────────────────────────────┐
│  Active agents                                  [Stop all] │
├────────────────────────────────────────────────────────────┤
│  🔵 Researcher          running    2,341 tok  $0.03  4.2s  │
│  🟢 Coder               done       1,024 tok  $0.01  1.8s  │
│  🟡 Reviewer            waiting    —          —      —     │
│     ├─ Sub: fact-checker running   ...                     │
│     └─ Sub: citation     done      ...                     │
└────────────────────────────────────────────────────────────┘
```

Icons:

- 🔵 running (accent)
- 🟢 done (green)
- 🟡 waiting (yellow)
- 🔴 error (red)
- ⚪ idle (text-3)

Sub-agents are indented with a tree connector.

Each row has a "Stop" button on hover.

---

## 6. Tool call card

### 6.1 Collapsed

```
⚙ brain.search   query="MCP security"          OK · 42ms  ▸
```

- Icon: category icon (`⚙` for tools, `🧠` for brain, `🌐` for web, `💻`
  for shell, `📁` for fs, `🤖` for LLM, `🔌` for MCP).
- Name: mono, `var(--text-2)`.
- Args preview: truncated, `var(--text-3)`.
- Status badge: green/red/yellow.
- Duration: mono, `var(--text-3)`.
- Expand chevron.

### 6.2 Expanded

```
⚙ brain.search   query="MCP security"          OK · 42ms  ▾
  ┌─ Arguments ──────────────────────────────────────────┐
  │  { "query": "MCP security", "top_k": 8 }             │
  └──────────────────────────────────────────────────────┘
  ┌─ Result (8 items) ───────────────────────────────────┐
  │  1. notes/mcp-security.md#threats       score 0.87   │
  │  2. ...                                              │
  └──────────────────────────────────────────────────────┘
```

Arguments and result are in code blocks. JSON is syntax-highlighted.

---

## 7. Composer

```
┌────────────────────────────────────────────────────────────┐
│  [📎]  Ask anything…                              [↑ send] │
│                                                            │
│  [@ mention] [ / command] [🎤]                             │
└────────────────────────────────────────────────────────────┘
```

- Container: `var(--glass)`, `border: 1px solid var(--border)`,
  `radius: --r-lg`.
- Textarea: auto-resizing, no border.
- Attach button: left.
- Send button: right, `var(--accent)` when enabled, `var(--text-4)` when
  disabled.
- Bottom row: optional actions (`@`, `/`, voice).
- A hint row above: "Enter to send, Shift+Enter for newline".

### 7.1 Mentions

`@` triggers an autocomplete dropdown:

- Files, notes, agents, workflows, MCP servers.

### 7.2 Commands

`/` triggers a command list:

- `/summarize`, `/refactor`, `/write-note`, etc.
- Plugin-registered commands.

---

## 8. Attachments

### 8.1 In composer

Small chips:

```
┌──────────────┐  ┌──────────────┐
│ 📄 plan.md × │  │ 🖼 image ×    │
└──────────────┘  └──────────────┘
```

- `border: 1px solid var(--border-2)`, `radius: --r-sm`, `padding: --s-1 --s-2`.

### 8.2 In message

Images render inline (max 400px). Files show as chips with download links.

---

## 9. Editor panels

### 9.1 File tree item

```
▸ 📁 src/
  📄 main.rs          M
  📄 lib.rs
  📁 plugins/
```

- Indentation: 12px per level.
- Icons: folder/file.
- Git status: `M` (modified, `var(--yellow)`), `A` (added,
  `var(--green)`), `U` (untracked, `var(--text-3)`).
- Hover: `background: var(--hover)`.
- Selected: `background: var(--active)`.

### 9.2 Editor tab

```
┌─────────────────────────┐
│  📄 main.rs     ●  ×    │
└─────────────────────────┘
```

- Active: `background: var(--bg-elev)`, `color: var(--text)`.
- Inactive: `color: var(--text-3)`.
- Modified dot: `var(--accent)`.
- Close: `×` on hover.

### 9.3 Breadcrumb

```
src / main.rs / fn main
```

- Mono, small.
- Each segment clickable.
- Last segment: `var(--text)`.

---

## 10. Terminal

### 10.1 Tab bar

```
[ shell ]  [ dev ]  [ tests ]  [+]
```

### 10.2 Output

- Font: `var(--font-mono)`, 12.5px.
- Colors: ANSI (mapped to styleguide tokens where possible).
- Background: `rgba(0,0,0,0.35)`.
- Padding: `--s-3`.

---

## 11. Workflow canvas

### 11.1 Background

- Grid of dots (12px spacing), `var(--border)`.
- Zoom via scroll.
- Pan via drag (space + drag, or middle-mouse).

### 11.2 Edges

- Bezier curves.
- Default: `var(--border-2)`, 1.5px.
- Active (data flowing): `var(--accent)`, animated dash.
- Hover: `var(--border-3)`.

### 11.3 Ports

- 8px circles on node edges.
- Inputs: left, outputs: right.
- Hover: `var(--accent)`.

### 11.4 Minimap

Bottom-right, 160x100px, semi-transparent, showing the whole graph with a
viewport rectangle.

### 11.5 Inspector

Right-side panel, 320px, showing the selected node's config as a
schema-driven form.

---

## 12. Workflow node

```
┌───────────────────────────┐
│  ⚙  brain.search          │  ← header, 28px
├───────────────────────────┤
│  query: "MCP security"    │  ← body, mono, small
│  top_k: 8                 │
└───────────────────────────┘
```

- Header: `background: var(--glass-strong)`, icon + name.
- Body: `background: var(--glass)`, key-value preview.
- Border: `1px solid var(--border)`.
- Radius: `--r-lg`.
- Shadow: `var(--sh-sm)`.

Status variants:

- Running: `border-color: var(--accent)`, animated.
- Done: `border-color: var(--green)`.
- Failed: `border-color: var(--red)`.
- Skipped: `opacity: 0.5`.

---

## 13. Graph view

### 13.1 Node

- Circle, radius 6-20px (by centrality).
- Fill: by entity type.
- Border: 1.5px, `var(--bg-elev)`.
- Label: below, 11px, `var(--text-2)`, truncated.

### 13.2 Edge

- Line, 1px, `var(--border-2)`.
- Weight: opacity.
- Hover: `var(--accent)`.

### 13.3 Selected

- Ring: `0 0 0 3px var(--accent-soft)`.
- Neighbors highlighted.
- Others dimmed (opacity 0.3).

### 13.4 Filters panel

Left or right, collapsible, with checkboxes for entity types and a
confidence slider.

---

## 14. Command palette

```
┌────────────────────────────────────────────────────────────┐
│  > _                                                        │
├────────────────────────────────────────────────────────────┤
│  > brain: search                  Ctrl+Shift+B              │
│  > brain: new note                Ctrl+Shift+N              │
│  > chat: new session              Ctrl+N                    │
│  > settings: open                 Ctrl+,                    │
├────────────────────────────────────────────────────────────┤
│  Recent                                                     │
│  > brain: search                                            │
│  > chat: new session                                        │
└────────────────────────────────────────────────────────────┘
```

- Overlay: centered, max-width 640px, top 20% of the viewport.
- Container: `var(--glass-strong)`, `blur(28px)`, `shadow-lg`.
- Input: large, no border, `var(--text)`.
- List: max-height 400px, scrollable.
- Selected item: `background: var(--active)`.
- Category headers: `var(--text-4)`, uppercase, 10px.

---

## 15. Rail

```
┌────┐
│ 💬 │  ← active
│ 🧠 │
│ 💻 │
│ ⚙  │
│ 📊 │
│ 🤖 │
│ 🧩 │
│ ⚙  │
└────┘
```

- Width: 48px.
- Icons: 20px, centered.
- Active: `background: var(--active)`, accent bar on the left (2px,
  `var(--accent)`).
- Hover: `background: var(--hover)`.
- Tooltip on hover.

---

## 16. Workspace tabs

Top of the main area:

```
┌─────────────┬─────────────┬─────────────┐
│ Chat     ●  │ main.rs     │ notes.md    │
└─────────────┴─────────────┴─────────────┘
```

- Active: `background: var(--bg-elev)`, `color: var(--text)`.
- Inactive: `color: var(--text-3)`.
- Modified dot: `var(--accent)`.
- Close on hover.
- Overflow: `…` menu.

---

## 17. Split panes

- Gutter: 4px, `background: transparent`.
- Hover: `background: var(--border-2)`.
- Active (dragging): `background: var(--accent)`.
- Cursor: `col-resize` / `row-resize`.

Minimum pane size: 200px.

---

## 18. Status bar

```
┌────────────────────────────────────────────────────────────────────┐
│ proj: fergents  brain: personal  ⎇ main    Sonnet  2,341 tok  $0.03│
│                                            ●●● 3 agents  sync: LAN │
└────────────────────────────────────────────────────────────────────┘
```

- Height: 28px.
- Background: `var(--bg-elev)`, border-top `var(--border)`.
- Font: 11px, mono for numeric values.
- Each item: hoverable, clickable.

---

## 19. Plugin card

```
┌────────────────────────────────────────────────────────────┐
│  🧩  RSS Reader                                 v1.0.0     │
│      Read RSS feeds and save items to the brain.           │
│                                                             │
│  Permissions:  fs.read · net.http · brain.write            │
│                                                             │
│  [ Enable ]  [ Settings ]  [ Uninstall ]                   │
└────────────────────────────────────────────────────────────┘
```

- Card: `var(--glass)`, `border: 1px solid var(--border)`,
  `radius: --r-lg`.
- Icon: left, 32px.
- Title + version: top.
- Description: below.
- Permissions: chips.
- Actions: right-aligned buttons.

---

## 20. Dashboard widget

```
┌────────────────────────────────────────────────────────────┐
│  Cost by model (30d)                              [⋯] [×] │
├────────────────────────────────────────────────────────────┤
│                                                             │
│  [chart]                                                    │
│                                                             │
└────────────────────────────────────────────────────────────┘
```

- Header: 32px, `background: rgba(255,255,255,0.02)`.
- Title: 12.5px, `var(--text-2)`.
- Actions: `⋯` (menu), `×` (remove) on hover.
- Body: content.

---

## 21. Widget grid

- Columns: 12 (configurable).
- Row height: 80px.
- Gap: `--s-3`.
- Drag handle: whole widget header.
- Resize handle: bottom-right corner.

---

## 22. Empty states (Fergents-specific)

### 22.1 No sessions

```
        💬
   No conversations yet
   Start by asking anything.
   [ New session ]
```

### 22.2 No notes

```
        🧠
   Your brain is empty
   Create a note or import a vault.
   [ New note ]  [ Import ]
```

### 22.3 No agents

```
        🤖
   No agents running
   Your agents will appear here when they run.
```

### 22.4 No plugins

```
        🧩
   No plugins installed
   Browse the marketplace or install from a file.
   [ Browse ]  [ Install from file ]
```

Style: same as base `.empty` from `/style/`, with Fergents-specific icons.

---

## 23. Tokens extension

Additional tokens used by Fergents-specific components. Add these to
`/style/tokens.css`:

```css
:root {
    /* Chat */
    --chat-user-accent:   var(--accent);
    --chat-assistant-bg:  var(--glass);
    --chat-tool-bg:       rgba(255, 255, 255, 0.02);

    /* Diff */
    --diff-add-bg:        var(--green-soft);
    --diff-add-fg:        var(--green);
    --diff-remove-bg:     var(--red-soft);
    --diff-remove-fg:     var(--red);

    /* Agent status */
    --agent-running:      var(--accent);
    --agent-done:         var(--green);
    --agent-waiting:      var(--yellow);
    --agent-error:        var(--red);
    --agent-idle:         var(--text-3);

    /* Workflow */
    --node-header-bg:     var(--glass-strong);
    --node-body-bg:       var(--glass);
    --node-border:        var(--border);
    --edge-default:       var(--border-2);
    --edge-active:        var(--accent);

    /* Graph */
    --graph-node-1:       var(--accent);
    --graph-node-2:       var(--blue);
    --graph-node-3:       var(--green);
    --graph-node-4:       var(--yellow);
    --graph-edge:         var(--border-2);

    /* Rail */
    --rail-width:         48px;
    --rail-active-bar:    2px;

    /* Status bar */
    --statusbar-height:   28px;
}
```

These extend, not replace, the base tokens.

---

*End of `STYLEGUIDE_EXTENSIONS.md`.*