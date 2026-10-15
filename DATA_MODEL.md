# Data Model — Phase 0 Global Store

> The Phase 0 global SQLite schema, storage engines, migrations, and audit-log
> tamper-evidence.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md` §7 (storage), `SECURITY.md` §16 (audit
log), `ROADMAP.md` §4 (Phase 0).

This document describes the schema shipped in
`migrations/global/0001_initial.sql` (the Phase 0 global store). Per-brain
stores are deferred to Phase 1 (see below).

---

## 1. Storage engines

- **Global store:** a single SQLite database at `~/.fergents/global.sqlite`.
  It holds cross-brain state: the event log, the audit log, sessions, the
  registered brains and projects, LLM request accounting, and settings
  (ARCHITECTURE.md §7.1).
- **Per-brain stores (Phase 1):** each brain owns an `index.sqlite` (documents,
  chunks, FTS5, `sqlite-vec` embeddings, tags, links) and a `graph.db`
  (CozoDB relations). These are **deferred to Phase 1**; the `migrations/brain/`
  directory is reserved for them.
- **File system:** markdown vaults and large binaries live on disk; the
  databases hold indexes and metadata only.

Every connection runs in **WAL mode** with **foreign keys enabled**. The kernel
opens a pool of N read connections, `N = min(4, cpu_count)`, and a single
writer task owns all writes (serialized through an mpsc channel) so that no
concurrent writers exist.

---

## 2. Global schema

Column types, nullability, defaults, and indexes below match
`migrations/global/0001_initial.sql` exactly. Columns not marked nullable are
`NOT NULL`; `PRIMARY KEY` columns are `NOT NULL`. `TEXT (JSON)` denotes a
JSON-encoded string; `REAL` denotes an IEEE-754 double. Where a column has a
`DEFAULT`, it is noted.

### 2.1 `schema_version`

Tracks applied migrations.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `version` | INTEGER | no | Primary key; migration number. |
| `description` | TEXT | no | Human-readable description of the migration. |
| `checksum` | TEXT | no | Checksum of the migration file. |
| `applied_at` | INTEGER | no | Unix epoch milliseconds when applied. |

### 2.2 `events`

The append-only event log. Every event emitted on the bus is persisted here.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `id` | TEXT | no | Primary key; monotonic ULID. |
| `ts` | INTEGER | no | Unix epoch milliseconds. |
| `kind` | TEXT | no | Event kind (e.g. `kernel.started`). |
| `source` | TEXT | no | Emitting component. |
| `correlation_id` | TEXT | yes | Groups related events. |
| `brain_id` | TEXT | yes | Brain scope, if any. |
| `project_id` | TEXT | yes | Project scope, if any. |
| `payload` | TEXT (JSON) | no | Event body. `DEFAULT '{}'`. |
| `version` | INTEGER | no | Event schema version. `DEFAULT 1`. |

Indexes:

- `(kind, ts)`
- `(correlation_id)`
- `(ts)`

### 2.3 `audit_log`

The append-only, tamper-evident audit log (SECURITY.md §16.3). Phase 0
implements append and chain verification; permission writes that populate it
arrive in Phase 2.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `id` | TEXT | no | Primary key; ULID. |
| `ts` | INTEGER | no | Unix epoch milliseconds. |
| `actor_type` | TEXT | no | e.g. `user`, `agent`, `kernel`. |
| `actor_id` | TEXT | no | Actor identifier. |
| `session_id` | TEXT | yes | Originating session. |
| `action` | TEXT | no | e.g. `fs.write`. |
| `target` | TEXT | yes | Action target (path, resource). |
| `args_json` | TEXT (JSON) | no | Action arguments. `DEFAULT '{}'`. |
| `result` | TEXT | no | One of `allowed`, `denied`, `error`. |
| `reason` | TEXT | yes | Policy rule or explanation. |
| `correlation_id` | TEXT | yes | Groups related entries. |
| `reversible` | INTEGER | no | Boolean (0/1): whether an undo path exists. `DEFAULT 0`. |
| `revert_ref` | TEXT | yes | Reference to the revert artifact. |
| `prev_hash` | TEXT | no | Hash of the previous entry. |
| `hash` | TEXT | no | Hash of this entry (see §4). |

Indexes:

- `(actor_type, actor_id, ts)`
- `(action, ts)`
- `(ts)`

### 2.4 `sessions`

Client sessions attached to the kernel.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `id` | TEXT | no | Primary key; ULID. |
| `client_kind` | TEXT | no | e.g. `desktop`, `cli`. |
| `client_version` | TEXT | yes | Client version string. |
| `protocol_version` | INTEGER | no | Negotiated protocol version. |
| `capabilities` | TEXT (JSON) | no | Declared client capabilities. `DEFAULT '{}'`. |
| `started_at` | INTEGER | no | Unix epoch milliseconds. |
| `last_seen_at` | INTEGER | no | Unix epoch milliseconds. |
| `closed_at` | INTEGER | yes | Set when the session ends. |

On kernel start after an unclean stop, sessions without a `closed_at` are
reconciled as closed.

### 2.5 `brains`

Registered brains (empty in Phase 0).

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `brain_id` | TEXT | no | Primary key; UUID from `brain.toml`. |
| `name` | TEXT | no | Display name. |
| `path` | TEXT | no | Unique; brain folder path. |
| `role` | TEXT | yes | Optional role. |
| `schema_version` | INTEGER | no | Brain schema version. `DEFAULT 0`. |
| `created_at` | INTEGER | no | Unix epoch milliseconds. |
| `last_opened_at` | INTEGER | yes | Unix epoch milliseconds. |

Indexes:

- unique on `(path)`

### 2.6 `projects`

Workspaces that link one or more brains.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `project_id` | TEXT | no | Primary key. |
| `name` | TEXT | no | Display name. |
| `path` | TEXT | no | Project folder path. |
| `created_at` | INTEGER | no | Unix epoch milliseconds. |

### 2.7 `project_brains`

Association between projects and brains.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `project_id` | TEXT | no | References `projects(project_id)` `ON DELETE CASCADE`. |
| `brain_id` | TEXT | no | References `brains(brain_id)` `ON DELETE CASCADE`. |
| `role` | TEXT | no | One of `primary`, `read-only`, `scratch`. `DEFAULT 'primary'`. |

- Primary key: `(project_id, brain_id)`.

### 2.8 `llm_requests`

Per-request LLM accounting. Phase 0 ships the table; the writer arrives in
Phase 2/6.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `id` | TEXT | no | Primary key; ULID. |
| `ts` | INTEGER | no | Unix epoch milliseconds. |
| `provider` | TEXT | yes | Provider name. |
| `model` | TEXT | yes | Model identifier. |
| `task` | TEXT | yes | Routing task. |
| `input_tokens` | INTEGER | no | Input token count. `DEFAULT 0`. |
| `output_tokens` | INTEGER | no | Output token count. `DEFAULT 0`. |
| `cached_tokens` | INTEGER | no | Cached token count. `DEFAULT 0`. |
| `cost_usd` | REAL | no | Estimated cost in USD. `DEFAULT 0`. |
| `latency_ms` | INTEGER | yes | Wall-clock latency. |
| `agent_id` | TEXT | yes | Originating agent. |
| `session_id` | TEXT | yes | Originating session. |
| `status` | TEXT | no | Request outcome. `DEFAULT 'ok'`. |

Indexes:

- `(ts)`
- `(provider, model, ts)`

### 2.9 `settings`

Key/value application settings.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `key` | TEXT | no | Primary key. |
| `value_json` | TEXT (JSON) | no | JSON-encoded value. |
| `updated_at` | INTEGER | no | Unix epoch milliseconds. |

---

## 3. Migrations

- Migrations are **numbered SQL files** under `migrations/` (Phase 0:
  `migrations/global/0001_initial.sql`; `migrations/brain/` is reserved for
  Phase 1).
- The kernel applies pending migrations on store open and records each applied
  version in `schema_version`.
- Applying migrations is **idempotent**: an already-migrated store reopens
  without re-applying any migration.
- **Newer-schema refusal:** if the on-disk `schema_version` is greater than the
  kernel's known maximum, the kernel refuses to open the store and returns a
  clear error rather than risking corruption.

---

## 4. Audit log tamper-evidence

The audit log is **append-only** and **tamper-evident**. Each entry stores the
previous entry's hash and its own hash, forming a verifiable chain
(SECURITY.md §16.6):

```
hash(n) = sha256(prev_hash || canonical_entry(n))
```

- `canonical_entry(n)` is the deterministic serialization of entry `n`'s
  fields.
- The **genesis** entry (the first in the chain) uses a defined all-zero
  `prev_hash`: 64 zero hex characters
  (`0000000000000000000000000000000000000000000000000000000000000000`).
- Chain verification recomputes each hash in order. If any historical entry's
  fields are modified out of band, verification fails and reports the first
  mismatching `id`.

---

*End of `DATA_MODEL.md`.*
