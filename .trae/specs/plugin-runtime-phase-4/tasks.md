# Phase 4 — Tasks

Backend tasks are implemented in order. Each task ships code, tests, and a
green local `cargo check`/`cargo test` before the next begins. UI tasks are
deferred until the backend is merged.

## Task 4.0 — Foundation (capabilities, permissions, schema, guards)

**Files**

- `crates/capabilities/src/catalogue.rs`
- `crates/permissions/src/model.rs`
- `crates/api/src/server.rs` (`required_capability`)
- `migrations/global/0015_plugins.sql`, `0016_skills.sql`,
  `0017_marketplace.sql`
- `.trae/specs/plugin-runtime-phase-4/{goal,tasks,checklist}.md`

**Work**

1. Register `capability.plugins` and `capability.skills` (default on, hot),
   `capability.marketplace` (default off, hot); upgrade
   `capability.plugin-factory` (hot, depends on plugins + skills + agents).
2. Add `Category::SkillInstall` (`skill.install`) and `Category::SkillInvoke`
   (`skill.invoke`), updating `ALL`, `as_str`, `parse`, `is_risky`, and
   `for_tool`.
2b. Add `Category::PluginCall`? No — plugin tools keep their inferred
   categories; install/update/uninstall use `plugin.install`.
3. Add the Phase 4 global migrations (Appendix B of the spec).
4. Extend `required_capability` for `plugin.*`, `skill.*`, `factory.*`,
   `marketplace.*`.

**Acceptance**

- `cargo test -p fergents-capabilities -p fergents-permissions` passes.
- New capability defaults are asserted in a test.
- Migrations apply on a fresh store and on an existing store.

---

## Task 4.1 — `crates/packages` and `crates/wasm`

**Files**

- `crates/packages/{Cargo.toml,src/lib.rs,src/archive.rs,src/digest.rs,src/signature.rs,src/version.rs,tests/*}`
- `crates/wasm/{Cargo.toml,build.rs?,wit/host.wit,src/lib.rs,src/engine.rs,src/host.rs,src/limits.rs,tests/*}`

**Work — packages**

1. `PackageError` with stable codes (`package_io`, `package_invalid`,
   `package_unsafe_path`, `package_too_large`, `package_hash_mismatch`,
   `package_signature_invalid`, `package_api_incompatible`).
2. `extract_zip(bytes, dest, Limits)` rejecting absolute paths, `..`,
   symlinks, and exceeding file count/size.
3. `compute_digest(dir)` — SHA-256 over sorted relative paths + contents,
   excluding `signature.ed25519`.
4. `verify_signature(dir, public_key, signature)` — Ed25519.
5. `api_compatible(required, current)` — equal or N-1; semver helpers.

**Work — wasm**

6. WASM Component Model host (`wasmtime`, component-model, async, fuel,
   epoch, cranelift).
7. WIT worlds `fergents:plugin@1` and `fergents:skill@1`; one import
   interface `host.call(op, payload) -> result<string, string>`.
8. `Limits` (memory, fuel, wall time, input/output sizes) enforced per call.
9. `HostBridge` trait: the only route from a component to fs/http/llm/mcp/
   storage/events/secrets. Permission checks live in the bridge.
10. `Sandbox::invoke_skill` and `Sandbox::call_tool` helpers; trap/timeout
    mapped to `SandboxError`.

**Acceptance**

- Archive/signature/version tests pass with temp dirs and test keys.
- A committed fixture component executes `run` under limits.
- Fuel exhaustion and a scope-denied host call are tested.

---

## Task 4.2 — `crates/plugins`

**Files**

- `crates/plugins/{Cargo.toml,src/lib.rs,src/manifest.rs,src/registry.rs,src/install.rs,src/host.rs,src/tools.rs,src/dev.rs,src/error.rs,tests/*}`

**Work**

1. `plugin.toml` model: `[plugin]`, `[kernel]`, `[ui]`, `[permissions]`,
   `[slots]`, `[workflow_nodes]`, `[commands]`, `[capabilities]`,
   `[settings]`; strict unknown-field rejection.
2. Validation: id pattern, semver, `api_version`, slot ids (16), command/
   tool uniqueness, permission names, archive limits.
3. Registry: DB rows + disk reconciliation; enable/disable/degraded.
4. Install: `inspect` (review id, permission diff, signature status),
   `install` (approval subset), `update`, `uninstall` (trash + storage
   confirmation).
5. Loader: instantiate via `crates/wasm`; call `init`; collect `list-tools`;
   register `plugin.<...>` tools into the agent `ToolRegistry`; route
   `plugin.tool.call`.
6. Host bridge implementation: fs/http/llm/mcp/storage/events/log/secrets/
   brain/ui with scope checks and quotas; event emissions namespaced.
7. Dev mode: link/unlink + `notify` watcher (debounce 300 ms) + reload.
8. Plugin-provided capabilities: register on enable, remove on disable/
   uninstall.

**Acceptance**

- Manifest tests (valid/invalid), install review/approval, tool conflict,
  archive safety, registry reconcile, dev reload.
- Tool calls denied by scope return `plugin_permission_denied`.
- No plugin is loaded when `capability.plugins` is off.

---

## Task 4.3 — `crates/skills`

**Files**

- `crates/skills/{Cargo.toml,src/lib.rs,src/manifest.rs,src/registry.rs,src/runtime.rs,src/cache.rs,src/tools.rs,src/error.rs,tests/*}`

**Work**

1. `SKILL.md` parsing with `crates/markdown` frontmatter; validate the
   frontmatter schema (`SKILLS.md` §11); closed input/output schemas via
   `jsonschema`.
2. Registry: install/enable/disable/remove/duplicate/test; hash + reconcile.
3. Runtime pipeline: validate → permissions → execute → validate → metrics.
4. `prompt` (model route + JSON output + one repair), `tool` (delegate),
   `code` (sandbox), `hybrid` (ordered steps).
5. Chaining with depth 5, cycle detection, shared run budget.
5. Prompt cache (hash + `ttl_s`, invalidation on update).
6. Agent integration: one tool per attached skill, named `skill.<id>`, plus
   the general `skill.invoke`.
7. AI helpers: `create`, `update` (backup), `delete`, `test`; install
   enabled only when no permissions are declared.

**Acceptance**

- Parse/validate fixtures for all four kinds.
- Schema rejection, chaining depth/cycle, cache hit/expiry, and tool
  delegation tests.
- Prompt skills run against a test `ModelClient` without network.

---

## Task 4.4 — Core wiring, factory, marketplace plumbing

**Files**

- `crates/core/{Cargo.toml,src/lib.rs,src/factory.rs,src/market.rs}`
- `crates/api/src/server.rs`
- `crates/protocol` (only if typed params/results are added)

**Work**

1. Wire `Sandbox`, `PluginService`, `SkillService` into `Kernel`.
2. Register plugin tools and skill tools before `AgentService` is built.
- 2b. Extend the context/capabilities summary with plugins and skills
   (read-only; no agent-runtime rewrite).
3. Factory: `factory.scaffold/build/test/package/install/report`, workspace
   under `$FERGENTS_HOME/factory/<run-id>/`, allowlisted toolchain, test
   harness, no auto-install.
4. Marketplace: sources, refresh, search, get, install (delegates),
   update-check; signed index + payload verification; cache.
5. API: dispatch arms for every command in Appendix A; error mapping
   (`capability_disabled`, `plugin_*`, `skill_*`, `marketplace_*`).
6. Audit events for install/update/uninstall/permission changes; metrics
   rows (`plugin_calls`, `skill_invocations`).

**Acceptance**

- IPC tests for: install/disable/uninstall, tool call, skill invoke, factory
  scaffold/test, marketplace search with a local fixture index.
- Capability-disabled tests for all four capabilities.
- `cargo test --workspace` green.

---

## Deferred tasks (UI, after backend merge)

- 4.5 `apps/desktop` Shadow DOM host + 16 slots.
- 4.6 Plugins/Skills settings, install review, dev panel.
- 4.7 Palette contributions + marketplace browser.
- 4.8 `packages/plugin-sdk` + SDK docs/examples.
