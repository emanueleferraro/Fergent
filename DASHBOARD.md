# Dashboard

> Prebuilt and custom dashboards: widgets, data sources, YAML definitions,
> MCP integration, and the drag-and-drop grid.

**Audience:** developers, power users, AI developers.
**Companion docs:** `UI.md`, `PLUGINS.md`, `MCP.md`, `SEARCH.md`.

---

## Table of contents

1. [Concept](#1-concept)
2. [Prebuilt dashboards](#2-prebuilt-dashboards)
3. [Custom dashboards](#3-custom-dashboards)
4. [The widget model](#4-the-widget-model)
5. [Widget types](#5-widget-types)
6. [Data sources](#6-data-sources)
7. [YAML definition](#7-yaml-definition)
8. [MCP integration](#8-mcp-integration)
9. [The grid](#9-the-grid)
10. [Interactivity](#10-interactivity)
11. [Sharing and export](#11-sharing-and-export)
12. [AI operations](#12-ai-operations)
13. [Performance](#13-performance)
14. [Observability](#14-observability)
15. [Examples](#15-examples)

---

## 1. Concept

A dashboard is a **grid of widgets**. Each widget shows data from a source,
with a specified visualization. Dashboards can be:

- **Prebuilt** (shipped with Fergents).
- **Custom** (created by the user or the AI).
- **Shared** (exported and imported).

The dashboard is not just for costs and tokens. It is a general
observability and control surface: any data Fergents knows (or can fetch
via MCP, plugins, SQL, events) can be visualized.

---

## 2. Prebuilt dashboards

| Dashboard | Purpose |
|---|---|
| **Overview** | At-a-glance: active agents, recent runs, cost today, sync status. |
| **Costs** | Per provider, model, agent, task, day/week/month. |
| **Tokens** | Input, output, cached; per model, per agent. |
| **Runs** | Agent sessions, workflow runs, with filters. |
| **Errors** | Grouped errors from LLM, tools, MCP, plugins. |
| **Agents** | Activity, status, limits, spend. |
| **Brains** | Index size, growth, search quality, entity counts. |
| **Sync** | Peers, last sync, conflicts, bytes. |
| **Security** | Permission denials, audit log highlights, anomalies. |
| **Performance** | IPC latency, kernel queue depth, frame rate. |

Each prebuilt dashboard is itself defined in YAML; users can duplicate and
modify them.

---

## 3. Custom dashboards

The user creates a dashboard:

1. Click "New dashboard".
2. Name it.
3. Add widgets (drag from the library or write YAML).
4. Configure each widget (data source, visualization, filters).
5. Save.

The AI can create one from a description:

```
User: "Create a dashboard that shows my writing output per week."
Agent: [creates a YAML dashboard with a bar chart of word counts]
Agent: [registers it]
Agent: "Done. Open the 'Writing' dashboard."
```

---

## 4. The widget model

A widget has:

- **id**: unique within the dashboard.
- **type**: the visualization.
- **title**: optional.
- **source**: where the data comes from.
- **query**: the data query (source-specific).
- **options**: visualization-specific.
- **layout**: position and size in the grid.
- **refresh**: manual or interval.

---

## 5. Widget types

| Type | Description |
|---|---|
| **metric** | A single value (e.g. "Cost today: $2.34"). |
| **sparkline** | A small line chart. |
| **line** | A line chart over time. |
| **bar** | A bar chart. |
| **pie** | A pie chart. |
| **donut** | A donut chart. |
| **table** | A data table. |
| **list** | A scrollable list of items. |
| **timeline** | Events over time. |
| **gauge** | A radial gauge. |
| **heatmap** | A heatmap. |
| **markdown** | Rendered markdown (static). |
| **log** | A live log stream. |
| **graph** | A mini graph visualization. |
| **custom** | A React component (from a plugin). |

---

## 6. Data sources

| Source | Description |
|---|---|
| **sql** | Query the global or brain SQLite DB. |
| **event** | Subscribe to a kernel event topic. |
| **mcp** | Call an MCP tool or resource. |
| **plugin** | Call a plugin's tool or query. |
| **chat** | Query chat sessions (costs, tokens, messages). |
| **search** | Run a search and show results. |
| **brain** | Query brain stats (documents, chunks, entities). |
| **agent** | Query agent sessions, runs, costs. |
| **workflow** | Query workflow runs. |
| **static** | A static value (for notes, links). |

Each source has a query language:

- **sql**: SQL (read-only, scoped to the brain or global DB).
- **event**: a topic + optional filter (JSONPath).
- **mcp**: `server.tool(args)`.
- **plugin**: `plugin.tool(args)`.

---

## 7. YAML definition

A dashboard is a YAML file:

```yaml
id: writing
name: Writing
description: Track writing output over time.
layout:
  columns: 12
  row_height: 80

widgets:
  - id: total-words
    type: metric
    title: Total words written (30d)
    layout: { x: 0, y: 0, w: 3, h: 1 }
    source:
      kind: sql
      db: brain
      query: |
        SELECT SUM(word_count) AS value
        FROM notes_meta nm
        JOIN documents d ON d.id = nm.document_id
        WHERE d.updated_at > (strftime('%s','now') - 30*86400) * 1000
    options:
      format: number
      unit: words

  - id: words-per-day
    type: bar
    title: Words per day (30d)
    layout: { x: 3, y: 0, w: 9, h: 2 }
    source:
      kind: sql
      db: brain
      query: |
        SELECT date(d.updated_at/1000, 'unixepoch') AS day,
               SUM(nm.word_count) AS words
        FROM notes_meta nm
        JOIN documents d ON d.id = nm.document_id
        WHERE d.updated_at > (strftime('%s','now') - 30*86400) * 1000
        GROUP BY day
        ORDER BY day
    options:
      x: day
      y: words
      color: accent

  - id: recent-notes
    type: list
    title: Recently updated notes
    layout: { x: 0, y: 2, w: 6, h: 3 }
    source:
      kind: sql
      db: brain
      query: |
        SELECT path, title, updated_at
        FROM documents d
        LEFT JOIN notes_meta nm ON nm.document_id = d.id
        ORDER BY d.updated_at DESC
        LIMIT 20
    options:
      title_field: title
      subtitle_field: path
      time_field: updated_at

  - id: cost-today
    type: metric
    title: LLM cost today
    layout: { x: 6, y: 2, w: 3, h: 1 }
    source:
      kind: sql
      db: global
      query: |
        SELECT SUM(cost_usd) AS value
        FROM llm_requests
        WHERE created_at > (strftime('%s','now','start of day')) * 1000
    options:
      format: currency
      currency: USD

  - id: mcp-health
    type: table
    title: MCP servers
    layout: { x: 9, y: 2, w: 3, h: 3 }
    source:
      kind: mcp
      server: internal
      tool: list_servers
    options:
      columns: [name, status, latency_ms]
```

---

## 8. MCP integration

Widgets can pull data from any MCP server:

```yaml
- id: arxiv-trends
  type: line
  title: arXiv papers on "MCP"
  source:
    kind: mcp
    server: arxiv
    tool: search_papers
    args: { query: "MCP", from: "2024-01-01" }
  options:
    x: date
    y: count
```

This is how the dashboard becomes **unbounded**: anything reachable via MCP
can be visualized.

Widgets can also **push** data: a widget can have an action that calls an
MCP tool (e.g. "Deploy", "Restart").

---

## 9. The grid

- **Columns**: 12 by default (configurable).
- **Row height**: 80 px by default.
- **Drag**: move widgets.
- **Resize**: drag handles.
- **Snap**: to grid.
- **Collision**: widgets push each other.
- **Responsive**: columns collapse on narrow screens.

Layout is persisted per dashboard.

---

## 10. Interactivity

- **Click** on a data point → drill down (e.g. open the run).
- **Hover** → tooltip with details.
- **Filter** → a filter widget that affects all widgets in the dashboard.
- **Time range** → a global time range selector.
- **Refresh** → manual or interval (per widget).
- **Export** → PNG, CSV, JSON.

Widgets can be **linked**: clicking one filters another.

---

## 11. Sharing and export

- **Export** a dashboard as a `.fergdash` file (YAML + assets).
- **Import** a `.fergdash` file.
- **Share** via git, email, or the marketplace (post-v1).
- **Embed** a dashboard in a note (rendered inline).

---

## 12. AI operations

The AI can:

- **Create** a dashboard from a description.
- **Modify** an existing dashboard.
- **Add** widgets.
- **Explain** what a widget shows.
- **Query** a dashboard's data.
- **Suggest** new widgets.
- **Diagnose** why a widget is empty or slow.

Example:

```
User: "Add a widget showing my token usage per model this week."
Agent: [creates the widget, adds it to the Overview dashboard]
```

---

## 13. Performance

- **Lazy loading**: widgets load as they scroll into view.
- **Caching**: query results cached with TTL.
- **Incremental**: event-based widgets update incrementally.
- **Debounce**: rapid refreshes are debounced.
- **Virtualization**: tables and lists are virtualized.

---

## 14. Observability

- Widget load time.
- Query latency.
- Cache hit rate.
- Errors.

Visible in a "Dashboard" meta-dashboard (yes, the dashboard has a dashboard).

---

## 15. Examples

### 15.1 Cost per model

```yaml
- id: cost-per-model
  type: pie
  title: Cost by model (30d)
  source:
    kind: sql
    db: global
    query: |
      SELECT model, SUM(cost_usd) AS cost
      FROM llm_requests
      WHERE created_at > (strftime('%s','now') - 30*86400) * 1000
      GROUP BY model
      ORDER BY cost DESC
  options:
    label: model
    value: cost
```

### 15.2 Active agents

```yaml
- id: active-agents
  type: list
  title: Active agents
  source:
    kind: event
    topic: agent.started
    filter: "$.status == 'running'"
  options:
    title_field: agent_id
    subtitle_field: task
```

### 15.3 Sync status

```yaml
- id: sync-peers
  type: table
  title: Sync peers
  source:
    kind: sql
    db: global
    query: |
      SELECT name, last_seen_at, last_addr, trusted
      FROM peers
      ORDER BY last_seen_at DESC
```

### 15.4 Custom React widget

```yaml
- id: custom-heatmap
  type: custom
  title: Activity heatmap
  source:
    kind: plugin
    plugin: com.example.heatmap
    query: activity
  options:
    weeks: 52
```

The plugin registers a React component for the `custom` type.

---

*End of `DASHBOARD.md`.*