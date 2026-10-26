# Plugin Runtime & Skills — Phase 4 Spec

## Why

Phase 1 delivered the **brain** (markdown vault, index, hybrid search, graph).
Phase 2 (`v0.2.0-phase2`) delivered the **agent runtime**: LLM router, MCP host,
ReAct loop, memory, sub-agents, permissions, reversibility, chat.
Phase 3 (`v0.3.0-phase3`) delivered the **IDE**: projects, Monaco, LSP,
terminal, git, inline AI, Composer.

Everything Fergents can do today is compiled into the kernel. The next step is
the one that turns a product into a platform: **users and the AI can extend
Fergents without touching the kernel**.

Phase 4 (`ROADMAP.md` §8) delivers two extension systems with one security
model:

- **Plugins** — sandboxed WASM components that add tools, event subscribers,
  storage, workflow node types; and React UI modules that render into named
  slots of the shell (`PLUGINS.md`, `ARCHITECTURE.md` §6).
- **Skills** — declarative, composable capabilities (prompt, tool, code,
  hybrid) described by a `SKILL.md` folder (`SKILLS.md`).

Both are installed, permissioned, audited, and reversible exactly like every
other kernel action. Both can be authored by the AI: the **Plugin Factory**
writes, builds, tests in a sandbox, and installs — always behind the same
permission engine (D18).

This spec delivers exactly `ROADMAP.md` §8, `PLUGINS.md`, `SKILLS.md`, and the
16 slots of `UI.md` §22. It does **not** deliver the marketplace backend, a
full marketplace UI, payments, a VS Code-compatible extension host, workflow
node *execution* (Phase 5), or a JS sandbox stronger than Shadow DOM
(Appendix G).

## What Changes

Phase 4 is **additive**. No Phase 1–3 command, event, or schema changes
meaning. The following are new: two kernel crates (`plugins`, `skills`), one
shared sandbox crate (`wasm`), one packaging crate (`packages`), the SDK
crates/packages, three migrations, a protocol surface, and a desktop plugin
host plus two settings areas.

### 0. Capability registrations (cross-cutting)

Every Phase 4 feature is a registered **Capability** (Phase 2 D1–D3). Disabled
means the kernel refuses the commands (`capability_disabled`), allocates no
resources, publishes no events, and the UI renders no slots.

| Capability | Default | Hot | Depends on | What it allocates when enabled |
|---|---|---|---|---|
| `capability.plugins` | **on** | yes | `capability.agents` | wasmtime engine, plugin registry, host functions, UI slot registry, plugin tool registration |
| `capability.skills` | **on** | yes | `capability.agents` | skill registry, skill execution, skill cache, per-skill agent tools |
| `capability.marketplace` | **off** | yes | `capability.plugins`, `capability.skills` | index client, outbound fetches, marketplace cache |
| `capability.plugin-factory` | **off** (existing) | **yes** (was restart-required) | `capability.plugins`, `capability.skills`, `capability.agents` | factory workspace, toolchain builds, sandbox test harness |

`capability.plugin-factory` already exists in
`crates/capabilities/src/catalogue.rs` with `default_enabled = false` and
`hot_toggle = false`; Phase 4 upgrades it (MODIFIED Requirements).

Defaults follow Phase 2 D15: local, zero-marginal-cost features default on;
networked, code-executing, or token-costly features default off. Plugins and
skills are local-first and permission-gated per call, so they default on and
can be switched off entirely.

**Plugin-provided capabilities.** A plugin may declare optional capabilities
in its manifest (`[capabilities]`). They are registered into the existing
`CapabilityRegistry` with `source = "plugin:<plugin-id>"`, `default_enabled =
false`, and are removed on uninstall (`BACKLOG.md` §0; `Capability.source`
already models this). A plugin-provided capability may be disabled like any
other and must not be a dependency of a kernel capability.

### 1. Kernel plugin host (WASM)

**New crate `crates/wasm`** — the shared Component Model runtime:

- One `wasmtime::Engine` per kernel process, lazily created on first load
  (`capability.plugins` or `capability.skills`). Cranelift, Component Model,
  async, epoch interruption, and fuel metering enabled.
- **WASI preview 2** with only `clocks` and `random`; **no** WASI filesystem,
  sockets, or environment. All ambient I/O is through host functions (§2).
- Limits per call (Appendix E): memory, fuel, wall-clock epoch deadline,
  input/output size, instance count.
- Compiled-artifact cache at `$FERGENTS_HOME/cache/wasm/<sha256>.cwasm`, keyed
  by module hash + wasmtime version + engine config; validated before reuse.
- Typed worlds (`fergents:plugin@1`, `fergents:skill@1`). Cross-boundary
  payloads are **JSON strings** because plugin/skill schemas are dynamic.
- A test-only host (`TestHost`) with in-memory storage, no network, and a stub
  LLM, used by the factory and by CI.

**New crate `crates/plugins`** — manifest, registry, lifecycle, host
functions, install/update/uninstall, dev mode, and factory integration.

**Component ABI (`fergents:plugin@1`).**

Exports:

| Export | Purpose |
|---|---|
| `init()` | Called once after instantiation; returns `result<_, string>`. |
| `list-tools()` | Returns tool metadata (id, description, JSON Schema). |
| `call-tool(name, input_json)` | Executes one tool; returns `result<json, string>`. |
| `on-event(topic, payload_json)` | Delivery for declared subscriptions. |
| `on-command(id, args_json)` | Handles a manifest command routed by the shell. |
| `on-shutdown()` | Best-effort cleanup; hard timeout, then drop. |

The SDK's `ctx.register_tool(...)` (`PLUGINS.md` §4.2) is sugar over
`list-tools`/`call-tool`: registration is metadata, invocation is the export.
A component that lists a tool but does not implement it fails validation at
load (`plugin_invalid`).

**Instance and fault model.**

- One instance handles one call; instances are pooled per plugin
  (default max 4 concurrent).
- Event delivery is serialized per plugin through a bounded queue (1024);
  overflow drops oldest and emits `plugin.event.dropped`.
- A trap, fuel exhaustion, or epoch timeout drops the instance, returns
  `plugin_failed`, and emits `plugin.crashed`. Three failures within 60 s mark
  the plugin **degraded**: tools stay registered but return
  `plugin_unavailable` until a successful reload.
- No reentrancy: a plugin calling its own tool via `plugin.tool.call` is
  rejected (`plugin_reentrant`).

**Tool identity.** A plugin's declared tool ids must be globally unique across
kernel, MCP, and plugins. Installation rejects collisions with
`plugin_tool_conflict`; the convention is a dotted namespace (`rss.fetch`,
`com.example.github.*`). The kernel surface is `plugin.tool.call` with an
explicit `plugin_id`, so the shell and agents never guess provenance.

### 2. Host functions and permission enforcement

Every host function is gated by the plugin's declared manifest permissions,
the stored grant, and the kernel permission engine (`SECURITY.md` §10).

| Group | Functions | Permission | Scope / quota |
|---|---|---|---|
| Filesystem | `fs.read`, `fs.write`, `fs.list`, `fs.delete`, `fs.stat` | `fs.read`, `fs.write` | path globs; canonicalized; symlink containment; 8 MiB per call |
| Network | `http.request`, `http.stream` | `net.http` | URL scopes (scheme+host+path glob); redirects re-checked; private ranges blocked; 30 s timeout; 8 MiB response |
| LLM | `llm.chat`, `llm.embed`, `llm.rerank` | `llm.call` | model scopes; routed through `crates/llm` with per-plugin token/cost budget |
| MCP | `mcp.call`, `mcp.list_tools` | `mcp.connect` | server scopes; server must be connected/enabled |
| Storage | `storage.get`, `storage.set`, `storage.delete`, `storage.list` | `storage.local` | namespaced per plugin; 5 MiB total; keys ≤ 256 B; values ≤ 1 MiB |
| Events | `event.subscribe`, `event.emit`, `event.unsubscribe` | `events.subscribe`, `events.emit` | topic allowlists; emissions are namespaced (§ below) |
| Logging | `log.debug/info/warn/error` | none (always) | 500-entry in-memory ring per plugin |
| Secrets | `secrets.get` | `secrets.get` | name allowlist; values never logged, stored, or returned to the UI |
| Brain | `brain.search`, `brain.read`, `brain.write` | `brain.read`, `brain.write` | capability gates (`search`, `brain.write`) on top |
| UI (hybrid) | `ui.notify`, `ui.openPanel` | `ui.notify` | no payload over 4 KiB |

- **Resolution order** per call: plugin identity → stored grant
  (`plugin_permissions`) → permission service (`mode`, category policy, agent
  overrides) → execute. A missing grant is a denial, never an implicit allow.
- **Scope variables**: `$BRAIN`, `$CACHE`, `$PLUGIN_DATA`, `$FERGENTS_HOME`,
  `$PROJECT` (only in a trusted project). Paths are canonicalized before
  containment checks; `..` and symlink escapes are rejected.
- **Immediate revocation**: removing a grant invalidates the per-plugin check
  cache and fails subsequent calls (`plugin_permission_denied`). In-flight
  calls are cancelled where the host function supports cancellation.
- **Event provenance**: a plugin that declares `events.emit =
  ["rss.item.fetched"]` publishes `plugin.<plugin-id>.rss.item.fetched`.
  Emitting into kernel-owned namespaces (`file.*`, `agent.*`, `plugin.*` of
  another id, `capability.*`, …) is rejected.
- **Attribution**: LLM, MCP, and network calls made through host functions are
  recorded with `actor = "plugin:<id>"` in `llm_requests` / the audit log, so
  budgets and traces stay correct.
- Risky host calls (`fs.write`, `net.http`, `llm.call`, `mcp.call`,
  `secrets.get`, `brain.write`) are audited individually; safe reads are
  aggregated into per-plugin metrics.

### 3. Plugin registry, installation, lifecycle

**On-disk layout** (`PLUGINS.md` §12):

```
$FERGENTS_HOME/
├── plugins/installed/<plugin-id>/      # plugin.toml, plugin.wasm, ui.js, ui.css, assets/
├── plugins/dev/<plugin-id>/            # linked dev directory (not copied)
├── cache/wasm/<sha256>.cwasm
├── trust/keys/<key-id>.pub
└── trash/plugins/<plugin-id>-<ulid>/   # uninstalled, restorable
```

**Registry truth** is the `plugins` table plus the on-disk manifest. At boot
the kernel reconciles: a missing directory, hash mismatch, or parse error
marks the plugin disabled and emits `plugin.load_failed`; it never crashes
boot.

**Two-phase install** (mirrors `UI.md` §12 permission cards):

1. `plugin.inspect` resolves a source (`file`, `dir`, `git`, `marketplace`),
   validates the manifest/archive, computes the content hash, verifies the
   signature, and returns a **review** (ULID, 10-minute TTL, in memory) with
   the full permission list and any conflicts.
2. `plugin.install` takes the `review_id` plus the approved permission subset.
   Any unapproved permission aborts the install (`plugin_permissions_denied`).
3. The plugin is unpacked to `plugins/installed/<id>/`, recorded, loaded, and
   `plugin.installed` is emitted.

**Validation** (reject with actionable codes): manifest schema, id pattern,
semver, `api_version` compatibility, slot ids, command ids, tool id
uniqueness, permission names/scopes, archive safety (no absolute paths, no
`..`, ≤ 2 000 files, ≤ 64 MiB unpacked), WASM component validity, UI module
size. Archives are never extracted outside the target directory.

**Signing** (`PLUGINS.md` §11.2): Ed25519 over a canonical digest of the
manifest plus every file (sorted path + SHA-256). `signature_status` is
`verified`, `unknown_key`, `invalid`, or `none`. A `verified` signature
requires a key in `trust/keys/`. Unsigned or unknown-key plugins install only
after an explicit consent checkbox; dev-directory plugins are labelled `dev`
and cannot be published as-is.

**Lifecycle**: install → load (instantiate, `init`, register tools/events/
commands) → running → disable/enable → update → uninstall.
Update rules: semver only, no downgrade without explicit confirmation, a new
permission forces re-approval, and config survives. Uninstall cancels calls,
moves the directory to `trash/`, revokes grants, unregisters tools and slots,
and deletes plugin storage only on confirmation (with a trash backup).

**Dev mode**: `plugin.dev.link(path)` registers a directory; a `notify`
watcher (debounced 300 ms) reloads the WASM component and signals the UI host
to remount. State preservation is best-effort: plugin storage persists; UI
panels remount unless the plugin registered `getState`/`setState`. Every
reload emits `plugin.dev.reloaded`.

### 4. UI plugin host (React in Shadow DOM)

**Location**: `apps/desktop/src/plugins/host` (slot registry, mount,
bridge) and `packages/plugin-sdk` (`@fergents/plugin-sdk`).

- Each panel renders in its own **Shadow DOM** root with an adopted stylesheet
  built from `style/tokens.css` (tokens only; no `components.css` leakage
  into plugin DOM unless the plugin opts into the exported subset).
- **Isolation model**: Shadow DOM gives DOM/CSS isolation, **not** a JS
  sandbox — UI modules share the webview realm. The real security boundary is
  the kernel: the bridge binds the caller's `plugin_id`; the kernel rejects
  calls for another plugin and enforces permissions on every call (D5).
- **Bridge**: a frozen SDK object exposes `registerPlugin`, `useKernel`
  (call/query/subscribe), `ui.notify`, `ui.openPanel`, and `storage` (proxied
  through the kernel). No direct socket, no `invoke`, no Tauri API, no Node.
- **Registration**: `panels[]` (id, slot, title, render), `commands[]` (id,
  label, shortcut, `when`), optional `settings` page (schema-driven). Unknown
  slot ids, unsupported slot API versions, duplicate panel ids, and invalid
  commands are rejected (`plugin.ui.failed`) without affecting the shell.
- **Slot API v1** is the 16 slots of `UI.md` §22 (Appendix C). `UI.md` §22 is
  **authoritative**; the 12-name list in `ARCHITECTURE.md` §6.3 is the older
  subset and is superseded (D4).
- **Error containment**: one React error boundary per panel. A crash unmounts
  that panel only, emits `plugin.ui.crashed`, and offers "disable plugin".
- **Theming**: `theme.tokens` may override a small allowlist of tokens
  (`--accent`, `--surface-*`, `--radius-*`, `--font-*`) scoped to that
  plugin's shadow roots; global tokens are never mutated.
- **Accessibility**: panels are focusable landmarks with labels; focus is not
  trapped; shortcuts must not shadow `UI.md` §20 globals.
- **UI manifest caching**: panel metadata is returned by `plugin.list` (and
  `plugin.get`) so the shell can render slot placeholders before the module
  loads.

**Command palette contributions** (`UI.md` §11):

- Manifest `[commands]` + runtime `commands[]` merge into the palette as a
  data-driven provider; ids are `plugin.<plugin-id>.<command>`.
- A plugin shortcut that collides with a built-in is rejected; a collision
  between two plugins keeps the first registration and rejects the second
  (`plugin_command_conflict`).
- `when` is a small declarative predicate object (`capability`, `view`,
  `project_trusted`), not arbitrary code.
- The `command.palette` slot can render extra result rows; search-provider
  registration and nested palette modes are out of scope (Appendix G).

### 5. Skill runtime

**New crate `crates/skills`.** A skill is a folder (`SKILL.md` + optional
`prompt.md`, `skill.wasm`, `schema.json`, `examples/`, `README.md`), parsed
with the existing Markdown frontmatter handling (`crates/markdown`) and
validated against the frontmatter schema (`SKILLS.md` §11).

**Kinds and execution** (`skill.invoke`):

| Kind | Execution |
|---|---|
| `prompt` | Render the body/prompt with input variables → routed LLM call → parse JSON → validate `output` |
| `tool` | Call `wraps` (`mcp:<server>:<tool>` or `<kernel-tool>`) with the mapped input |
| `code` | Run `entry` (WASM component, `fergents:skill@1`) in the shared sandbox |
| `hybrid` | Declared step order (`steps`), default `[code, prompt]`; each step's output is shallow-merged over the original input |

Pipeline for every invocation: validate input → check permissions → resolve
dependencies (`capability.llm` / `mcp` / `plugins` as needed) → execute →
validate output → record metrics → return. Errors are explicit:
`skill_not_found`, `skill_disabled`, `skill_invalid_input`,
`skill_invalid_output`, `skill_permission_denied`, `skill_depth_exceeded`,
`skill_failed`.

- **Schemas are closed by default**: `additionalProperties: false` is assumed
  unless the schema sets `true`; unknown input keys are rejected.
- **Prompt repair**: one JSON-repair attempt on invalid model output, then
  `skill_invalid_output`. Prompt skills must declare a model or inherit the
  agent's route; `model_fallback` is honored.
- **Chaining**: a skill may call `skill.invoke` only if it declares the
  `skill.invoke` permission. The runtime keeps a call stack, enforces max
  depth **5**, rejects cycles by id (`skill_depth_exceeded`), and inherits the
  parent run's token/cost budget.
- **Caching**: prompt output is cached by
  `sha256(skill_id, version, model, temperature, canonical input)` for a TTL
  from `cache.ttl_s` (default 3 600 s). Cache is enabled by default only for
  skills with **no** permissions; file/network skills opt in. Updating a
  skill invalidates its cache.
- **Registry**: install/list/get/enable/disable/update/delete/duplicate/test.
  Built-in skills live under `skills/builtin/`, user skills under
  `$FERGENTS_HOME/skills/<id>/`; duplicate ids are rejected. Every install
  shows permissions for review.
- **Agent integration**: `AgentDefinition.skills` (already parsed in
  `crates/agents/src/definition.rs`) becomes live. For each **assigned and
  enabled** skill, the runtime registers a tool `skill.<skill-id>` with that
  skill's input schema; the generic `skill.invoke` tool stays available for
  workflows, plugins, and agents that prefer it. The context builder lists
  assigned skills (id, description, schema digest) in the capabilities
  summary.
- **AI authoring**: `skill.create`, `skill.update` (with a `.fergents`
  backup), `skill.duplicate`, `skill.delete` (confirmation), `skill.test`.
  AI-created skills install **enabled** when they declare no permissions, and
  **disabled** when they do — the user reviews and enables them (D17).

### 6. Plugin Factory (the AI loop)

Gated by `capability.plugin-factory` (default **off**). The loop is:

1. **Scaffold** — `factory.scaffold` copies a template into
   `$FERGENTS_HOME/factory/<run-id>/`. Templates: `kernel-tool`, `ui-panel`,
   `hybrid`, `theme`, `skill-prompt`, `skill-code`. UI templates import only
   `@fergents/plugin-sdk` and use `style/` tokens by construction.
2. **Write** — the agent writes files with `fs.write` scoped to the factory
   workspace (never the kernel source tree, never `$PROJECT` unless granted).
3. **Build** — `factory.build` invokes the discovered toolchain
   (`cargo component build --release`, or `jco componentize` for TypeScript)
   through `shell.exec` with an allowlist and a wall-clock limit. Missing
   toolchains return `toolchain_missing` with an install hint; **no automatic
   downloads**.
4. **Test** — `factory.test` runs the in-process `TestHost`: manifest
   validation, component instantiation, every declared tool against
   `examples/` inputs, schema round-trips, a permission matrix (granted vs
   denied), and resource-limit checks. UI is smoke-tested in Vitest with a
   mocked kernel. Results are a structured report.
5. **Package + install** — `factory.package` produces a `.fergplugin`;
   `factory.install` delegates to the normal two-phase install with the user's
   approval. The factory **never auto-installs** (D18).

Additional rules:

- Generated plugins/skills start **disabled** until the install review is
  approved; unsigned status is shown.
- A style check scans generated UI for hardcoded hex colors and raw pixel
  spacing and reports warnings in the review (templates are the primary
  enforcement, not a hard gate).
- Every factory step is a Phase 2 **run** (`kind = 'plugin-factory'`) with
  artifacts (files, build log, test report, diff), audited and reversible via
  `.fergents` backups.
- Factory tools: `factory.scaffold`, `factory.build`, `factory.test`,
  `factory.package`, `factory.install`, `factory.report`. They are exposed to
  agents only while the capability is on, and are permission-checked per
  action (`shell.exec` for builds, `fs.write` for the workspace,
  `plugin.install` for install).

### 7. Marketplace hooks

Gated by `capability.marketplace` (default **off**). Phase 4 ships the
**plumbing and a basic browser**, not a hosted service (D19).

- **Index v1** is JSON fetched over HTTPS or git from a configured source:
  entries for `kind = plugin|skill` with `id`, `name`, `version`,
  `api_version`, `description`, `tags`, `author`, `homepage`, `min_kernel`,
  `download { url, sha256, size }`, `signature`, `published_at`, `yanked`.
  The index itself is signed; plugin/skill payloads are verified after
  download (Ed25519 + SHA-256) before unpacking.
- **Commands**: `marketplace.sources.list/add/remove`, `marketplace.refresh`,
  `marketplace.search`, `marketplace.get`, `marketplace.install`,
  `marketplace.check_updates`.
- **Cache**: `marketplace_cache` (raw entry JSON + ETag + TTL, default 15
  min) so search works offline; a stale index is clearly labelled.
- **Trust**: source keys live in `trust/keys/`; unknown publisher keys are
  shown as `unknown_key`, never silently trusted. `yanked` entries cannot be
  installed.
- **Install** delegates to `plugin.inspect`/`plugin.install` or
  `skill.inspect`/`skill.install`, so the same review, consent, and audit
  paths apply.
- **Updates**: check only (no background auto-update in Phase 4); a version
  change produces a notification and a re-approval when permissions changed.
- Ratings, reviews, payments, moderation tooling, and a hosted registry are
  out of scope (Appendix G).

### 8. Plugin and skill UI

All UI is built from `style/` tokens and components; new patterns are added to
`style/` **first** and mirrored in `patterns.html` (Phase 3 rule, unchanged).

- **Settings → Plugins** (`apps/desktop/src/features/plugins`): installed
  list with search/filter, enable/disable, version, signature status, update
  badge, uninstall, and a detail view (permissions with revoke, tools, slots,
  commands, storage usage, logs, metrics, settings form generated from the
  manifest `[settings]` schema).
- **Install review** (reuses `UI.md` §12 permission card): source, publisher
  and signature status, every requested permission with scope, conflicts, and
  a single explicit approve action.
- **Settings → Skills** (`apps/desktop/src/features/skills`): list by kind,
  view `SKILL.md`, enable/disable, test runner (sample input → output),
  duplicate, delete, install from file/git.
- **Plugin Dev panel** (dev mode): logs, events, tool calls, a permission
  simulator, and event time travel (replay the last N events against the
  plugin). Visible only when a dev plugin is linked.
- **Marketplace browser** (basic, behind the capability): search, categories/
  tags, detail, install.
- **Palette entries**: "Install plugin…", "Manage plugins", "Install skill…",
  "Open plugin dev panel".
- Empty states, error states, and the permission review use existing
  `components.css` primitives (`panel`, `card`, `badge`, `toast`, `modal`,
  `empty state`).

### 9. Foundation

- **Global schema**: `0015_plugins.sql`, `0016_skills.sql`,
  `0017_marketplace.sql` (Appendix B).
- **Protocol**: Phase 4 commands and events (Appendix A), generated into
  `crates/protocol/src/generated.rs` and `packages/protocol/src/generated.ts`
  from `protocol/schema/protocol.schema.json`, with the existing freshness
  test.
- **Capability guards**: `crates/api` `required_capability` gains prefix
  mappings for `plugin.*` → `capability.plugins`, `skill.*` →
  `capability.skills`, `factory.*` → `capability.plugin-factory`,
  `marketplace.*` → `capability.marketplace`.
- **Permissions**: two new categories, `skill.install` and `skill.invoke`
  (Appendix E, MODIFIED Requirements).
- **Audit**: install, update, uninstall, permission changes, tool calls,
  host-function risky calls, factory builds, and marketplace installs are
  recorded with actor, arguments, result, and a reversibility reference.
- **Reversibility**: installs are additive; updates and uninstalls keep a
  trash copy and a `.fergents` backup of the previous manifest/config.
- **Observability**: per-plugin/skill load time, memory, call count, error
  rate, latency percentiles, event emissions, permission denials, and storage
  usage; logs carry `plugin=<id>` / `skill=<id>` tracing fields.
- **Docs**: `PLUGINS.md`, `SKILLS.md`, `ARCHITECTURE.md` §6 (slot list
  superseded), `SECURITY.md` §10, `STACK.md`, `ROADMAP.md` §8 checkboxes,
  `DECISIONS.md` (ADR-034+).

No **BREAKING** changes.

## Impact

- **Affected docs (source of truth)**: `ROADMAP.md` §8; `PLUGINS.md`;
  `SKILLS.md`; `ARCHITECTURE.md` §6, §10; `UI.md` §11, §12, §22;
  `SECURITY.md` §10, §18.2; `STACK.md` §2.3, §12, §23; `AGENTS.md` §7, §9;
  `BACKLOG.md` §0, §5; `DECISIONS.md`; `style/` (MANDATORY design system).
- **Affected code (new)**:
  - `crates/wasm`, `crates/packages`, `crates/plugins`, `crates/skills`.
  - `sdk/rust/fergents-plugin-sdk`, `sdk/rust/fergents-skill-sdk`,
    `packages/plugin-sdk`.
  - `migrations/global/0015_plugins.sql`, `0016_skills.sql`,
    `0017_marketplace.sql`.
  - `apps/desktop/src/plugins/host`, `apps/desktop/src/features/plugins`,
    `apps/desktop/src/features/skills`.
- **Affected code (modified, minimal)**:
  - `crates/capabilities` (catalogue + plugin-sourced capabilities).
  - `crates/permissions` (two categories, identity-aware requests).
  - `crates/agents` (per-skill tools, plugin tools in the context summary).
  - `crates/core` (service wiring, host-function adapters).
  - `crates/api` (dispatch arms + capability guards).
  - `crates/markdown` (only if the frontmatter API needs a public splitter).
  - `protocol/schema/protocol.schema.json`, `packages/protocol` (regenerated).
  - `apps/desktop/src/App.tsx` (slot host + settings tabs),
    `apps/desktop/src/features/palette/commands.ts` (dynamic provider),
    `apps/desktop/src/features/settings/SettingsView.tsx`.
  - `style/patterns.html` (install review, plugin detail, skill card).
- **Out of scope (later phases)**: marketplace backend/payments/ratings;
  workflow node execution (Phase 5); VS Code extension compatibility; remote
  plugin registry hosting; a JS/iframe sandbox beyond Shadow DOM; multi-user
  plugin trust; plugin-to-plugin direct messaging.

## Decisions (resolved)

Normative for Phase 4.

| # | Topic | Decision |
|---|---|---|
| D1 | WASM runtime | `wasmtime` + **Component Model** + WASI preview 2, one engine per process, in `crates/wasm`. WASI exposes only clocks/random; all I/O is host functions. |
| D2 | Plugin ABI | World `fergents:plugin@1`: `init`, `list-tools`, `call-tool`, `on-event`, `on-command`, `on-shutdown`; JSON strings across the boundary. Skills use `fergents:skill@1` with `run`. |
| D3 | Tool identity | Declared tool ids are globally unique; collisions fail install (`plugin_tool_conflict`); invocation is `plugin.tool.call` with an explicit `plugin_id`. |
| D4 | Slot authority | The **16 slots in `UI.md` §22** are the Phase 4 API v1. The 12-name list in `ARCHITECTURE.md` §6.3 is superseded and updated in the docs. |
| D5 | UI isolation | Shadow DOM only (per ADR-007). It is DOM/CSS isolation, not a code sandbox; the kernel bridge + permission engine is the security boundary, and every UI call is bound to its plugin id. |
| D6 | Instance model | One instance per call, pooled (max 4); per-plugin serialized events (queue 1024); no reentrancy; degraded after 3 failures/60 s. |
| D7 | Limits | Plugin: 64 MiB memory, 100 M fuel, 10 s per call. Skill: 32 MiB, 50 M fuel, 5 s. Input/output ≤ 8 MiB; storage 5 MiB/plugin, 1 MiB/skill (Appendix E). |
| D8 | Permissions | No implicit grants. Each host call resolves plugin grant → kernel policy → execute. New categories `skill.install`, `skill.invoke`; `plugin.install` covers install/update/uninstall. |
| D9 | Install consent | Install is **two-phase** (`inspect` → review → `install`). Permission approval is per-category and user-mediated; "full access" mode skips prompts but still audits and still requires unsigned consent. |
| D10 | Signatures | Ed25519 over a canonical manifest+files digest; trust store in `$FERGENTS_HOME/trust/keys/`; unsigned/unknown-key installs require explicit consent; dev links are labelled `dev`. |
| D11 | Registry truth | SQLite is authoritative; boot reconciles with disk and disables (never crashes on) invalid plugins/skills. |
| D12 | Skill format | Folder + `SKILL.md`; frontmatter parsed with the existing Markdown frontmatter code; JSON Schemas are **closed** by default. |
| D13 | Skill execution | prompt/tool/code/hybrid with an explicit pipeline (validate → permission → execute → validate); hybrid steps default to `[code, prompt]`; one JSON repair attempt for prompt output. |
| D14 | Chaining | `skill.invoke` requires the `skill.invoke` permission; depth 5; cycle detection; budget inheritance. |
| D15 | Skill cache | Prompt outputs cached by skill/version/model/temperature/input hash; TTL default 3 600 s; disabled by default for permitted skills; invalidated on update. |
| D16 | Agent exposure | Each assigned, enabled skill becomes a tool `skill.<skill-id>`; `skill.invoke` remains for generic callers; the context summary lists assigned skills. |
| D17 | AI-created skills | No-permission skills install enabled; skills that declare permissions install disabled pending user review. |
| D18 | Factory guardrails | Factory builds only in `$FERGENTS_HOME/factory/<run-id>/`, uses allowlisted toolchains, tests in `TestHost` (no network), never auto-installs, and never edits the kernel source tree. |
| D19 | Marketplace scope | Signed JSON/git index, search/inspect/install/update-check plumbing, basic browser, off by default. No hosted service, ratings, payments, or auto-update. |
| D20 | Dev mode | Directory links + `notify` watcher (300 ms debounce) reload WASM; the UI host remounts panels unless the plugin supplies `getState`/`setState`; production has no hot reload. |
| D21 | Observability | Metrics per plugin/skill in SQLite; lifecycle/failure events; logs tagged `plugin=`/`skill=`; every risky host call audited. |
| D22 | Testing | Fixture components committed under `crates/plugins/tests/fixtures`; no network, no real toolchain, no real LLM in CI; UI host tested in jsdom with a mock kernel. |

## ADDED Requirements

### Requirement: Phase 4 Capability Gating

The system SHALL register `capability.plugins` and `capability.skills` as
enabled-by-default, hot-toggleable capabilities, and gate every Phase 4
command behind them.

#### Scenario: Plugins disabled

- **WHEN** `capability.plugins` is disabled
- **THEN** no plugin is loaded, no slot is rendered, plugin tools are
  unregistered, and `plugin.*` commands return `capability_disabled`

#### Scenario: Skills disabled

- **WHEN** `capability.skills` is disabled
- **THEN** `skill.*` commands return `capability_disabled`, per-skill agent
  tools disappear, and installed skill files remain untouched on disk

#### Scenario: Plugin-provided capability

- **WHEN** an enabled plugin declares a capability
- **THEN** it appears in the capability list with `source = plugin:<id>`,
  defaults to off, and is removed on uninstall

### Requirement: Kernel Plugin Host

The system SHALL load WASM components in the shared sandbox and expose their
tools, events, and commands to the kernel.

#### Scenario: Registering a tool and a panel

- **WHEN** a plugin declaring a tool and a `sidebar.section` panel is enabled
- **THEN** the tool appears in the registry and the panel renders in the
  sidebar, both attributable to that plugin

#### Scenario: Component fails to instantiate

- **WHEN** a component traps, fails validation, or times out during `init`
- **THEN** the plugin is marked failed, `plugin.load_failed` is emitted, the
  kernel keeps running, and no tool is registered

#### Scenario: Fuel exhaustion

- **WHEN** a call exceeds its fuel or wall-clock limit
- **THEN** the instance is dropped, the call returns `plugin_failed`, and
  `plugin.crashed` is emitted

### Requirement: Host Functions and Scope Enforcement

The system SHALL expose only declared host functions, enforce path/domain/
model/server scopes, and audit risky calls.

#### Scenario: Filesystem escape

- **WHEN** a plugin calls `fs.read` outside its granted globs
- **THEN** the call returns `plugin_permission_denied` and nothing is read

#### Scenario: Symlink escape

- **WHEN** a granted path resolves through a symlink outside the scope
- **THEN** the canonicalized path fails containment and the call is denied

#### Scenario: Revocation

- **WHEN** the user revokes `net.http` from a running plugin
- **THEN** subsequent `http.*` calls fail immediately and an audit entry is
  written

#### Scenario: Event spoofing

- **WHEN** a plugin emits a topic in a kernel-owned namespace
- **THEN** the emission is rejected, while declared topics publish under
  `plugin.<plugin-id>.`

### Requirement: Plugin Registry and Installation

The system SHALL install, validate, sign-verify, update, disable, and
uninstall plugins with recoverable state.

#### Scenario: Signed install

- **WHEN** a user installs a `.fergplugin` signed by a trusted key
- **THEN** the review shows `verified`, approval installs it, and
  `plugin.installed` is emitted

#### Scenario: Unsigned install

- **WHEN** a plugin has no signature or an unknown key
- **THEN** the review marks it `none`/`unknown_key` and install requires an
  explicit consent step

#### Scenario: Malicious archive

- **WHEN** an archive contains an absolute path or `..` traversal
- **THEN** the install is rejected with `plugin_invalid_archive` and no file
  is written outside the target directory

#### Scenario: Update adds a permission

- **WHEN** a new version requests a permission that was not granted
- **THEN** the update pauses for re-approval and does not load the new version

#### Scenario: Uninstall is reversible

- **WHEN** a plugin is uninstalled
- **THEN** its directory moves to `trash/`, grants are revoked, tools and
  slots are removed, and the action is audited with a restore reference

### Requirement: UI Plugin Host and Slots

The system SHALL render UI plugins as isolated React roots in the 16 named
slots, with command contributions and contained failures.

#### Scenario: Slot registration

- **WHEN** a UI plugin declares a valid panel for `chat.composer.extras`
- **THEN** it mounts inside the composer's extras area, receives the v1 slot
  context, and uses only `style/` tokens

#### Scenario: Unknown slot or version

- **WHEN** a plugin targets an unknown slot or an unsupported slot API version
- **THEN** registration is rejected, `plugin.ui.failed` is emitted, and the
  shell is unaffected

#### Scenario: Panel crash

- **WHEN** a panel throws during render
- **THEN** only that panel unmounts, `plugin.ui.crashed` is emitted, and the
  user can disable the plugin

#### Scenario: Palette command

- **WHEN** a plugin registers a command
- **THEN** it appears in the palette as `plugin.<plugin-id>.<command>` and
  runs through the plugin's `on-command` or UI handler

#### Scenario: Shortcut conflict

- **WHEN** a plugin shortcut collides with a built-in or an earlier plugin
- **THEN** the conflicting registration is rejected

### Requirement: Skill Format and Registry

The system SHALL parse, validate, install, enable, and remove `SKILL.md`
skills from local, git, marketplace, and AI sources.

#### Scenario: Install a skill

- **WHEN** the user installs a skill folder
- **THEN** the frontmatter is validated, permissions are reviewed, the folder
  is placed under `$FERGENTS_HOME/skills/<id>/`, and the registry updates

#### Scenario: Invalid frontmatter

- **WHEN** a skill omits `id`, `name`, `version`, or `kind`, or has an
  unknown kind
- **THEN** installation is rejected with a field-level error

#### Scenario: Remove a skill

- **WHEN** a skill is removed
- **THEN** running invocations are cancelled, the registry updates, its agent
  tools disappear, and storage deletion is confirmed separately

### Requirement: Skill Execution and Chaining

The system SHALL execute prompt, tool, code, and hybrid skills with schema
validation, permissions, chaining limits, and caching.

#### Scenario: Prompt skill

- **WHEN** `com.fergents.summarize` is invoked with valid input
- **THEN** the input is validated, the LLM is called through the router, and
  the output validates against the skill's schema

#### Scenario: Code skill in the sandbox

- **WHEN** a code skill runs
- **THEN** it executes in the shared WASM sandbox with its declared
  permissions and limits, and no ambient filesystem or network

#### Scenario: Invalid output

- **WHEN** a prompt skill returns malformed JSON twice
- **THEN** the invocation fails with `skill_invalid_output` and the raw model
  text is included in the error details

#### Scenario: Chaining depth

- **WHEN** skills invoke each other beyond depth 5 or form a cycle
- **THEN** the call fails with `skill_depth_exceeded` before executing

#### Scenario: Cache hit

- **WHEN** a cached prompt skill is invoked with identical input and a valid
  TTL
- **THEN** the stored output is returned without an LLM call and
  `skill.cache.hit` is emitted

#### Scenario: Agent uses an assigned skill

- **WHEN** an agent whose definition lists a skill runs
- **THEN** a `skill.<id>` tool with that skill's schema is available and its
  calls are permission-checked

### Requirement: AI Skill Authoring

The system SHALL let the AI create, modify, duplicate, test, and delete
skills under the same permission rules as user actions.

#### Scenario: Create without permissions

- **WHEN** the AI creates a prompt skill with no permissions
- **THEN** the skill is installed enabled and immediately invocable

#### Scenario: Create with permissions

- **WHEN** the AI creates a skill that declares `net.http`
- **THEN** the skill is installed disabled and awaits explicit user approval

#### Scenario: Modify

- **WHEN** the AI updates an existing skill
- **THEN** a `.fergents` backup is written, the new version is validated, and
  the skill's cache is invalidated

### Requirement: Plugin Factory

The system SHALL provide a guarded loop in which the AI scaffolds, writes,
builds, tests, packages, and (with approval) installs a plugin or skill.

#### Scenario: Build and test

- **WHEN** the factory builds a generated Rust plugin
- **THEN** the allowlisted build runs in the factory workspace and the test
  harness instantiates the component and exercises every declared tool

#### Scenario: Toolchain missing

- **WHEN** `cargo component` is not available
- **THEN** the build fails with `toolchain_missing` and an actionable hint;
  nothing is downloaded

#### Scenario: Sandbox test failure

- **WHEN** a generated tool violates its schema or exceeds a limit
- **THEN** the report marks the run failed and installation is blocked

#### Scenario: No auto-install

- **WHEN** the factory finishes successfully
- **THEN** installation still requires the normal review and approval

### Requirement: Marketplace Hooks

The system SHALL support signed indexes and the search/inspect/install
plumbing for plugins and skills, behind an off-by-default capability.

#### Scenario: Index search

- **WHEN** `capability.marketplace` is on and an index is refreshed
- **THEN** `marketplace.search` returns matching entries from the cached index

#### Scenario: Signature mismatch

- **WHEN** a downloaded payload's hash or signature does not match the index
- **THEN** installation is rejected before unpacking

#### Scenario: Yanked entry

- **WHEN** an entry is marked `yanked`
- **THEN** it cannot be installed

#### Scenario: Capability off

- **WHEN** `capability.marketplace` is off
- **THEN** all `marketplace.*` commands return `capability_disabled` and no
  network request is made

### Requirement: Phase 4 Protocol Surface

The system SHALL add the Phase 4 commands and events (Appendix A) through the
existing schema/codegen pipeline and capability guards.

#### Scenario: Guarded dispatch

- **WHEN** a `plugin.*`, `skill.*`, `factory.*`, or `marketplace.*` command is
  dispatched with its capability disabled
- **THEN** the kernel returns `capability_disabled` with no side effects

#### Scenario: Generated contract

- **WHEN** the protocol schema changes
- **THEN** `gen:protocol` regenerates Rust and TypeScript and the freshness
  test fails on drift

### Requirement: Phase 4 Schema

The system SHALL add plugin, skill, and marketplace tables through new global
migrations (Appendix B).

#### Scenario: Idempotent migration

- **WHEN** the kernel opens the global store
- **THEN** `0015`–`0017` apply once, are recorded in `schema_version`, and an
  older kernel refuses a newer schema

### Requirement: Plugin and Skill UI (design-system compliance)

All Phase 4 UI SHALL use `style/` tokens and components, with new patterns
added to `style/` first.

#### Scenario: No hardcoded style

- **WHEN** any plugin-management, skill, review, or marketplace view renders
- **THEN** it uses only `style/` tokens (dark-first, glass, accessible)

#### Scenario: Permission review

- **WHEN** an install review is shown
- **THEN** it lists every requested permission with scope and blocks install
  until the user approves

### Requirement: Observability and Testing

Every Phase 4 task SHALL ship tests and keep CI green; Phase 4 tests SHALL NOT
require a network, a real toolchain, a real LLM, or a browser.

#### Scenario: Kernel tests

- **WHEN** the Rust suite runs
- **THEN** it covers manifest/archive validation, signatures, scope
  containment, fuel/epoch limits, registry reconciliation, skill parsing and
  execution kinds, chaining, caching, and capability guards using committed
  fixture components

#### Scenario: Desktop tests

- **WHEN** the Vitest suite runs
- **THEN** it covers the slot registry, shadow-root mounting, palette merging,
  error boundaries, the install review, and settings tabs with a mocked
  kernel

## MODIFIED Requirements

### Requirement: Capability Registry (Phase 2 §Capability Registry)

`capability.plugin-factory` SHALL become hot-toggleable and depend on
`capability.plugins` and `capability.skills`; the registry SHALL accept
plugin-sourced capabilities (`source = plugin:<id>`), list them alongside
kernel ones, and remove them on uninstall.

### Requirement: Permission Engine (Phase 2 §Permission Bar and Policy Engine)

`Category` SHALL gain `skill.install` and `skill.invoke` (14 categories
total), including `as_str`/`parse`/`is_risky`/`for_tool` coverage.
`PermissionRequest` SHALL accept a plugin/skill identity so plugin-originated
calls are attributed and cannot be confused with user or agent calls.

### Requirement: Agent Runtime and Tools (Phase 2 §Agent Runtime)

The `ToolRegistry` SHALL contain plugin tools and per-assigned-skill tools;
the context builder SHALL list installed plugins (id, tools) and assigned
skills so the model can choose them; every such call SHALL be permission
checked with the originating identity.

### Requirement: Protocol Dispatch (Phase 3 §Phase 3 Protocol Surface)

`required_capability` SHALL map `plugin.*`, `skill.*`, `factory.*`, and
`marketplace.*` to their capabilities, following the existing prefix
precedent.

### Requirement: Command Palette (Phase 3 §Desktop Shell)

`apps/desktop/src/features/palette/commands.ts` SHALL accept runtime
contributions from plugins (grouped under "Plugins") instead of remaining a
static array, while preserving existing built-in commands and shortcut
behaviour.

### Requirement: Settings (Phase 3 §Settings)

`SettingsView` SHALL gain **Plugins** and **Skills** tabs, plus a plugin
detail view and a dev panel entry point, following the existing tab pattern.

### Requirement: Reversibility and Audit (Phase 2 §Reversibility)

Install, update, uninstall, permission changes, factory builds, and
marketplace installs SHALL be audited and reversible (trash copy + `.fergents`
backup of manifest/config).

## REMOVED Requirements

None.

## Dependencies

New dependencies, each justified (`STACK.md` §2.3, §12, §22).

| Crate / package | Purpose | Justification |
|---|---|---|
| `wasmtime` (component-model, async, cranelift, epoch, fuel) | WASM host | `STACK.md` §12; ADR-006; decision D1 |
| `wasmtime-wasi` | WASI preview 2 (clocks/random only) | Component Model baseline; D1 |
| `ed25519-dalek` | Plugin/index signatures | `PLUGINS.md` §11.2; `SECURITY.md` §10; D10 |
| `semver` | Plugin/skill versions and compatibility | `PLUGINS.md` §13; D11 |
| `globset` | Permission path globs | Scope syntax in `PLUGINS.md` §7.2; D8 |
| `jsonschema` | Skill input/output and manifest validation | `SKILLS.md` §3, §11; D12 |
| `jco` (dev, npm) | TypeScript → WASM Component in the factory | `STACK.md` §22; template `skill-code`/`ui` |
| `@fergents/plugin-sdk` (workspace package) | UI SDK | `PLUGINS.md` §9.2 |

Reused, already in the workspace: `zip` (`.fergplugin`/`.fergskill`),
`sha2` + `hex` (digests), `notify` (dev watch), `reqwest` (marketplace/git
fetches), `toml`/`serde_yaml` (manifests), `rusqlite`, `tokio`,
`tempfile` (tests), `regex` (style check).

No dependency shall be added for: a marketplace backend, payments, a VS Code
extension host, a JS sandbox/iframe runtime, or a workflow engine (Phase 5).

## Appendix A: Phase 4 protocol surface

**Command methods (Phase 4)**

- Plugins: `plugin.list`, `plugin.get`, `plugin.inspect`, `plugin.install`,
  `plugin.update`, `plugin.uninstall`, `plugin.enable`, `plugin.disable`,
  `plugin.reload`, `plugin.tool.call`, `plugin.command.run`,
  `plugin.permissions.get`, `plugin.permissions.set`, `plugin.settings.get`,
  `plugin.settings.set`, `plugin.storage.usage`, `plugin.storage.clear`,
  `plugin.logs`, `plugin.stats`.
- Plugin dev: `plugin.dev.link`, `plugin.dev.unlink`, `plugin.dev.reload`.
- Skills: `skill.list`, `skill.get`, `skill.inspect`, `skill.install`,
  `skill.create`, `skill.update`, `skill.delete`, `skill.enable`,
  `skill.disable`, `skill.duplicate`, `skill.test`, `skill.invoke`,
  `skill.invocations`.
- Factory: `factory.scaffold`, `factory.build`, `factory.test`,
  `factory.package`, `factory.install`, `factory.report`.
- Marketplace: `marketplace.sources.list`, `marketplace.sources.add`,
  `marketplace.sources.remove`, `marketplace.refresh`, `marketplace.search`,
  `marketplace.get`, `marketplace.install`, `marketplace.check_updates`.

**Event kinds (Phase 4)**

```
plugin.installed / plugin.updated / plugin.uninstalled
plugin.enabled / plugin.disabled / plugin.reloaded
plugin.loaded / plugin.load_failed / plugin.crashed / plugin.shutdown
plugin.degraded / plugin.event.dropped
plugin.tool.registered / plugin.tool.failed / plugin.command.failed
plugin.permission.denied / plugin.log
plugin.ui.registered / plugin.ui.failed / plugin.ui.crashed
plugin.dev.linked / plugin.dev.reloaded / plugin.dev.unlinked

skill.installed / skill.updated / skill.removed
skill.enabled / skill.disabled / skill.cache.hit
skill.invoked / skill.completed / skill.failed

factory.started / factory.scaffolded / factory.built
factory.tested / factory.packaged / factory.failed

marketplace.index.refreshed / marketplace.update.available
```

Plugin-declared emissions are published under
`plugin.<plugin-id>.<declared-topic>`.

## Appendix B: Global schema (normative)

New files under `migrations/global/`, numbered after `0014_terminal.sql`.

- **`plugins`** — `id TEXT PRIMARY KEY`, `name TEXT NOT NULL`,
  `version TEXT NOT NULL`, `api_version INTEGER NOT NULL`,
  `kind TEXT NOT NULL` (`kernel|ui|hybrid`), `source TEXT NOT NULL`
  (`file|dir|git|marketplace`), `path TEXT NOT NULL`, `manifest_hash TEXT NOT
  NULL`, `signature_status TEXT NOT NULL` (`verified|unknown_key|invalid|
  none|dev`), `enabled INTEGER NOT NULL DEFAULT 0`, `dev INTEGER NOT NULL
  DEFAULT 0`, `degraded INTEGER NOT NULL DEFAULT 0`, `config TEXT NULL`
  (JSON), `installed_at INTEGER NOT NULL`, `updated_at INTEGER NOT NULL`.

- **`plugin_permissions`** — `plugin_id TEXT NOT NULL`, `category TEXT NOT
  NULL`, `scope TEXT NULL` (JSON array or `true`), `granted INTEGER NOT NULL`,
  `granted_at INTEGER NULL`; primary key `(plugin_id, category)`.

- **`plugin_storage`** — `plugin_id TEXT NOT NULL`, `key TEXT NOT NULL`,
  `value BLOB NOT NULL`, `updated_at INTEGER NOT NULL`; primary key
  `(plugin_id, key)`; size enforced by the host.

- **`plugin_calls`** — `id TEXT PRIMARY KEY`, `plugin_id TEXT NOT NULL`,
  `tool TEXT NOT NULL`, `status TEXT NOT NULL` (`ok|error|denied|timeout`),
  `duration_ms INTEGER NOT NULL`, `error_code TEXT NULL`,
  `created_at INTEGER NOT NULL`; index `(plugin_id, created_at)`.

- **`skills`** — `id TEXT PRIMARY KEY`, `name TEXT NOT NULL`,
  `version TEXT NOT NULL`, `kind TEXT NOT NULL`
  (`prompt|tool|code|hybrid`), `source TEXT NOT NULL`
  (`local|git|marketplace|ai|builtin`), `path TEXT NOT NULL`,
  `manifest_hash TEXT NOT NULL`, `enabled INTEGER NOT NULL DEFAULT 1`,
  `config TEXT NULL` (JSON), `installed_at INTEGER NOT NULL`,
  `updated_at INTEGER NOT NULL`.

- **`skill_permissions`** — `skill_id TEXT NOT NULL`, `category TEXT NOT
  NULL`, `scope TEXT NULL` (JSON), `granted INTEGER NOT NULL`; primary key
  `(skill_id, category)`.

- **`skill_storage`** — `skill_id TEXT NOT NULL`, `key TEXT NOT NULL`,
  `value BLOB NOT NULL`, `updated_at INTEGER NOT NULL`; primary key
  `(skill_id, key)`.

- **`skill_cache`** — `skill_id TEXT NOT NULL`, `input_hash TEXT NOT NULL`,
  `output TEXT NOT NULL` (JSON), `expires_at INTEGER NOT NULL`,
  `created_at INTEGER NOT NULL`; primary key `(skill_id, input_hash)`; index
  `(expires_at)`.

- **`skill_invocations`** — `id TEXT PRIMARY KEY`, `skill_id TEXT NOT NULL`,
  `parent_id TEXT NULL`, `depth INTEGER NOT NULL`, `status TEXT NOT NULL`
  (`ok|error|denied`), `duration_ms INTEGER NOT NULL`, `created_at INTEGER
  NOT NULL`; index `(skill_id, created_at)`.

- **`marketplace_sources`** — `id TEXT PRIMARY KEY`, `url TEXT NOT NULL`,
  `kind TEXT NOT NULL` (`git|https`), `key_id TEXT NULL`, `enabled INTEGER
  NOT NULL DEFAULT 1`, `last_refreshed_at INTEGER NULL`, `etag TEXT NULL`.

- **`marketplace_cache`** — `source_id TEXT NOT NULL`, `entry_id TEXT NOT
  NULL`, `kind TEXT NOT NULL` (`plugin|skill`), `entry TEXT NOT NULL` (JSON),
  `fetched_at INTEGER NOT NULL`; primary key `(source_id, kind, entry_id)`.

## Appendix C: UI slot contracts (v1)

The 16 slots from `UI.md` §22 are the complete v1 surface.

| Slot | Host region | Context passed | Cardinality | Notes |
|---|---|---|---|---|
| `rail.item` | Rail | `{ active: boolean }` | one per plugin | icon + label + order |
| `sidebar.section` | Sidebar | `{ collapsed: boolean }` | up to 3 | collapsible section |
| `main.tab` | Main area | `{ tabId }` | up to 3 | plugin-owned tab type |
| `editor.toolbar` | Editor top | `{ file, selection }` | up to 4 buttons | read-only context |
| `editor.gutter` | Editor left | `{ file, visibleRange }` | 1 renderer | decorations only; no edits |
| `chat.message.actions` | Chat message | `{ messageId, role }` | up to 4 | action buttons |
| `chat.composer.extras` | Composer | `{ draft, attachments }` | up to 4 | read-only draft snapshot |
| `chat.permission.card` | Permission card | `{ category, target }` | 1 | extra info only; never grants |
| `statusbar.item` | Status bar | `{ activeView }` | up to 3 | short text/icon |
| `rightpanel.section` | Right panel | `{ selection }` | up to 3 | collapsible section |
| `dashboard.widget` | Dashboard | `{ }` | up to 4 | card-sized widget |
| `command.palette` | Palette | `{ query }` | 1 contributor | static result rows |
| `settings.section` | Settings | `{ }` | up to 2 | plugin settings page |
| `graph.overlay` | Graph | `{ viewport }` | 1 overlay | SVG layer, read-only |
| `workflow.node` | Workflow editor | `{ nodeId }` | declared node types | render only in Phase 4; execution is Phase 5 |
| `theme.tokens` | Theming | `{ scope }` | 1 override set | allowlisted tokens, shadow-scoped |

Slot ids are versioned by plugin `api_version`; Phase 4 ships v1 only. The
legacy ARCHITECTURE §6.3 names (`sidebar.rail`, `sidebar.panel`) are not
accepted and are corrected in the docs.

## Appendix D: Capability catalogue seed (Phase 4)

| Id | Name | Default | Hot | Dependencies |
|---|---|---|---|---|
| `capability.plugins` | Plugins | on | yes | `capability.agents` |
| `capability.skills` | Skills | on | yes | `capability.agents` |
| `capability.marketplace` | Marketplace | off | yes | `capability.plugins`, `capability.skills` |
| `capability.plugin-factory` | Plugin Factory | off | yes | `capability.plugins`, `capability.skills`, `capability.agents` |

Ordering in `default_catalogue()` keeps dependencies before dependents (the
`seed` invariant). Plugin-provided capabilities are appended at runtime.

## Appendix E: Security constraints

- **Sandbox**: WASM components run in wasmtime with no ambient authority; WASI
  exposes only clocks/random; filesystem, network, secrets, and LLM access
  exist solely as permission-gated host functions.
- **Limits**: plugin 64 MiB / 100 M fuel / 10 s; skill 32 MiB / 50 M fuel /
  5 s; input and output ≤ 8 MiB; storage 5 MiB per plugin and 1 MiB per skill;
  event queue 1024; concurrent instances 4. Limits are kernel-enforced, not
  plugin-declared.
- **Scopes**: path globs canonicalized and symlink-checked; URL scopes checked
  per redirect; private IP ranges blocked; model and MCP-server allowlists.
  `$PROJECT` is only available in a trusted project (Phase 3 D3).
- **Secrets**: only manifest-declared names; values never enter logs, plugin
  storage, prompts, or UI responses.
- **Signatures**: Ed25519 over manifest + files; trust store; unsigned/unknown
  key requires explicit consent; dev links are labelled and cannot be
  published as-is; archive extraction is path-contained.
- **UI**: Shadow DOM isolates DOM/CSS only. UI plugins share the webview realm,
  so the security boundary is the kernel bridge: caller identity is bound,
  kernel calls are permission-checked, and only `[ui].kernel_calls` plus the
  plugin's own tools are callable.
- **Events**: plugin emissions are namespaced; kernel-owned namespaces cannot
  be spoofed; subscriptions are limited to declared topics.
- **Factory**: builds run in `$FERGENTS_HOME/factory/<run-id>/` with an
  allowlisted toolchain and no network by default; generated code is tested
  only in `TestHost`; installation always goes through review.
- **Marketplace**: index and payload signatures and hashes are verified before
  unpacking; yanked entries are not installable; no auto-update execution.
- **Audit**: install/update/uninstall, permission changes, risky host calls,
  factory builds, and marketplace installs are recorded with actor,
  arguments, result, and a reversibility reference.

## Appendix F: Testing strategy

- **Rust**
  - `crates/wasm`: component instantiation, fuel/epoch/memory limits, trap
    isolation, compiled-cache reuse.
  - `crates/packages`: archive path traversal/size guards, Ed25519
    verification with test keys, SHA-256 mismatch, semver/`api_version`
    compatibility.
  - `crates/plugins`: manifest validation, registry reconciliation,
    install/update/uninstall with trash restore, scope containment (globs,
    `..`, symlinks), permission denial, dev reload, event namespacing.
  - `crates/skills`: frontmatter parsing, closed-schema validation, all four
    kinds, hybrid step order, chaining depth/cycles, cache TTL/invalidation,
    tool wrapping.
  - `crates/permissions`: new categories in `ALL`/`as_str`/`parse`/`is_risky`
    and identity-aware requests.
  - `crates/api`: IPC tests for every Phase 4 command, including
    `capability_disabled` for all four capabilities.
  - Fixture components are committed under `crates/plugins/tests/fixtures/`
    (hash-verified); no network or toolchain is required.
- **Desktop (Vitest)**
  - Slot registry and shadow-root mount/unmount; slot context objects; error
    boundaries; palette merge and shortcut conflicts; install review wizard.
  - Plugins/Skills settings tabs; dev panel; marketplace list; mocked
    `KernelClient`; no canvas/worker required.
- **Acceptance (manual canary, documented in `checklist.md`)**
  - Install a signed sample plugin; use its tool from chat; see its panel.
  - Deny one permission and confirm the tool fails with a card/audit entry.
  - Install, run, chain, and remove a prompt and a code skill.
  - Ask the AI to create a skill; review; invoke it from an agent.
  - Ask the AI to create a plugin; build/test report appears; approve install;
    panel renders.
  - Disable `capability.plugins` / `capability.skills`; commands are refused.
  - Hot-reload a dev plugin's WASM and UI.

## Appendix G: Out of scope (later phases)

- Hosted marketplace backend, ratings, reviews, payments, moderation tooling.
- Automatic background plugin/skill updates (checks only in Phase 4).
- Workflow node **execution** (slot renders in Phase 4; runtime is Phase 5).
- VS Code/Open VSX extension compatibility and an extension host.
- A JS/iframe sandbox beyond Shadow DOM; UI plugins remain webview-realm code.
- Plugin-to-plugin direct messaging/channels (`AGENTS.md` §11) and remote
  plugin execution.
- Project-local skill/plugin scopes and per-workspace trust for plugins.
- Kernel self-modification by the factory (kernel crates are never written).
- Multi-user/team plugin trust and signed skill marketplaces with payments.

---

*End of `spec.md` — Phase 4.*
