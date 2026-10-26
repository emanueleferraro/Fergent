# Phase 4 — Checklist

Status legend: `[ ]` pending, `[x]` done, `[~]` deferred (documented).

## Spec acceptance (ROADMAP §8)

- [ ] A plugin can register a new tool and a new UI panel.
- [ ] A skill can be installed, run, and removed.
- [ ] The AI can create a plugin and install it.
- [ ] The AI can create a skill and use it.
- [ ] Permissions are enforced and auditable.
- [ ] Plugin isolation works (a malicious plugin cannot read the FS).
- [ ] `PLUGINS.md` and `SKILLS.md` match the implementation.

## Task 4.0 — Foundation

- [ ] `capability.plugins` and `capability.skills` registered (on, hot).
- [ ] `capability.marketplace` registered (off, hot).
- [ ] `capability.plugin-factory` made hot-toggleable with dependencies.
- [ ] Permission categories `skill.install` and `skill.invoke` added.
- [ ] Migrations `0015_plugins.sql`, `0016_skills.sql`,
      `0017_marketplace.sql` applied idempotently.
- [ ] `required_capability` maps `plugin.*`, `skill.*`, `factory.*`,
      `marketplace.*`.
- [ ] Capability catalogue test updated.

## Task 4.1 — `crates/packages` and `crates/wasm`

- [ ] Safe archive extraction (absolute paths, `..`, size/count caps).
- [ ] Deterministic digest over sorted files.
- [ ] Ed25519 signature verification with a trust key.
- [ ] Semver and `api_version` (N-1) compatibility.
- [ ] Sandbox loads a Component Model component.
- [ ] Fuel/epoch/memory limits enforced.
- [ ] `host.call` dispatcher gated by a `HostBridge`.
- [ ] Tests pass without network or a toolchain.

## Task 4.2 — `crates/plugins`

- [ ] Manifest parsing and validation (`plugin.toml`).
- [ ] Registry persistence and boot reconciliation.
- [ ] Install inspect → review → install (permission subset).
- [ ] Enable/disable/update/uninstall with trash restore.
- [ ] Tool registration and `plugin.tool.call`.
- [ ] Event namespacing and subscription allowlists.
- [ ] Dev linking and reload.
- [ ] Plugin-provided capabilities registered/removed.

## Task 4.3 — `crates/skills`

- [ ] `SKILL.md` frontmatter parsing and validation.
- [ ] Prompt, tool, code, and hybrid execution.
- [ ] Closed-schema input/output validation.
- [ ] Chaining depth 5 and cycle detection.
- [ ] Prompt cache with TTL and invalidation.
- [ ] Registry install/enable/disable/remove/duplicate/test.
- [ ] Per-skill agent tools resolved from `AgentDefinition.skills`.

## Task 4.4 — Factory and marketplace plumbing

- [ ] `factory.*` commands (scaffold/build/test/package/install/report).
- [ ] `marketplace.*` commands (sources/refresh/search/get/install/check).
- [ ] Trust store and signature verification before unpacking.
- [ ] Kernel wiring and API dispatch for every Phase 4 command.
- [ ] Audit events for install/update/uninstall/permission changes.
- [ ] Capability-disabled tests for all four capabilities.

## Deferred (explicitly out of scope until backend merge)

- [~] `apps/desktop` slot host, plugins/skills settings, palette, dev panel.
- [~] `packages/plugin-sdk` and SDK documentation site.
- [~] Marketplace browser UI.
- [~] Workflow node execution (Phase 5).

## CI gates

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] No UI files changed in backend PRs.
