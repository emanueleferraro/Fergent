# Phase 3 — Goal

## Objective

Turn Fergents from a chat-first agent workspace (Phase 2) into an **IDE**: open
a real code project, edit it in Monaco, get language intelligence from a
kernel-side LSP client, run commands in a PTY terminal, review and commit with
git, and let the agent work inside all of it — with a chat pane next to the
code. This completes the MVP.

Source of truth: [spec.md](./spec.md). This goal exists to keep the work
verifiable and bounded.

## Definition of done

Phase 3 is complete when:

1. **A project is a first-class object.** Open/close/list/trust a folder; the
   project layer detects the VCS root and honours ignore files; a new project
   starts **untrusted** and nothing executable runs until it is trusted
   (spec D3; ADR-031).
2. **The kernel owns the files.** All project reads/writes go through
   `file.*` with `base_etag` optimistic concurrency: a stale write returns
   `file_conflict` and never overwrites; external changes raise
   `file.changed`; every mutation is reversible (spec D2; ADR-027).
3. **Language intelligence works.** A kernel LSP client speaks
   `Content-Length`-framed JSON-RPC to real servers, with diagnostics,
   completion, hover, definition, references, rename, symbols, formatting,
   and code actions; servers are discovered, never auto-installed; crashes are
   restarted within limits (spec D4–D5; ADR-028).
4. **The terminal is real.** xterm.js renders PTYs owned by the kernel:
   multiple sessions, shell selection, resize, bounded scrollback, and
   permission-checked `term.read` / `term.exec` for the agent (spec D6–D7;
   ADR-029).
5. **Git is integrated.** Status, diff, stage/unstage, commit, branches, log,
   and blame through `git2` with a CLI fallback; agent/Composer commits carry
   trailers; **shadow git stays separate** from the user's repository until an
   explicit commit; remote operations are a separate, off-by-default
   capability (spec D8–D10; ADR-030).
6. **The agent lives in the editor.** Inline ghost text (opt-in) and the
   Composer produce reviewable diffs; applying is per-hunk/file/all and goes
   through backup + shadow git (spec D11–D12; ADR-032).
7. **Chat and IDE share one window.** A persisted layout tree renders chat and
   IDE side by side, resizable from the keyboard; selections and files can be
   attached to the conversation (spec D14; ADR-033).
8. **Nothing is trusted by default.** Untrusted projects, disabled
   capabilities, and permission-denied operations all fail with structured,
   side-effect-free errors.
9. **Quality is enforced.** Every task ships tests; CI is green on Windows,
   macOS, and Linux before merge; no language server, shell, git remote, or
   network is required by CI.

## Scope

### In scope

- New crates: `fergents-ide` (projects + files), `fergents-lsp`,
  `fergents-terminal`, `fergents-git`.
- New global tables: `projects`, `editor_layouts`, `lsp_servers`,
  `terminal_sessions`; `runs.project_id` (spec Appendix B).
- Phase 3 capabilities: `ide`, `lsp`, `terminal`, `git`, `git-remote`,
  `inline-ai`, `composer` (spec Appendix D).
- The Phase 3 protocol surface (spec Appendix A).
- The desktop IDE area (Monaco, problems, terminal, source control, diff
  preview, inline AI, Composer, split view) — **only after the backend tasks
  (3.1–3.5) are merged**.
- Security: trust gate, PTY sandbox policy, LSP environment scrubbing, git
  permission categories, audit entries (spec Appendix E).

### Out of scope (later phases)

- Debugger/DAP, notebooks, extension marketplace, remote development,
  task/launch configurations, multi-window tear-off, semantic code graph.
- Plugin runtime (Phase 4); workflow engine (Phase 5); dashboards (Phase 6);
  sync & mobile (Phase 7); PR review (Phase 8).

## Constraints

- **Design system is law.** Any new UI uses only `style/` tokens/components;
  new tokens/components are added to `style/` first and mirrored in
  `patterns.html`.
- **Backend before UI.** No `apps/desktop` changes until 3.1–3.5 are merged.
- **Offline CI.** Tests use fixtures (fake LSP server, local shell, temp git
  repos); no downloads, no network.
- **Tests per task.** Rust tests for backend, Vitest for UI.
- **CI before merge.** `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --workspace`, offline test, `cargo build`; ESLint, Prettier,
  `tsc --noEmit`, `pnpm -r test`, `pnpm build`.
- **GitHub flow.** Branch → PR → green CI → merge, Conventional Commits.

## Key risks

| Risk | Mitigation |
|---|---|
| Untrusted code runs on open | Trust gate **before** any LSP/PTY/git hook; tested that nothing spawns while untrusted. |
| Losing user edits | `base_etag` conflicts, external-change banner, never silent overwrite. |
| Language servers hang or crash | Per-request timeouts, bounded restarts, explicit `failed` state. |
| PTY escaping the project | Sandbox policy + `cwd` pinning + scrubbed env + audit; `term.read` bounds. |
| Agent commits pollute history | Shadow git separation; user repo commits require approval; trailers identify agent commits. |
| Monaco bloats startup | Lazy chunk + workers; startup budget enforced by a Vitest/bundle check. |
| Shell/LSP unavailable in CI | Fixtures and skips; the suite must not depend on external binaries. |
| Protocol churn | Phase 3 commands are additive; Phase 1/2 contracts unchanged. |

## Deliverables

- `goal.md`, `tasks.md`, `checklist.md` (this phase).
- Merged PRs for 3.1 → 3.5 (backend), then the UI tasks.
- Working project layer, file operations, LSP, terminal, git, and the IDE UI.
- Updated docs where behaviour changes (`UI.md`, `ARCHITECTURE.md`,
  `SECURITY.md`, `STACK.md`) and new ADRs (ADR-027…ADR-033).
