# Phase 3 — Checklist

Normative checks derived from [spec.md](./spec.md). A box is ticked only when
the behaviour is implemented, tested, and merged.

## Project layer (3.1)

- [x] New crate `fergents-ide` with `ProjectService`
- [x] `migrations/global/0012_projects.sql` (`projects`, `editor_layouts`)
- [x] `capability.ide` registered (default on, hot, no dependencies)
- [x] `project.open` detects the VCS root and builds an ignore-aware index
- [x] A new project is **untrusted**; nothing executable spawns until trusted
- [x] `project.close/list/get/update/trust` + `project.*` events
- [x] `project.files` fuzzy match and `project.search` bounded text search
- [x] Path containment rejects `../` and symlink escapes
- [x] `project.*` refused with `capability_disabled` when the capability is off
- [x] Tests + wired IPC tests green

## File operations (3.2)

- [x] `file.read/write/create/mkdir/rename/delete/list/stat` in the kernel
- [x] `etag = sha256(content)`; stale `base_etag` → `file_conflict`, no write
- [x] External change → `file.changed`, editor banner (no silent overwrite)
- [x] Every mutation snapshotted (backup) and restorable
- [x] Binary/large file guards (`truncated`, read-only)
- [x] Untrusted project → `project_untrusted`
- [x] Agent `file.read`/`file.write` tools are project-scoped and permissioned
- [x] Tests green

## LSP (3.3)

- [x] New crate `fergents-lsp` with `Content-Length` framing over stdio
- [x] `migrations/global/0013_lsp.sql` (`lsp_servers`)
- [x] `capability.lsp`; no server process when the capability is off
- [x] Lifecycle `initialize → initialized → shutdown → exit` with timeouts
- [x] Restart policy: max 3 in 5 minutes, then `failed`
- [x] Incremental, versioned document sync (`didOpen/didChange/didSave/didClose`)
- [x] completion, hover, definition, references, rename, symbols, format,
      code actions, signature help
- [x] Diagnostics stored, emitted, and listed; stale on server death
- [x] Discovery from `PATH`/registry only; `not_installed` reported, never
      auto-installed
- [x] Env scrubbed, `cwd` pinned, untrusted project refused
- [x] Tests (incl. fake stdio server fixture) green

## Terminal (3.4)

- [x] New crate `fergents-terminal` with `portable-pty`
- [x] `migrations/global/0014_terminal.sql` (`terminal_sessions`)
- [x] `capability.terminal`; no PTY when off
- [x] `term.open/write/resize/close/list` + batched `term.output`
- [x] Bounded scrollback ring (1 MB) and bounded, redacted `term.read`
- [x] `term.exec` visible, permission-checked, audited
- [x] Env scrubbed; `cwd` inside the project; untrusted project refused
- [x] Sessions marked closed on the next boot
- [x] Tests green (skipped only when no local shell exists)

## Git (3.5)

- [x] New crate `fergents-git` (`git2`, vendored)
- [x] `capability.git` and `capability.git-remote` (off by default)
- [x] status, diff, stage, unstage, discard, commit, branches, log, show,
      blame, conflicts/resolve
- [x] Discard is reversible (backup first)
- [x] Agent/Composer commits carry `Fergents-Agent` / `Fergents-Session`
- [x] Shadow git never merged into the user repository without approval
- [x] Remote ops gated by capability + `git.remote`; no force-push; hooks run
- [x] Tests on temp repositories green

## Desktop UI (3.6–3.16)

- [x] Monaco lazy chunk; startup budget unchanged
- [x] Tabs (dirty/preview/pinned), breadcrumb, outline, find/replace
- [x] Save/conflict banner; external change handled
- [x] Problems panel grouped and jumpable
- [x] Terminal tabs/split/shell selector; live session reconnect
- [x] Source control: status/diff/stage/commit/branches/log/blame/conflicts
- [x] Diff preview with per-hunk/file/all approval and line stats
- [x] Inline AI ghost text (opt-in), Tab/Esc, quick actions, suppression rules
- [x] Composer proposal → approve subset → reversible apply; reject writes
      nothing
- [x] Agent mode: spec/plan/tasks tabs, approval cards, `@file`/`@selection`
- [x] Chat + IDE split persisted, resizable by keyboard, restores on restart
- [x] Command palette and shortcuts (`Ctrl+Tab`, `Ctrl+W`, `Ctrl+\`)
- [x] Settings → IDE (servers, shells, git remote, editor preferences)
- [x] All UI built from `style/` tokens; accessible per `UI.md` §19–§20

## Release (3.17)

- [x] ADR-027 … ADR-033 recorded
- [x] `UI.md` §8, `ARCHITECTURE.md`, `SECURITY.md` §22, `STACK.md` §14 updated
- [x] CHANGELOG + README status
- [x] End-to-end test: open → trust → edit → diagnostics → terminal → agent
      edit → approve → commit
- [x] All CI gates green on Windows, macOS, Linux
- [x] Tag `v0.3.0-phase3`
