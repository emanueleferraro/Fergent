# Phase 3 — Tasks

Each task is one branch → one PR → green CI → merge (Conventional Commits).
Backend tasks (3.1–3.5) must be merged before any `apps/desktop` work begins.

Legend: **Dep** = depends on · **🔒** = gated on another task merging first.

---

## 3.1 — Project layer (`crates/ide`)

**Dep:** — · **Blocks:** 3.2–3.5, all IDE UI.

**Deliverables**
- New crate `fergents-ide` with `ProjectService`.
- `migrations/global/0012_projects.sql` (`projects`, `editor_layouts`).
- Capability `capability.ide` in the catalogue (default on, hot).
- Kernel wiring + `project.*` commands + capability gate.

**Steps**
1. `Project` model: id, path, name, `vcs` (`git|none`), `brain_id`, `trust`
   (`untrusted|trusted`), settings, timestamps.
2. `open`: canonicalise the path, detect the git root, build the file index
   (respecting `.gitignore` / `.fergentsignore`), persist as **untrusted**,
   emit `project.opened`.
3. `close`, `list`, `get`, `update` (name / brain_id / settings), `trust`
   (persists, emits `project.trusted` / `project.untrusted`).
4. `files(query, limit)`: fuzzy subsequence match over the cached index.
5. `search(query, limit)`: bounded text search with ignore rules and
   binary/large-file guards.
6. `resolve(project, rel)` + `require_trusted(project)`; a path that escapes
   the root (including through symlinks) is rejected.

**Tests**
- Open detects the git root and the project name; duplicates are rejected.
- Ignore rules exclude `node_modules/`-style paths; `.fergentsignore` wins.
- Fuzzy file list ranks a prefix match first; search caps results.
- Trust transitions persist across a restart; `require_trusted` returns a
  structured `project_untrusted`.
- Containment rejects `../escape` and symlink escapes.
- API: `project.*` is gated by `capability.ide`; round-trip and wired tests.

**Acceptance**
- A folder can be opened, trusted, listed, and searched; untrusted projects are
  visibly restricted. CI green.

---

## 3.2 — Kernel file operations (etags, conflicts)

**Dep:** 3.1.

**Deliverables**
- `FileService` in `fergents-ide`: `read`, `write`, `create`, `mkdir`,
  `rename`, `delete`, `list`, `stat`.
- `file.*` protocol commands + capability gate.
- Backups on every mutation (via `fergents-reversibility`).
- External-change watcher → `file.changed` events.

**Steps**
1. `etag = sha256(content)`; `read` returns `content`, `etag`, `size`,
   `language_id`, `truncated`.
2. `write(rel, content, base_etag)`: mismatch → `file_conflict` (no write);
   match/absent → snapshot, atomic write, new etag, `file.saved`.
3. `create`, `mkdir`, `rename`, `delete`: contained, reversible, audited.
4. Binary/large-file detection for `read` (> 5 MB → `truncated`).
5. Debounced watcher per open project publishing `file.changed`.
6. `file.read`/`file.write` also exposed to the agent as tools, scoped to the
   project and permission-checked.

**Tests**
- Round-trip write/read; stale etag → `file_conflict` and content unchanged.
- Create/rename/delete restore from backups.
- Containment on every operation; untrusted project rejected.
- Watcher emits `file.changed` for an external write (bounded wait).
- Agent tool path is permission-gated.

**Acceptance**
- Concurrent edits are safe: no lost writes, conflicts are explicit. CI green.

---

## 3.3 — LSP client (`crates/lsp`)

**Dep:** 3.1.

**Deliverables**
- New crate `fergents-lsp`: framed JSON-RPC client, server registry, lifecycle.
- `migrations/global/0013_lsp.sql` (`lsp_servers`).
- Capability `capability.lsp`; `lsp.*` commands and `lsp.*` events.

**Steps**
1. `Content-Length` codec over stdio; request/response correlation by id;
   per-request timeout.
2. Session lifecycle: `initialize` (rootUri, workspace folders, client
   capabilities), `initialized`, `shutdown`, `exit`; graceful then kill.
3. Crash policy: restart with backoff, max 3 in 5 minutes, then `failed`.
4. Document sync: `didOpen`, incremental `didChange` (versioned), `didSave`,
   `didClose`.
5. Requests: completion (+resolve), hover, definition, references, rename,
   symbols, workspace symbols, formatting, code actions, signature help.
6. Diagnostics: push and pull, stored per project/file, emitted as
   `lsp.diagnostics`; `lsp.diagnostics.list`.
7. Discovery: `PATH` + registry; statuses `not_installed|starting|ready|
   failed|stopped`; env scrubbed of secrets; `cwd` pinned to the root.

**Tests**
- Codec round-trip (framing, partial reads, unicode lengths).
- Lifecycle and correlation against an in-repo fake stdio server fixture.
- Timeout, crash → restart count → `failed`.
- Incremental sync sends increasing versions.
- Diagnostics are stored and emitted; guarded by the capability.
- API: `lsp.server.*`, `lsp.document.*`, `lsp.completion` wired tests.

**Acceptance**
- A real server can be attached and queried; a fake server exercises CI. Green.

---

## 3.4 — PTY terminal backend (`crates/terminal`)

**Dep:** 3.1.

**Deliverables**
- New crate `fergents-terminal` using `portable-pty`.
- `migrations/global/0014_terminal.sql` (`terminal_sessions`).
- Capability `capability.terminal`; `term.*` commands and events.

**Steps**
1. `open(project, shell?, cwd?, cols, rows)`: trusted-only, shell discovery,
   spawn PTY, register session, emit `term.opened`.
2. Reader task → bounded ring buffer (1 MB) → batched `term.output` events.
3. `write`, `resize`, `close`, `list`; exit monitoring → `term.exit`.
4. `read(lines|max_bytes)`: bounded, redacted tail for the agent.
5. `exec(command)`: visible session, `shell.exec` policy, audited.
6. Env scrubbing and `cwd` containment; state persisted, marked closed on boot.

**Tests**
- Open/write/read/close against a local shell (skipped if none is available).
- Ring buffer drops oldest data; `read` is bounded.
- Untrusted project and disabled capability are rejected without spawning.
- `term.exec` honours the permission policy and is audited.

**Acceptance**
- Interactive terminal works end to end; the agent can read bounded output.

---

## 3.5 — Git integration (`crates/git`)

**Dep:** 3.1.

**Deliverables**
- New crate `fergents-git` (`git2`, CLI fallback).
- Capability `capability.git` + `capability.git-remote` (off).
- `git.*` commands and events; agent commit trailers.

**Steps**
1. Repo open/reuse per project; status (branch, ahead/behind, entries).
2. Diff (worktree/staged/file), hunks; log, show, blame.
3. Stage/unstage (file or hunk), discard (backup first), commit.
4. Branch list/create/switch; conflict detection + `git resolve` strategies.
5. Remote ops (fetch/pull/push) behind `capability.git-remote` +
   `git.remote`; never force-push; hooks honoured.
6. Trailer injection for agent/Composer commits; shadow-git separation and
   `git.status.changed` events.

**Tests**
- Status/diff/stage/commit on temp repositories (vendored libgit2).
- Trailers are present only for agent-originated commits.
- Discard is reversible from a backup.
- Remote commands are refused unless the capability and permission allow it.
- API: `git.*` wired tests + capability gate.

**Acceptance**
- A full review → stage → commit cycle works offline. Green.

---

## 3.6 — Desktop: project shell, file tree, split view

**Dep:** 3.1, 3.5. **🔒** backend merged.

Project switcher, project file tree with git badges, separate brain tree, tab
strip, and the persisted chat+IDE split (default: chat left, IDE right).
**Tests (Vitest):** layout restore/fallback; tree filter; splitter keyboard.

---

## 3.7 — Desktop: Monaco editor

**Dep:** 3.6. Monaco lazy chunk + workers, tabs/dirty/preview, breadcrumb,
outline, find/replace, format on save, save/conflict banner.
**Tests (Vitest):** adapter contract, dirty state, conflict banner, large-file
guard.

---

## 3.8 — Desktop: problems + LSP UI

**Dep:** 3.7. Problems panel grouped by file/severity, jump-to, inline
squiggles, hover, peek definition, rename, quick fixes; server status manager.
**Tests (Vitest):** grouping, jump, status states.

---

## 3.9 — Desktop: terminal

**Dep:** 3.6. xterm tabs/split, shell selector, resize, reconnect to a live
session, agent-terminal indicator.
**Tests (Vitest):** tab lifecycle, adapter writes, exit rendering.

---

## 3.10 — Desktop: source control

**Dep:** 3.6. Status list, inline diff, stage/unstage, commit box, branches,
log, blame, basic conflict resolution.
**Tests (Vitest):** stage/commit payloads, conflict banner, log rendering.

---

## 3.11 — Desktop: diff preview + per-hunk approval

**Dep:** 3.10. Monaco diff editor, per-hunk accept/reject, accept/reject all,
line stats; used by agent edits, Composer, and inline actions.
**Tests (Vitest):** hunk selection → apply payload; stats rendering.

---

## 3.12 — Desktop: inline AI (ghost text)

**Dep:** 3.7, 3.11. Debounced completions, Tab/Esc, quick actions (`Ctrl+.`),
suppression rules, budget display.
**Tests (Vitest):** accept/dismiss, cancellation, suppression.

---

## 3.13 — Desktop: Composer

**Dep:** 3.11. Prompt + explicit context, proposal list per file, per-hunk/file/
all approval, apply/reject, run status.
**Tests (Vitest):** proposal → approve subset → apply payload; reject writes
nothing.

---

## 3.14 — Desktop: agent mode in the IDE

**Dep:** 3.11, 3.13. Spec/plan/tasks artifacts as tabs, approval cards, run
panel; `@file`/`@selection` from the editor into chat.
**Tests (Vitest):** artifact tabs, selection attachment payload.

---

## 3.15 — Desktop: command palette + shortcuts

**Dep:** 3.7–3.14. IDE commands in the palette, tab/split shortcuts, "Add to
chat", format/save/go-to-definition bindings.
**Tests (Vitest):** command registration and dispatch.

---

## 3.16 — Settings → IDE

**Dep:** 3.3, 3.5, 3.9. Language-server registry UI, shell selection, git
remote toggle/credentials, editor preferences (format on save, size limits).
**Tests (Vitest):** server add/remove/status, shell preference persistence.

---

## 3.17 — Hardening, docs, ADRs, release

**Dep:** all. ADR-027…ADR-033, `UI.md` §8 update, `ARCHITECTURE.md`,
`SECURITY.md` categories, `STACK.md` §14, CHANGELOG, README status, an
end-to-end test (open project → trust → edit → diagnostics → terminal → agent
edit → approve → commit), tag `v0.3.0-phase3`.
**Tests:** the e2e Rust test; full suite; acceptance checklist.

---

## Suggested sequence

1. 3.1 → 3.2 (projects and files are the substrate for the rest).
2. 3.3, 3.4, 3.5 in parallel (independent crates, shared `project_id`).
3. 3.6 → 3.7 → 3.8/3.9/3.10 → 3.11 → 3.12/3.13 → 3.14 → 3.15/3.16.
4. 3.17 last.
