# Phase 2 — Goal

## Objective

Turn Fergents from a knowledge base (Phase 1) into a **chat-first, agent-native
workspace**, and make the whole product **modular**: every feature is a
kernel-registered **Capability** that can be switched off, and a disabled
capability consumes no resources and is invisible in the UI.

Source of truth: [spec.md](./spec.md). This goal exists to keep the work
verifiable and bounded.

## Definition of done

Phase 2 is complete when:

1. **Capabilities are real.** Every Phase 2 feature is a registered capability
   in the global `capabilities` table. Disabling one stops the kernel from
   allocating its tasks/watchers/pools and unregisters its tools/commands/
   events; the UI hides it. Dependency enforcement, plugin registration, and
   the Settings → Features page all work (spec D1–D3, D15; ADR-021).
2. **An agent can chat over the brain.** Streamed chat with a real router over
   ≥1 provider (offline `ollama`/`llama.cpp` plus one cloud adapter), tool
   calls with permission checks, persistent sessions, and memory (working,
   episodic, semantic, `brain.md`, `session.md`).
3. **The context is transparent and controllable.** The Context Viewer shows
   per-section token usage using the kernel's own counter, and toggling a
   section actually removes it from the next model call (spec D8).
4. **Rich outputs are safe.** AI charts/Mermaid/HTML render only behind their
   capability, inside a sandboxed frame, with schema validation and
   sanitization (spec D4–D5; ADR-022).
5. **The prompt improves itself, safely.** `brain.md` updates are diffed,
   default-`suggest`, audited, size-capped, secret-scrubbed, and revertible
   (spec D6; ADR-023).
6. **Complex coding tasks are gated.** The spec → plan → tasks workflow pauses
   for human approval at each configured gate and produces `spec.md`,
   `plan.md`, `tasks.md` as artifacts (spec D7; ADR-024).
7. **Everything is reversible and permissioned.** Every mutation has a
   permission decision and a restore path (shadow git / `.fergents`).
8. **It works offline.** Chat, agents, memory, context viewer, and the spec
   workflow run with a local model and no network.
9. **Quality is enforced.** Each task ships tests and CI is green on all
   platforms before merge (backlog rule).

## Scope

### In scope

- New crates: `fergents-capabilities`, `fergents-llm`, `fergents-mcp`,
  `fergents-agents` (runtime + memory).
- New global tables: `capabilities`, `agent_*`, `runs`, `run_steps`,
  `approvals`, `chat_prompts`, `context_snapshots`, `secret_refs`,
  `agent_messages_fts` (spec Appendix B).
- Provider adapters (OpenAI-compatible, Anthropic, Google, Ollama, llama.cpp),
  the router, budgets, caches.
- MCP host + a minimal MCP server exposing brain tools.
- The desktop chat UI, Context Viewer, Agent Builder, Provider/MCP managers,
  Secrets, and the Settings → Features page — **only after the backend
  foundation is merged**.
- Reversibility: shadow git + `.fergents` backups.

### Out of scope (later phases)

- Monaco / LSP / terminal / IDE (Phase 3).
- Plugin runtime + marketplace (Phase 4); Workflow engine + debugger
  (Phase 5); Dashboards (Phase 6); Sync & Android (Phase 7).
- Where a backlog item depends on a later substrate, Phase 2 ships the
  capability hook only (spec Appendix F).

## Constraints

- **Design system is law.** Any new UI uses only `style/` tokens/components;
  new tokens/components are added to `style/` first and mirrored in
  `patterns.html`.
- **Offline by default.** No network required for the default build or CI.
- **Tests per task.** Rust tests for backend, Vitest for UI.
- **CI before merge.** `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --workspace`, offline test, `cargo build`; ESLint, Prettier,
  `tsc --noEmit`, `pnpm -r test`, `pnpm build`.
- **GitHub flow.** Branch → PR → green CI → merge, Conventional Commits.
- **Autonomy order.** Backend foundation (capabilities → llm → mcp) is done and
  merged **before** any `apps/desktop` work begins.

## Key risks

| Risk | Mitigation |
|---|---|
| Capability toggling leaks resources | "Disabled ⇒ unallocated" is tested (no tasks/tools/commands registered). |
| Provider differences leak into the runtime | One normalized `Provider` trait + a deterministic mock provider in CI. |
| Charts/HTML become an XSS vector | Sandboxed iframe, strict CSP, schema validation, DOMPurify, no `innerHTML`. |
| Self-improvement corrupts the prompt | Diff-gated, `suggest` by default, backup + revert, size cap, secret scrub. |
| Approval gates block automation | Per-run autonomy (`supervised` / `batch` / `auto`). |
| Protocol churn breaks clients | Phase 2 commands are additive; existing commands/events unchanged. |

## Deliverables

- `goal.md`, `tasks.md`, `checklist.md` (this phase).
- Merged PRs for tasks 2.1, 2.2, 2.3 (backend foundation), then the rest.
- Working chat, agent runtime, Context Viewer, capability toggles, spec
  workflow, and reversibility.
- Updated docs where behavior changes, and new ADRs (ADR-021…ADR-026).
