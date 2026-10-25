# Phase 3 — Checklist

Normative checks derived from [spec.md](./spec.md). A box is ticked only when
the behaviour is implemented, tested, and merged.

## Project layer (3.1)

- [ ] New crate `fergents-ide` with `ProjectService`
- [ ] `migrations/global/0012_projects.sql` (`projects`, `editor_layouts`)
- [ ] `capability.ide` registered (default on, hot, no dependencies)
- [ ] `project.open` detects the VCS root and builds an ignore-aware index
- [ ] A new project is **untrusted**; nothing executable spawns until trusted
- [ ] `project.close/list/get/update/trust` + `project.*` events
- [ ] `project.files` fuzzy match and `project.search` bounded text search
- [ ] Path containment rejects `../` and symlink escapes
- [ ] `project.*` refused with `capability_disabled` when the capability is off
- [ ] Tests + wired IPC tests green

## File operations (3.2)

- [ ] `file.read/write/create/mkdir/rename/delete/list/stat` in the kernel
- [ ] `etag = sha256(content)`; stale `base_etag` → `file_conflict`, no write
- [ ] External change → `file.changed`, editor banner (no silent overwrite)
- [ ] Every mutation snapshotted (backup) and restorable
- [ ] Binary/large file guards (`truncated`, read-only)
- [ ] Untrusted project → `project_untrusted`
- [ ] Agent `file.read`/`file.write` tools are project-scoped and permissioned
- [ ] Tests green

## LSP (3.3)

- [ ] New crate `fergents-lsp` with `Content-Length` framing over stdio
- [ ] `migrations/global/0013_lsp.sql` (`lsp_servers`)
- [ ] `capability.lsp`; no server process when the capability is off
- [ ] Lifecycle `initialize → initialized → shutdown → exit` with timeouts
- [ ] Restart policy: max 3 in 5 minutes, then `failed`
- [ ] Incremental, versioned document sync (`didOpen/didChange/didSave/didClose`)
- [ ] completion, hover, definition, references, rename, symbols, format,
      code actions, signature help
- [ ] Diagnostics stored, emitted, and listed; stale on server death
- [ ] Discovery from `PATH`/registry only; `not_installed` reported, never
      auto-installed
- [ ] Env scrubbed, `cwd` pinned, untrusted project refused
- [ ] Tests (incl. fake stdio server fixture) green

## Terminal (3.4)

- [ ] New crate `fergents-terminal` with `portable-pty`
- [ ] `migrations/global/0014_terminal.sql` (`terminal_sessions`)
- [ ] `capability.terminal`; no PTY when off
- [ ] `term.open/write/resize/close/list` + batched `term.output`
- [ ] Bounded scrollback ring (1 MB) and bounded, redacted `term.read`
- [ ] `term.exec` visible, permission-checked, audited
- [ ] Env scrubbed; `cwd` inside the project; untrusted project refused
- [ ] Sessions marked closed on the next boot
- [ ] Tests green (skipped only when no local shell exists)

## Git (3.5)

- [ ] New crate `fergents-git` (`git2`, vendored)
- [ ] `capability.git` and `capability.git-remote` (off by default)
- [ ] status, diff, stage, unstage, discard, commit, branches, log, show,
      blame, conflicts/resolve
- [ ] Discard is reversible (backup first)
- [ ] Agent/Composer commits carry `Fergents-Agent` / `Fergents-Session`
- [ ] Shadow git never merged into the user repository without approval
- [ ] Remote ops gated by capability + `git.remote`; no force-push; hooks run
- [ ] Tests on temp repositories green

## Desktop UI (3.6–3.16)

- [ ] Monaco lazy chunk; startup budget unchanged
- [ ] Tabs (dirty/preview/pinned), breadcrumb, outline, find/replace
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
- [ ] All UI built from `style/` tokens; accessible per `UI.md` §19–§20

## Release (3.17)

- [ ] ADR-027 … ADR-033 recorded
- [ ] `UI.md` §8, `ARCHITECTURE.md`, `SECURITY.md` §22, `STACK.md` §14 updated
- [ ] CHANGELOG + README status
- [ ] End-to-end test: open → trust → edit → diagnostics → terminal → agent
      edit → approve → commit
- [ ] All CI gates green on Windows, macOS, Linux
- [ ] Tag `v0.3.0-phase3`
