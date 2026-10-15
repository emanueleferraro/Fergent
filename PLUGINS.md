# Plugins

> The plugin system: WASM kernel plugins, React UI plugins, the SDK, the
> marketplace, permissions, and packaging.

**Audience:** plugin authors, developers, AI developers.
**Companion docs:** `ARCHITECTURE.md`, `SKILLS.md`, `UI.md`, `SECURITY.md`.

---

## Table of contents

1. [What a plugin is](#1-what-a-plugin-is)
2. [Plugin kinds](#2-plugin-kinds)
3. [The plugin manifest](#3-the-plugin-manifest)
4. [The kernel plugin (WASM)](#4-the-kernel-plugin-wasm)
5. [The UI plugin (React)](#5-the-ui-plugin-react)
6. [Host functions](#6-host-functions)
7. [Permissions](#7-permissions)
8. [UI slots](#8-ui-slots)
9. [The SDK](#9-the-sdk)
10. [Development mode](#10-development-mode)
11. [Packaging](#11-packaging)
12. [Installation](#12-installation)
13. [Versioning](#13-versioning)
14. [The marketplace](#14-the-marketplace)
15. [Security](#15-security)
16. [AI-created plugins](#16-ai-created-plugins)
17. [Observability](#17-observability)
18. [Examples](#18-examples)
19. [Appendix: manifest schema](#19-appendix-manifest-schema)

---

## 1. What a plugin is

A plugin is a **sandboxed extension** of Fergents. It can:

- Add **tools** the AI can call.
- Add **UI panels**, widgets, and commands.
- Subscribe to **events** and react.
- Add **workflow node types**.
- Integrate **external services**.
- Provide **themes**.
- Provide **importers/exporters**.

A plugin cannot:

- Access the host without permission.
- Bypass the audit log.
- Escape the sandbox.

---

## 2. Plugin kinds

| Kind | Runtime | Capabilities |
|---|---|---|
| **Kernel** | WASM (wasmtime, Component Model) | Tools, events, storage, LLM, MCP, workflow nodes |
| **UI** | React in Shadow DOM | Panels, widgets, commands, themes |
| **Hybrid** | Both | Both |

A single plugin can ship both a kernel and a UI part. They communicate
through the kernel: the UI plugin calls commands, the kernel plugin handles
them.

---

## 3. The plugin manifest

Every plugin has a `plugin.toml`:

```toml
[plugin]
id = "com.example.rss-reader"
name = "RSS Reader"
version = "1.0.0"
api_version = "1"
description = "Read RSS feeds and save items to the brain."
author = "Example Inc."
license = "MIT"
homepage = "https://example.com/rss-reader"
repository = "https://github.com/example/rss-reader"

[kernel]
module = "plugin.wasm"
entry = "init"

[ui]
entry = "ui.js"
stylesheet = "ui.css"

[permissions]
fs.read = ["$BRAIN/**/*.md", "$CACHE/rss/**"]
fs.write = ["$BRAIN/inbox/**"]
net.http = ["https://*.example.com/feed/*"]
llm.call = ["openai:gpt-4o-mini", "ollama:*"]
mcp.connect = ["stdio:rss-mcp-server"]
events.subscribe = ["file.changed", "workflow.triggered"]
events.emit = ["rss.item.fetched"]
storage.local = true

[slots]
- "rail.item"
- "sidebar.section"
- "chat.composer.extras"

[workflow_nodes]
- id = "rss.fetch"
  name = "Fetch RSS"
  ...

[commands]
- id = "rss.refresh"
  label = "Refresh RSS feeds"
  shortcut = "Ctrl+Alt+R"
```

Everything is declared. Nothing is implicit.

---

## 4. The kernel plugin (WASM)

### 4.1 Language

Any language that compiles to a WASM Component:

- Rust (first-class, recommended).
- TypeScript (via `jco`).
- Go (via TinyGo).
- Python (via `componentize-py`).
- C/C++ (via clang).
- Zig.

### 4.2 The Rust SDK

```rust
use fergents_plugin_sdk::*;

#[fergents_plugin]
struct RssReader;

impl Plugin for RssReader {
    fn init(ctx: &mut Context) -> Result<()> {
        ctx.register_tool("rss.fetch", Tool {
            description: "Fetch items from an RSS feed.",
            parameters: schema! {
                url: string,
                limit: integer = 10,
            },
            handler: fetch,
        });
        Ok(())
    }
}

fn fetch(ctx: &mut Context, args: FetchArgs) -> Result<FetchOutput> {
    let body = ctx.http_get(&args.url)?;
    let items = parse_rss(&body)?;
    ctx.emit("rss.item.fetched", &items)?;
    Ok(FetchOutput { items })
}
```

### 4.3 Lifecycle

- `init`: called once on load.
- `on_event`: called when a subscribed event fires.
- `on_command`: called when a UI command is invoked.
- `on_shutdown`: called on unload.

### 4.4 Isolation

- WASM sandbox, no ambient authority.
- Host functions gated by permissions.
- Memory and CPU limits.
- No filesystem, network, or env vars unless granted.

---

## 5. The UI plugin (React)

### 5.1 Structure

```tsx
import { registerPlugin, Panel, useKernel } from '@fergents/plugin-sdk';

registerPlugin({
  id: 'com.example.rss-reader',
  panels: [
    {
      id: 'rss-panel',
      slot: 'sidebar.section',
      title: 'RSS Feeds',
      render: () => <RssPanel />,
    },
  ],
  commands: [
    {
      id: 'rss.refresh',
      label: 'Refresh RSS feeds',
      run: async () => {
        const kernel = useKernel();
        await kernel.call('rss.fetch', { url: 'https://example.com/feed.xml' });
      },
    },
  ],
});

function RssPanel() {
  const kernel = useKernel();
  const { data } = kernel.query('rss.list');
  return <ul>{data?.map(item => <li key={item.id}>{item.title}</li>)}</ul>;
}
```

### 5.2 Isolation

- Rendered in a Shadow DOM root.
- No access to the host DOM or JS globals.
- Communication only via a message-passing bridge (the SDK).
- Styles are scoped to the shadow root.

### 5.3 Theming

Plugins can use the host's design tokens (CSS custom properties). They
should not hardcode colors or spacing.

---

## 6. Host functions

Kernel plugins access the host through functions. Each is gated by a
permission.

### 6.1 Filesystem

- `fs.read(path)` → bytes
- `fs.write(path, bytes)`
- `fs.list(path)` → entries
- `fs.delete(path)`
- `fs.stat(path)` → metadata

### 6.2 Network

- `http.request(method, url, headers, body)` → response
- `http.stream(method, url, ...)` → stream

### 6.3 LLM

- `llm.chat(messages, model, params)` → response
- `llm.embed(texts)` → vectors
- `llm.rerank(query, docs)` → scores

### 6.4 MCP

- `mcp.call(server, tool, args)` → result
- `mcp.list_tools(server)` → tools

### 6.5 Storage

- `storage.get(key)` → bytes
- `storage.set(key, bytes)`
- `storage.delete(key)`
- `storage.list(prefix)` → keys

### 6.6 Events

- `event.subscribe(topic)` (declared in manifest)
- `event.emit(topic, payload)`
- `event.unsubscribe(topic)`

### 6.7 Logging

- `log.debug(msg, fields)`
- `log.info(msg, fields)`
- `log.warn(msg, fields)`
- `log.error(msg, fields)`

### 6.8 Secrets

- `secrets.get(name)` → value (only for names declared in the manifest)

### 6.9 Brain

- `brain.search(query, opts)` → results
- `brain.read(path)` → content
- `brain.write(path, content)` (with permission)

### 6.10 UI (for hybrid plugins)

- `ui.notify(title, body)`
- `ui.openPanel(id)`

---

## 7. Permissions

Every host function is gated. The manifest declares what the plugin needs;
the user approves at install.

### 7.1 Categories

- `fs.read`, `fs.write` (with path scopes)
- `net.http` (with domain scopes)
- `llm.call` (with model scopes)
- `mcp.connect` (with server scopes)
- `storage.local`
- `events.subscribe`, `events.emit`
- `secrets.get`
- `brain.read`, `brain.write`
- `ui.notify`

### 7.2 Scope syntax

- Paths: glob patterns (`$BRAIN/**/*.md`).
- Domains: `https://*.example.com/*`.
- Models: `openai:gpt-4o-mini`, `ollama:*`.

### 7.3 Review

At install, the user sees the full list of permissions and can:

- Approve all.
- Approve selectively (if the plugin supports it).
- Deny (install aborts).

### 7.4 Revocation

Permissions can be revoked at any time in Settings → Plugins. Revocation is
immediate; the plugin's calls fail.

---

## 8. UI slots

(see `UI.md §22`)

Each slot has a documented interface. Plugins declare which slots they use.

---

## 9. The SDK

### 9.1 Rust SDK

- Crate: `fergents-plugin-sdk`.
- Macros for tool/event/command registration.
- Typed host function bindings.
- Schema helpers.
- Templates and examples.

### 9.2 TypeScript SDK

- NPM: `@fergents/plugin-sdk`.
- React components for common patterns.
- Hooks for kernel queries and events.
- Typed commands.
- Templates and examples.

### 9.3 Documentation

A documentation site with:

- Getting started.
- API reference.
- Guides per use case (tool, panel, workflow node, theme).
- Examples (10+ complete plugins).

---

## 10. Development mode

### 10.1 Hot reload

When a plugin is loaded from a local directory in dev mode:

- Changes to the WASM module trigger a reload.
- Changes to the UI module trigger a hot update.
- The state is preserved where possible.

### 10.2 Dev tools

- A "Plugin Dev" panel showing logs, events, and calls.
- Time-travel debugging for events.
- A permission simulator (test with different permission sets).

### 10.3 Testing

- Unit tests (Rust: `cargo test`; TS: Vitest).
- Integration tests against a real kernel (a test harness spins up a
  temporary kernel).
- A mock SDK for isolated UI tests.

---

## 11. Packaging

### 11.1 Format

A `.fergplugin` file is a zip:

```
com.example.rss-reader/
├── plugin.toml
├── plugin.wasm
├── ui.js
├── ui.css
├── assets/
└── README.md
```

### 11.2 Signing

Official plugins are signed with Ed25519. The signature covers the manifest
and all files. Unsigned plugins are allowed but require explicit consent
with a warning.

### 11.3 Versioning

Plugins follow semver. The kernel enforces API version compatibility.

---

## 12. Installation

### 12.1 Sources

- Local `.fergplugin` file.
- Local directory (dev).
- Git URL.
- Marketplace (post-v1).

### 12.2 Flow

1. User (or AI) provides a source.
2. Kernel fetches and validates the manifest.
3. Permissions are shown.
4. User approves.
5. Plugin is installed to `plugins/installed/<id>/`.
6. Kernel loads the WASM module and initializes it.
7. UI module is registered.
8. Plugin appears in Settings → Plugins.

### 12.3 Updates

- Manual (check for updates).
- Automatic (opt-in, per plugin).

### 12.4 Uninstallation

- Plugin is disabled.
- Running calls are cancelled.
- Files removed.
- Permissions revoked.
- Plugin-local storage deleted (with confirmation).

---

## 13. Versioning

- **Plugin version**: semver.
- **API version**: an integer. Plugins declare which they target.
- **Compatibility**: the kernel supports N-1 API versions. Older plugins
  work until dropped.

---

## 14. The marketplace

Post-v1. A curated index of plugins, hosted on a Fergents-controlled
repository.

Features:

- Search, categories, ratings.
- Installation with one click.
- Automatic updates.
- Signature verification.
- Verified badges.
- Moderation pipeline.
- Payments (optional, TBD).

The AI can search and install from the marketplace (subject to permission).

---

## 15. Security

(see `SECURITY.md §10`)

Summary:

- WASM sandbox.
- Permission gates.
- Signature verification.
- Audit log.
- Content sanitization.
- Resource limits.

---

## 16. AI-created plugins

The AI can create a plugin:

```
User: "Create a plugin that shows my GitHub notifications in a panel."
Agent: [writes plugin.toml, plugin.wasm (Rust), ui.tsx]
Agent: [packages as .fergplugin]
Agent: [installs, with the user's approval]
Agent: "Done. The panel is now available in the sidebar."
```

The AI can:

- Generate the manifest.
- Write the Rust and TS code.
- Build the WASM.
- Test the plugin.
- Install it.
- Debug it.
- Update it.

This is subject to the same permissions as any other action.

---

## 17. Observability

Per plugin:

- Load time, memory usage.
- Tool calls, error rate, latency.
- Event emissions.
- Permission denials.
- Storage usage.

All in the dashboard.

---

## 18. Examples

### 18.1 A minimal tool plugin (Rust)

```rust
use fergents_plugin_sdk::*;

#[fergents_plugin]
struct Hello;

impl Plugin for Hello {
    fn init(ctx: &mut Context) -> Result<()> {
        ctx.register_tool("hello.greet", Tool {
            description: "Greet someone by name.",
            parameters: schema! { name: string },
            handler: |_ctx, args| Ok(format!("Hello, {}!", args.name)),
        });
        Ok(())
    }
}
```

```toml
[plugin]
id = "com.example.hello"
name = "Hello"
version = "1.0.0"
api_version = "1"

[kernel]
module = "plugin.wasm"
```

### 18.2 A UI panel plugin (TypeScript)

```tsx
import { registerPlugin, Panel, useKernel } from '@fergents/plugin-sdk';

registerPlugin({
  id: 'com.example.clock',
  panels: [{
    id: 'clock',
    slot: 'statusbar.item',
    render: () => <Clock />,
  }],
});

function Clock() {
  const [now, setNow] = React.useState(new Date());
  React.useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  return <span>{now.toLocaleTimeString()}</span>;
}
```

### 18.3 A workflow node plugin (Rust)

```rust
#[fergents_plugin]
struct RssNode;

impl Plugin for RssNode {
    fn init(ctx: &mut Context) -> Result<()> {
        ctx.register_workflow_node(NodeDef {
            id: "rss.fetch",
            name: "Fetch RSS",
            inputs: schema! { url: string, limit: integer = 10 },
            outputs: schema! { items: array },
            handler: |ctx, input| {
                let body = ctx.http_get(&input.url)?;
                let items = parse_rss(&body)?;
                Ok(json!({ "items": items }))
            },
        });
        Ok(())
    }
}
```

### 18.4 A hybrid plugin

Combines the above. The UI calls `rss.fetch` via the SDK; the kernel
handles it.

---

## 19. Appendix: manifest schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Fergents Plugin Manifest",
  "type": "object",
  "required": ["plugin"],
  "properties": {
    "plugin": {
      "type": "object",
      "required": ["id", "name", "version", "api_version"],
      "properties": {
        "id": { "type": "string", "pattern": "^[a-z][a-z0-9.]*[a-z0-9]$" },
        "name": { "type": "string" },
        "version": { "type": "string" },
        "api_version": { "type": "string" },
        "description": { "type": "string" },
        "author": { "type": "string" },
        "license": { "type": "string" },
        "homepage": { "type": "string" },
        "repository": { "type": "string" }
      }
    },
    "kernel": {
      "type": "object",
      "properties": {
        "module": { "type": "string" },
        "entry": { "type": "string" }
      }
    },
    "ui": {
      "type": "object",
      "properties": {
        "entry": { "type": "string" },
        "stylesheet": { "type": "string" }
      }
    },
    "permissions": { "type": "object" },
    "slots": { "type": "array", "items": { "type": "string" } },
    "commands": { "type": "array" },
    "workflow_nodes": { "type": "array" }
  }
}
```

---

*End of `PLUGINS.md`.*