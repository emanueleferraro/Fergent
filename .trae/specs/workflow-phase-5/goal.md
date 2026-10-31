# Phase 5 — Workflow Engine: Goal

## Objective

Deliver a kernel-owned, deterministic DAG workflow engine that can be defined
as a versioned document, validated and dry-run without side effects, triggered
by six independent trigger kinds, executed with durable checkpoints and full
crash recovery, and driven from the desktop shell through the existing JSON-RPC
protocol. When `capability.workflow` is enabled, a user can author a workflow,
run it, inspect every step, resume an interrupted run, and publish it as a
mini-app. When the capability is disabled, the kernel rejects every
`workflow.*` method with `capability_disabled` before allocating a run, before
emitting any event, and without exposing any surface to the UI.

The work is governed by [spec.md](./spec.md), which is the normative contract.
This document states the outcome and the order of delivery; it does not restate
the specification.

## Success criteria

1. **A workflow is a document, not code.** Every workflow is a YAML/JSON
   document validated against a committed JSON Schema. Invalid documents are
   rejected with a structured `workflow_invalid` error carrying a path, a code,
   and a human message. Validation never partially applies a document.
2. **The engine is deterministic.** Given the same document, the same inputs,
   and the same trigger payload, the run produces the same node dispatch order
   and the same recorded steps. Dispatch order is defined by graph rank and node
   id, never by map iteration order or wall-clock timing.
3. **The run is durable.** Every completed node writes a checkpoint. A run that
   is interrupted — by crash, by shutdown, by cancellation with grace — can be
   reconstructed from `workflow_runs` + `workflow_steps` + `workflow_checkpoints`
   alone. After a restart, interrupted runs are marked `interrupted` and are
   never auto-resumed.
4. **Triggers are independent and observable.** Cron, webhook, file watch, event
   bus, manual, and AI invocation each have their own runtime, their own
   enable/disable state in `workflow_triggers`, and their own last-fire and
   next-fire bookkeeping. A failing trigger records `last_error` and does not
   take down the scheduler.
5. **Termination is guaranteed.** Every run is bounded by a node timeout, a
   retry policy, an iteration limit for loops, and a token/cost/time/tool-call
   budget. Exceeding any bound ends the run with a specific error code, never
   with an unbounded loop.
6. **The capability gate is real.** `capability.workflow` is registered in the
   capability catalogue with `capability.agents` as a dependency. With it
   enabled, `workflow.*` methods route to the engine; with it disabled, they are
   rejected by `required_capability()` in the API server. `workflow.debug.*`
   routes to `capability.workflow-debugger` and is checked first.
7. **The protocol is generated, not hand-written.** New `workflow.*` methods and
   their `$defs` are added to `protocol/schema/protocol.schema.json`, and both
   the Rust and TypeScript bindings are regenerated and committed. The
   freshness tests stay green.
8. **Agents can drive workflows.** The built-in tool surface gains
   `workflow.list`, `workflow.run`, `workflow.status`, `workflow.cancel`, and
   `workflow.resume`, each permission-checked through `Category::WorkflowRun`
   and the kernel's policy engine.
9. **The backend lands before the frontend.** No file under `apps/desktop` is
   modified in this phase's backend work. The React Flow editor is a separate,
   later deliverable defined by spec §4.

## Sequencing (normative)

The backend is delivered in five tasks, in this order. Later tasks depend on
earlier ones; do not reorder.

1. **5.1 — Document model.** `crates/workflow`: the `WorkflowDocument` types,
   the committed JSON Schema, the validator (graph rules §1.1, node rules,
   permission block), the `{{ }}` template engine (§1.5, Appendix D), and the
   expression/condition evaluator (§1.6). No storage, no execution.
2. **5.2 — Engine and persistence.** `migrations/global/0018_workflow.sql`,
   the run store (`workflow_runs`, `workflow_steps`, `workflow_checkpoints`),
   the deterministic scheduler (§3.1), the executor, and checkpoint pruning
   (§3.3).
3. **5.3 — Trigger runtime.** The cron scheduler with catch-up policy, the
   loopback webhook listener with token auth and rate limiting, the file watcher
   with debounce and containment, the event-bus subscriber, manual invocation,
   and AI invocation.
4. **5.4 — Resilience.** Retry policies, timeouts, cancellation with grace,
   crash recovery, budgets (§3.9), human nodes (§3.7), and compensation (§3.8).
5. **5.5 — Protocol and wiring.** Schema additions, regenerated bindings,
   `capability.workflow` registration, `Category::WorkflowRun`, the
   `required_capability()` routing arms, and the agent tools.

## Constraints

- **Spec is the contract.** Where this document and [spec.md](./spec.md)
  disagree, the spec wins. Where the spec is silent, prefer the smallest change
  that satisfies an existing spec requirement.
- **No UI.** `apps/desktop` is untouched. The `capability.workflow-debugger`
  catalogue entry keeps its current description until the editor ships.
- **Reuse the kernel.** Persistence goes through `fergents-storage`, events
  through `fergents-events`, permissions through `fergents-permissions`. No new
  database driver, no new event mechanism, no parallel permission model.
- **Wire codes, not prose.** Every error carries a stable `code()` string from
  the spec's Appendix A list. Log messages may change; codes may not.
- **Additive migrations only.** `0018_workflow.sql` is idempotent and forward
  only. Existing tables are not altered.
- **CI stays green.** `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test`, the protocol freshness tests, and the Node typecheck must all
  pass before a PR is opened.
- **One PR per task.** Each of 5.1–5.5 lands as its own reviewable pull
  request, opened with `gh`.
