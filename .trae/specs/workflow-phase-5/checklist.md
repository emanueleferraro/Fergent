# Phase 5 — Checklist

Status legend: `[ ]` pending, `[x]` done, `[~]` deferred (documented).

## Spec acceptance (ROADMAP §9)

- [ ] Build a workflow visually, run it, and see the trace. *(UI — deferred to
      5.6–5.8)*
- [ ] Trigger a workflow via cron and via webhook.
- [ ] A workflow calls an agent, which calls a tool, which calls MCP.
- [ ] Replay a failed run from a step.
- [ ] Plugins can add custom node types.
- [ ] `WORKFLOW.md` matches the implementation.

## Task 5.1 — Document model, schema, validation, templates, expressions

- [ ] `crates/workflow` added to the workspace with workspace-inherited deps.
- [ ] `WorkflowError` with the Appendix A wire codes and `code()` accessor.
- [ ] Document types cover all twelve node kinds with `deny_unknown_fields`.
- [ ] `schema/workflow.schema.json` committed and embedded.
- [ ] All eight §1.1 validation rules implemented, errors collected not short
      circuited.
- [ ] Graph module: rank, reachability, cycle detection, unknown-source lint.
- [ ] Template engine: paths, filters, escaping, missing-variable policy,
      1 MiB spill / 32 MiB cap.
- [ ] Expression engine: conditions and transforms, pure and deterministic.
- [ ] Valid and invalid YAML fixtures covered by tests.

## Task 5.2 — Engine, scheduler, run store, checkpoints

- [ ] `0018_workflow.sql` matches Appendix B and is idempotent.
- [ ] `RunStore` create/append/transition/list with typed results.
- [ ] Deterministic `(rank, node_id)` dispatch; stable across runs.
- [ ] Lifecycle states and transitions persisted and evented.
- [ ] One executor module per node type behind a narrow `NodeContext`.
- [ ] Checkpoints written per node, pruned to 50, digest-verified on restore.
- [ ] `workflow.run` persists and returns `{ run_id }` immediately.
- [ ] Migration applies on a fresh store and on a store at `0017`.

## Task 5.3 — Trigger runtime

- [ ] Trigger registry with enable/disable, last/next fire, `last_error`.
- [ ] Cron scheduler with catch-up policy and persisted next-fire.
- [ ] Loopback webhook with constant-time token check, `404` on bad token,
      60 rpm, idempotency dedupe.
- [ ] File watcher with 300 ms debounce and path containment.
- [ ] Event subscription with topic and payload filter.
- [ ] Manual and AI invocation paths.
- [ ] `workflow.trigger.test` fires without polluting step history.

## Task 5.4 — Recovery, retries, cancellation, budgets, human nodes

- [ ] Retry policies with backoff, jitter, and retryable-code classification.
- [ ] Per-node timeouts feed the retry policy.
- [ ] Startup marks orphaned `running` runs `interrupted`; never auto-resumes.
- [ ] Resume restores the newest checkpoint and verifies the digest.
- [ ] Cancellation: cooperative token, 5 s grace, terminal `cancelled`.
- [ ] Budgets: tokens, cost, wall time, tool calls; exhaustion pauses.
- [ ] Human nodes create approvals, wait, and resolve on approve/reject.
- [ ] `on_error` policies including compensation steps.
- [ ] Loop iteration limit raises `workflow_iteration_limit`.

## Task 5.5 — Protocol, capability wiring, agent tools

- [ ] `workflow.*` methods and `$defs` added to the protocol schema.
- [ ] Rust and TypeScript bindings regenerated and committed; freshness tests
      pass.
- [ ] `capability.workflow` registered (off, hot, depends on agents).
- [ ] `capability.workflow-debugger` depends on `capability.workflow` and is
      hot-toggleable.
- [ ] `Category::WorkflowRun` added; `ALL` is 15; `as_str`/`parse`/`is_risky`/
      `for_tool` updated.
- [ ] `required_capability` routes `workflow.debug.` before `workflow.`.
- [ ] Command handlers wired: document, versions, runs, triggers, apps.
- [ ] Agent tools `workflow.list/run/status/cancel/resume` registered and
      gated.
- [ ] e2e: capability off ⇒ `capability_disabled`; on ⇒ validate, run, trace,
      cancel, resume.

## CI gates (every task)

- [ ] `cargo fmt --check` clean.
- [ ] `cargo clippy --workspace -- -D warnings` clean.
- [ ] `cargo test --workspace` green.
- [ ] `pnpm --filter @fergents/protocol gen` is a no-op on a clean tree.
- [ ] Node typecheck and desktop build green (**unmodified**, but must not
      regress).
- [ ] `apps/desktop` untouched in tasks 5.1–5.5.

## Deferred

- [~] 5.6 Canvas and inspector.
- [~] 5.7 Debugger and trace.
- [~] 5.8 Versioning and run history.
- [~] 5.9 Workflow apps.
- [~] 5.10 Custom nodes and agent tools UI.
- [~] 5.11 Polish and docs.
