# Phase 5 — Tasks

Backend tasks (5.1–5.5) are implemented in order. Each ships code, tests, and a
green local `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test`
before the next begins. UI tasks (5.6–5.11) come after the backend is merged and
are **out of scope** until then.

The normative contract for every task below is [spec.md](./spec.md); section
references (§) point into it.

---

## Task 5.1 — `crates/workflow`: document model, schema, validation, templates, expressions

**Files**

- `crates/workflow/Cargo.toml`
- `crates/workflow/src/lib.rs`
- `crates/workflow/src/error.rs`
- `crates/workflow/src/document.rs`
- `crates/workflow/src/schema.rs`
- `crates/workflow/schema/workflow.schema.json`
- `crates/workflow/src/validate.rs`
- `crates/workflow/src/graph.rs`
- `crates/workflow/src/template.rs`
- `crates/workflow/src/expr.rs`
- `crates/workflow/tests/{document,validate,template,expr}.rs`
- `Cargo.toml` (workspace `members` + `[workspace.dependencies]`)

**Work**

1. Add the crate to the workspace `members`; reuse `serde_yaml`, `serde_json`,
   `thiserror`, `chrono` from `[workspace.dependencies]`. No new third-party
   dependency without a spec justification.
2. `WorkflowError` in `error.rs`, mirroring `crates/skills/src/error.rs`:
   `thiserror` enum, `#[from]` conversions for storage/io/json/yaml, and
   `fn code(&self) -> &'static str` returning the Appendix A wire codes.
3. `document.rs`: `WorkflowDocument`, `Node`, `NodeKind` (the twelve node types
   of §1.2), `Edge`, `Port`, `TriggerSpec` (§2.1), `PermissionsBlock` (§1.8),
   `RetryPolicy`, `TimeoutSpec`, `OnError`. `serde` with `deny_unknown_fields`
   on every struct; `#[serde(default)]` where the spec defines a default.
4. `schema/workflow.schema.json`: hand-written JSON Schema (draft 2020-12) that
   matches the document model, embedded with `include_str!` and parsed once by
   `schema.rs`. Keep it in sync with the Rust types; a test asserts the schema
   `$id` and the top-level required keys.
5. `validate.rs`: the eight graph rules of §1.1 plus the node config checks of
   Appendix C and the §1.8 permission block. Returns
   `Validation { errors: Vec<Issue>, warnings: Vec<Issue> }` with
   `Issue { path, node, code, message }`; errors are collected, not short
   circuited.
6. `graph.rs`: topological rank computation (§3.1), reachability from triggers,
   cycle detection that permits cycles only inside `loop` bodies, and the
   unknown-source lint of §1.9.
7. `template.rs`: the `{{ }}` grammar of Appendix D — path access, filters,
   escaping, missing-variable policy (`workflow_template_missing`), 1 MiB
   artifact spill and 32 MiB cap (§1.5). `render(value, ctx) -> Result<Value>`.
8. `expr.rs`: the condition/transform evaluator of §1.6 — comparisons, boolean
   composition, membership, null coalescing, and the transform operations. Pure,
   total, deterministic; no arbitrary code execution.

**Acceptance**

- A committed YAML fixture validates clean; a broken fixture reports every
  violated rule with a path.
- Template tests cover nested paths, each filter, missing-variable, escaping,
  and the spill/cap boundary.
- `cargo test -p fergents-workflow` passes; `cargo clippy -p fergents-workflow
  -- -D warnings` is clean.
- Public API is documented with `///` where the spec defines normative
  behaviour.

---

## Task 5.2 — Engine, scheduler, run store, checkpoints

**Files**

- `migrations/global/0018_workflow.sql`
- `crates/workflow/src/store.rs`
- `crates/workflow/src/scheduler.rs`
- `crates/workflow/src/engine.rs`
- `crates/workflow/src/executor/mod.rs` + one module per node type
- `crates/workflow/src/checkpoint.rs`
- `crates/workflow/src/context.rs`
- `crates/workflow/tests/{store,engine,scheduler}.rs`

**Work**

1. `0018_workflow.sql` exactly as Appendix B: `workflows`, `workflow_versions`,
   `workflow_runs`, `workflow_steps`, `workflow_checkpoints`,
   `workflow_triggers`. Idempotent `CREATE TABLE IF NOT EXISTS`, forward only,
   no alteration of existing tables.
2. `store.rs`: `RunStore` over `fergents-storage::SqliteStorage` — create run,
   append step, write checkpoint, transition state, list runs/steps, prune
   checkpoints to 50 per run (§3.3). Every read returns typed structs, never raw
   rows.
3. `context.rs`: the run context object (§1.5), node output namespacing, and
   edge input assembly from explicit `inputs` templates (§1.4).
4. `scheduler.rs`: deterministic dispatch by `(rank, node_id)` (§3.1). Ready-set
   computation, fan-out, fan-in barrier, and end-node termination.
5. `engine.rs`: the run lifecycle of §3.2
   (`queued → running → (paused | waiting_human | interrupted) → done | failed |
   cancelled`); persist transitions; emit run/step events through
   `fergents-events`.
6. `executor/`: one module per node type — `llm`, `tool`, `agent`, `code`,
   `http`, `condition`, `loop`, `subworkflow`, `human`, `transform`, `end`.
   Executors receive a narrow `NodeContext` (capability handle, permission
   checks, cancellation token, budget) and never touch the store directly.
7. `checkpoint.rs`: serialize context after each node, restore it on resume,
   reject checkpoints that do not match the document digest
   (`workflow_version_conflict`).
8. Manual invocation path: `workflow.run` persists a run and returns its
   `run_id` immediately; execution continues on the engine task.

**Acceptance**

- A linear fixture runs end to end, persists one step per node, and finishes
  `done`.
- Dispatch order is asserted to be stable across repeated runs (same document,
  same order).
- Checkpoint pruning keeps the newest 50.
- `cargo test -p fergents-workflow` passes; migration applies on a fresh store
  and on a store already at `0017`.

---

## Task 5.3 — Trigger runtime (cron, webhook, file, event, manual, AI)

**Files**

- `crates/workflow/src/trigger/mod.rs`
- `crates/workflow/src/trigger/{cron,webhook,file,event,manual,ai}.rs`
- `crates/workflow/src/trigger/registry.rs`
- `crates/workflow/tests/trigger_*.rs`

**Work**

1. `registry.rs`: one row per `(workflow_id, trigger_id)` in
   `workflow_triggers`; enable/disable, `last_fired_at`, `next_fire_at`,
   `last_error`. A failing trigger records the error and stays armed.
2. **Cron** (§2.2): a single scheduler task, 6-field-or-5-field expressions,
   the catch-up policy (`skip | run_once | run_all`), timezone handling, and
   next-fire computation persisted so restarts do not double-fire.
3. **Webhook** (§2.3): loopback listener only, bearer or header token,
   constant-time comparison, `404` (not `401`) on a bad token so the endpoint
   is not discoverable, 60 requests/minute per trigger, idempotency key
   dedupe.
4. **File** (§2.4): `notify` watcher, 300 ms debounce, path containment inside
   the declared roots, no follow of symlinks out of scope.
5. **Event** (§2.5): subscribe through `fergents-events`; topic + filter match
   against the event payload.
6. **Manual** (§2.6): the `workflow.run` command path.
7. **AI** (§2.7): the agent-invoked path — same engine entry as manual, but the
   actor is the agent session and the run records `trigger_kind = "ai"`.
8. Trigger test command: `workflow.trigger.test` fires a synthetic payload
   without persisting a step history beyond the run record.

**Acceptance**

- Each of the six kinds fires exactly one run in tests (cron with an injected
  clock, webhook against the loopback listener, file with a temp dir, event
  with a published payload, manual and AI through the engine entry).
- Bad webhook token returns `404` and no run; rate limit returns the spec's
  status without a run.
- Cron restart with `run_once` does not replay missed windows twice.

---

## Task 5.4 — Recovery, retries, cancellation, budgets, human nodes

**Files**

- `crates/workflow/src/budget.rs`
- `crates/workflow/src/recovery.rs`
- `crates/workflow/src/human.rs`
- `crates/workflow/src/retry.rs`
- `crates/workflow/src/compensation.rs`
- `crates/workflow/tests/{recovery,budget,human,cancel}.rs`

**Work**

1. `retry.rs` (§3.4): fixed/exponential backoff, jitter, max attempts, retryable
   vs terminal error classification by wire code.
2. Timeouts (§1.3, §3.4): per-node `timeout_s`; a timeout is a node failure and
   feeds the retry policy.
3. `recovery.rs` (§3.6): on kernel start, mark `running` runs `interrupted`;
   **never** auto-resume. `workflow.run.resume` restores the newest checkpoint,
   verifies the document digest, and continues from the next ready node. A run
   with no checkpoint or a digest mismatch returns `workflow_run_not_resumable`
   / `workflow_version_conflict`.
4. Cancellation (§3.5): cooperative token into every node; 5 s grace, then
   abort; terminal state `cancelled`. Pause/resume are distinct from cancel.
5. `budget.rs` (§3.9): token, cost, wall-time, and tool-call budgets, checked
   before each LLM/tool call; exhaustion pauses the run and raises
   `workflow_budget_exceeded`.
6. `human.rs` (§3.7): `human` node creates an approval row, transitions to
   `waiting_human`, and completes on `workflow.run.approve` /
   `workflow.run.reject`; timeout per the node's `timeout_s`.
7. `compensation.rs` (§3.8): `on_error` policies — `fail`, `continue`, `retry`,
   `compensate`; compensation steps are recorded as regular steps.
8. Iteration limit for `loop` nodes (§1.6): stop with
   `workflow_iteration_limit`.

**Acceptance**

- A killed run is reconstructed after restart, marked `interrupted`, and resumes
  from the right node.
- Retry succeeds on a transient failure and exhausts to a terminal failure with
  the right code.
- Budget exhaustion pauses; approve/reject resumes; cancel stops within grace.
- `cargo test -p fergents-workflow` passes.

---

## Task 5.5 — Protocol, capability wiring, agent tools

**Files**

- `protocol/schema/protocol.schema.json`
- `crates/protocol/src/generated.rs` (regenerated)
- `packages/protocol/src/generated.ts` (regenerated)
- `crates/capabilities/src/catalogue.rs`
- `crates/permissions/src/model.rs`
- `crates/api/src/server.rs`
- the agent tool registry crate that owns the built-in tool table
- `crates/workflow/tests/e2e.rs`

**Work**

1. Add the `workflow.*` methods of Appendix A to the protocol schema, their
   request/response `$defs`, and the error codes. Run both generators
   (`cargo run -p fergents-protocol --bin gen-protocol` and
   `pnpm --filter @fergents/protocol gen`) and commit the outputs.
2. Register `capability.workflow` (`default_enabled = false`, `hot_toggle =
   true`, depends on `capability.agents`). Upgrade `capability.workflow-debugger`
   to depend on `capability.workflow` and allow hot toggling; keep its default
   off.
3. Add `Category::WorkflowRun` (`workflow.run`); update `ALL` to 15, `as_str`,
   `parse`, `is_risky` (running arbitrary node graphs is risky), and `for_tool`
   so `workflow.run` / `workflow.run.*` map to `WorkflowRun` while document
   editing keeps mapping to `WorkflowEdit`.
4. Add the two routing arms to `required_capability`, `workflow.debug.` first.
5. Wire the command handlers: document CRUD, versions, runs, triggers, apps.
   `workflow.run` returns `{ run_id }` as soon as the run is persisted.
6. Agent tools: `workflow.list`, `workflow.run`, `workflow.status`,
   `workflow.cancel`, `workflow.resume`, each permission-checked and each
   returning the structured tool-result envelope of `AGENTS.md` §6.5.
7. End-to-end test: capability off ⇒ every `workflow.*` method returns
   `capability_disabled`; capability on ⇒ validate, run, trace, cancel,
   resume all work through the protocol.

**Acceptance**

- `cargo test` across the workspace passes; protocol freshness tests pass;
  `pnpm --filter @fergents/protocol gen` is a no-op on a clean tree.
- `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` are clean.
- Capability on/off gating is asserted for both prefixes.
- e2e covers run → step inspect → cancel and run → approve → done.

---

## Deferred — UI tasks (5.6–5.11)

Not started until the backend is merged. Listed for traceability only; see
spec §4 and §5.

| # | Task | Depends on |
|---|---|---|
| 5.6 | Canvas and inspector (React Flow, node palette, property panel, YAML round-trip) | 5.5 |
| 5.7 | Debugger and trace (step, continue, inspect, replay from step) | 5.6 |
| 5.8 | Versioning and run history (diff, restore, pinned runs) | 5.6 |
| 5.9 | Workflow apps (publish, run, unpublish, export/import) | 5.6 |
| 5.10 | Custom nodes and agent tools UI | 5.6 |
| 5.11 | Polish and docs (styleguide, a11y, i18n, `WORKFLOW.md` / `DATA_MODEL.md` alignment) | 5.9, 5.10 |
