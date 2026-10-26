# Phase 4 — Plugin Runtime & Skills: Goal

## Objective

Deliver the extension platform described in
[`spec.md`](./spec.md): sandboxed WASM plugins, Shadow-DOM UI plugins, a
declarative skill runtime, the AI plugin factory, and the marketplace
plumbing — all gated by kernel capabilities, permissions, audit, and
reversibility.

## Success criteria

1. `capability.plugins` and `capability.skills` are registered, enabled by
   default, and hot-toggleable; disabled capabilities refuse their commands.
2. A plugin can register a tool and a UI panel; a skill can be installed,
   invoked, chained, cached, and removed.
3. A malicious plugin cannot read the filesystem, open network connections,
   or read secrets without an explicit, scoped grant.
4. The AI can author a plugin/skill, build it, test it in a sandbox, and
   install it only through the normal approval path.
5. Every Phase 4 mutation is audited and reversible.
6. CI stays green; the frontend is untouched until the backend crates and
   protocol commands are merged and tested.

## Sequencing (normative)

Backend first, in this order:

1. **4.0 Foundation** — capabilities, permission categories, migrations,
   dispatch guards.
2. **4.1 Sandbox and packaging** — `crates/wasm`, `crates/packages`.
3. **4.2 Plugin host and registry** — `crates/plugins`.
4. **4.3 Skill runtime** — `crates/skills`.
5. **4.4 Factory and marketplace plumbing** — `crates/core`, `crates/api`.

The React host (`apps/desktop`) is **out of scope** until 4.1–4.4 are merged
and tested.

## Constraints

- No React or UI changes in the backend PRs.
- No new hosted services; no auto-downloads; no auto-install.
- No network, real toolchain, or real LLM in CI tests.
- Follow the existing crate patterns, error mapping, migrations, capability
  registry, and permission engine.
- Keep `cargo fmt`, `cargo clippy`, and `cargo test` green.
