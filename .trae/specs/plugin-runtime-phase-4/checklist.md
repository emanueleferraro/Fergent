# Phase 4 — Checklist

Status legend: `[ ]` pending, `[x]` done, `[~]` deferred (documented).

## Spec acceptance (ROADMAP §8)

- [ ] A plugin can register a new tool and a new UI panel. *(tool: done; UI panel: Task 4.5)*
- [x] A skill can be installed, run, and removed.
- [ ] The AI can create a plugin and install it. *(factory backend done; AI tool wiring pending)*
- [ ] The AI can create a skill and use it. *(factory backend done; AI tool wiring pending)*
- [x] Permissions are enforced and auditable.
- [x] Plugin isolation works (a malicious plugin cannot read the FS).
- [ ] `PLUGINS.md` and `SKILLS.md` match the implementation.

## Task 4.0 — Foundation

- [x] `capability.plugins` and `capability.skills` registered (on, hot).
- [x] `capability.marketplace` registered (off, hot).
- [x] `capability.plugin-factory` made hot-toggleable with dependencies.
- [x] Permission categories `skill.install` and `skill.invoke` added.
- [x] Migrations `0015_plugins.sql`, `0016_skills.sql`,
      `0017_marketplace.sql` applied idempotently.
- [x] `required_capability` maps `plugin.*`, `skill.*`, `factory.*`,
      `marketplace.*`.
- [x] Capability catalogue test updated.

## Task 4.1 — `crates/packages` and `crates/wasm`

- [x] Safe archive extraction (absolute paths, `..`, size/count caps).
- [x] Deterministic digest over sorted files.
- [x] Ed25519 signature verification with a trust key.
- [x] Semver and `api_version` (N-1) compatibility.
- [x] Sandbox loads a Component Model component.
- [x] Fuel/epoch/memory limits enforced.
- [x] `host.call` dispatcher gated by a `HostBridge`.
- [x] Tests pass without network or a toolchain.

## Task 4.2 — `crates/plugins`

- [x] Manifest parsing and validation (`plugin.toml`).
- [x] Registry persistence and boot reconciliation.
- [x] Install inspect → review → install (permission subset).
- [x] Enable/disable/update/uninstall with trash restore.
- [x] Tool registration and `plugin.tool.call`.
- [x] Event namespacing and subscription allowlists.
- [x] Dev linking and reload.
- [x] Plugin-provided capabilities registered/removed.

## Task 4.3 — `crates/skills`

- [x] `SKILL.md` frontmatter parsing and validation.
- [x] Prompt, tool, code, and hybrid execution.
- [x] Closed-schema input/output validation.
- [x] Chaining depth 5 and cycle detection.
- [x] Prompt cache with TTL and invalidation.
- [x] Registry install/enable/disable/remove/duplicate/test.
- [x] Per-skill agent tools resolved from `AgentDefinition.skills`.

## Task 4.4 — Factory and marketplace plumbing

- [x] `factory.*` commands (scaffold/build/test/package/install/report).
- [x] `marketplace.*` commands (sources/refresh/search/get/install/check).
- [x] Trust store and signature verification before unpacking.
- [x] Kernel wiring and API dispatch for every Phase 4 command.
- [x] Audit events for install/update/uninstall/permission changes.
- [x] Capability-disabled tests for all four capabilities.

## Deferred (explicitly out of scope until backend merge)

- [~] `apps/desktop` slot host, plugins/skills settings, palette, dev panel.
- [~] `packages/plugin-sdk` and SDK documentation site.
- [~] Marketplace browser UI.
- [~] Workflow node execution (Phase 5).

## CI gates

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] No UI files changed in backend PRs.
