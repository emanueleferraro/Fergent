# Data Model — Global and Per-Brain Stores

> The global SQLite schema, the per-brain SQLite index schema, the storage
> engines, migrations, and audit-log tamper-evidence.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md` §7 (storage), `SECURITY.md` §16 (audit
log), `ROADMAP.md` §4 (Phase 0) and §5 (Phase 1).

This document describes the schemas shipped in
`migrations/global/0001_initial.sql` (the global store, §2) and
`migrations/brain/0001_initial.sql` plus `0002_chunks_fts_external_content.sql`
(the per-brain index store, §3).

---

## 1. Storage engines

- **Global store:** a single SQLite database at `~/.fergents/global.sqlite`.
  It holds cross-brain state: the event log, the audit log, sessions, the
  registered brains and projects, LLM request accounting, and settings
  (ARCHITECTURE.md §7.1).
- **Per-brain stores:** each brain owns an `index.sqlite` (documents, chunks,
  FTS5, `sqlite-vec` embeddings, tags, links, entities) and a `graph.db`
  (CozoDB relations). The SQLite index schema is documented in §3; the
  `migrations/brain/` directory holds its numbered migrations.
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
| `hash` | TEXT | no | Hash of this entry (see §5). |

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

## 3. Per-brain schema

Each brain owns a SQLite index store (`index.sqlite`) built by
`migrations/brain/0001_initial.sql` (spec Appendix B, normative) and
`migrations/brain/0002_chunks_fts_external_content.sql` (decision D7). Column
types, nullability, defaults, and indexes below match those files exactly.
`TEXT (JSON)` denotes a JSON-encoded string; `REAL` denotes an IEEE-754 double.
The graph itself lives in CozoDB (`graph.db`); `entities`, `relations`, and
`entity_mentions` below are the **SQLite mirror** of the graph (GRAPH.md §5.3).

### 3.1 `schema_version`

Identical in shape to the global store's table (§2.1): `version` (INTEGER,
primary key), `description` (TEXT), `checksum` (TEXT), `applied_at` (INTEGER).

### 3.2 `documents`

One row per indexed file in the vault.

| Column           | Type         | Nullable | Notes                                 |
| ---------------- | ------------ | -------- | ------------------------------------- |
| `id`             | TEXT         | no       | Primary key. Stable document id.      |
| `path`           | TEXT         | no       | Vault-relative path; `UNIQUE`.        |
| `kind`           | TEXT         | no       | `note`, `attachment`, `code`, ...     |
| `title`          | TEXT         | no       | Display title.                        |
| `mime`           | TEXT         | no       | Detected MIME type.                   |
| `size`           | INTEGER      | no       | Size in bytes.                        |
| `hash`           | TEXT         | no       | Content hash (change detection).      |
| `frontmatter`    | TEXT (JSON)  | yes      | Parsed YAML frontmatter.              |
| `word_count`     | INTEGER      | no       | `DEFAULT 0`.                          |
| `reading_time_s` | INTEGER      | no       | `DEFAULT 0`.                          |
| `created_at`     | INTEGER      | no       | Unix epoch milliseconds.              |
| `updated_at`     | INTEGER      | no       | Unix epoch milliseconds.              |
| `indexed_at`     | INTEGER      | no       | Unix epoch milliseconds.              |
| `deleted_at`     | INTEGER      | yes      | Tombstone time for soft deletes.      |

Indexes: `(path)`, `(kind)`, `(deleted_at)`.

### 3.3 `notes_meta`

Note-specific metadata, one row per `kind = note` document.

| Column        | Type        | Nullable | Notes                     |
| ------------- | ----------- | -------- | ------------------------- |
| `document_id` | TEXT        | no       | Primary key; → `documents`. |
| `aliases`     | TEXT (JSON) | no       | `DEFAULT '[]'`.           |
| `cssclasses`  | TEXT (JSON) | no       | `DEFAULT '[]'`.           |
| `publish`     | INTEGER     | no       | `DEFAULT 0`.              |

### 3.4 `chunks`

The retrieval unit: a token-bounded slice of a document.

| Column         | Type    | Nullable | Notes                                    |
| -------------- | ------- | -------- | ---------------------------------------- |
| `id`           | TEXT    | no       | Primary key.                             |
| `document_id`  | TEXT    | no       | → `documents`.                           |
| `ordinal`      | INTEGER | no       | Position within the document.            |
| `content`      | TEXT    | no       | Chunk text.                              |
| `token_count`  | INTEGER | no       | Token count (cl100k_base).               |
| `start_offset` | INTEGER | no       | Byte offset in the source document.      |
| `end_offset`   | INTEGER | no       | Byte offset in the source document.      |
| `heading_path` | TEXT    | yes      | Breadcrumb of enclosing headings.        |
| `language`     | TEXT    | yes      | For code chunks.                         |
| `symbol`       | TEXT    | yes      | For code chunks.                         |

Index: `(document_id, ordinal)`.

### 3.5 `chunks_fts`

FTS5 full-text index over `chunks` (SEARCH.md §7). Migration 0002 recreates it
as an **external-content** table so it shares the `chunks` rows instead of
duplicating the text:

```sql
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content, heading_path,
    content = 'chunks', content_rowid = 'rowid',
    tokenize = 'unicode61 remove_diacritics 2'
);
```

It is maintained by the indexing pipeline (insert on chunk write, `'delete'` on
removal); migration 0002 runs a `rebuild` so brains indexed under 0001 keep a
usable index. The tokenizer is fixed by decision ADR-019.

### 3.6 `vec_chunks`

The `sqlite-vec` dense-vector index (ADR-013).

| Column      | Type        | Nullable | Notes                       |
| ----------- | ----------- | -------- | --------------------------- |
| `chunk_id`  | TEXT        | no       | Primary key; → `chunks.id`. |
| `embedding` | float[1024] | no       | BGE-M3 dimension (spec D5). |

### 3.7 `embedding_meta`

Per-chunk embedding provenance.

| Column         | Type        | Nullable | Notes                              |
| -------------- | ----------- | -------- | ---------------------------------- |
| `chunk_id`     | TEXT        | no       | Primary key; → `chunks`.           |
| `model`        | TEXT        | no       | Embedding model id.                |
| `dim`          | INTEGER     | no       | Vector dimension.                  |
| `content_hash` | TEXT        | no       | Hash of the embedded text.         |
| `sparse`       | TEXT (JSON) | yes      | Sparse (lexical) weights, if kept. |
| `created_at`   | INTEGER     | no       | Unix epoch milliseconds.           |

Index: `(content_hash)`.

### 3.8 `tags` and `document_tags`

| Column   | Type | Nullable | Notes                          |
| -------- | ---- | -------- | ------------------------------ |
| `id`     | TEXT | no       | `tags` primary key.            |
| `name`   | TEXT | no       | `UNIQUE`.                      |
| `parent` | TEXT | yes      | Hierarchical tag parent.       |

`document_tags(document_id, tag_id)` is a join table with a composite primary
key and foreign keys to `documents` and `tags`.

### 3.9 `links`

The document graph: resolved and unresolved links between documents
(SEARCH.md §8).

| Column            | Type    | Nullable | Notes                                   |
| ----------------- | ------- | -------- | --------------------------------------- |
| `id`              | TEXT    | no       | Primary key.                            |
| `src_document_id` | TEXT    | no       | → `documents`.                          |
| `dst_document_id` | TEXT    | yes      | → `documents`; null when unresolved.    |
| `dst_path`        | TEXT    | yes      | Target path when unresolved.            |
| `dst_heading`     | TEXT    | yes      | Heading anchor.                         |
| `dst_block`       | TEXT    | yes      | Block anchor.                           |
| `display_text`    | TEXT    | yes      | Alias used at the link site.            |
| `anchor`          | TEXT    | yes      | URL fragment.                           |
| `kind`            | TEXT    | no       | `wikilink`, `markdown`, `embed`, ...    |
| `unresolved`      | INTEGER | no       | `DEFAULT 0`; `1` = unresolved link.     |

Indexes: `(src_document_id)`, `(dst_document_id)`, `(dst_path)`.

### 3.10 `entities`

The SQLite mirror of CozoDB entities (GRAPH.md §5.3).

| Column            | Type        | Nullable | Notes                          |
| ----------------- | ----------- | -------- | ------------------------------ |
| `id`              | TEXT        | no       | Primary key.                   |
| `name`            | TEXT        | no       | Surface form.                  |
| `type`            | TEXT        | no       | `person`, `org`, `place`, ...  |
| `canonical_name`  | TEXT        | yes      | Merged canonical form.         |
| `normalized_name` | TEXT        | no       | Normalized lookup key.         |
| `metadata`        | TEXT (JSON) | yes      | Extractor-specific metadata.   |
| `first_seen_at`   | INTEGER     | no       | Unix epoch milliseconds.       |
| `last_seen_at`    | INTEGER     | no       | Unix epoch milliseconds.       |

Index: `(normalized_name, type)`.

### 3.11 `relations`

The SQLite mirror of CozoDB relations.

| Column       | Type        | Nullable | Notes                          |
| ------------ | ----------- | -------- | ------------------------------ |
| `id`         | TEXT        | no       | Primary key.                   |
| `src`        | TEXT        | no       | Source entity id.              |
| `dst`        | TEXT        | no       | Target entity id.              |
| `kind`       | TEXT        | no       | Relation type.                 |
| `weight`     | REAL        | no       | `DEFAULT 0`.                   |
| `confidence` | REAL        | no       | `DEFAULT 0`.                   |
| `metadata`   | TEXT (JSON) | yes      | Provenance metadata.           |
| `source_doc` | TEXT        | yes      | Document the relation came from.|
| `created_at` | INTEGER     | no       | Unix epoch milliseconds.       |

Indexes: `(src, kind)`, `(dst, kind)`.

### 3.12 `entity_mentions`

Where an entity occurs in the vault.

| Column         | Type    | Nullable | Notes                                   |
| -------------- | ------- | -------- | --------------------------------------- |
| `id`           | TEXT    | no       | Primary key.                            |
| `entity_id`    | TEXT    | no       | → `entities`.                           |
| `document_id`  | TEXT    | no       | → `documents`.                          |
| `chunk_id`     | TEXT    | yes      | → `chunks`, when the mention is in one. |
| `start_offset` | INTEGER | no       | `DEFAULT 0`.                            |
| `end_offset`   | INTEGER | no       | `DEFAULT 0`.                            |
| `confidence`   | REAL    | no       | `DEFAULT 0`.                            |

Unique index: `(entity_id, document_id, start_offset)`.

### 3.13 `index_jobs` and `index_meta`

| Column        | Type    | Nullable | Notes                                 |
| ------------- | ------- | -------- | ------------------------------------- |
| `id`          | TEXT    | no       | `index_jobs` primary key.             |
| `path`        | TEXT    | no       | Vault-relative path to (re)index.     |
| `op`          | TEXT    | no       | `upsert`, `delete`, ...               |
| `priority`    | INTEGER | no       | `DEFAULT 0`.                          |
| `enqueued_at` | INTEGER | no       | Unix epoch milliseconds.              |
| `attempts`    | INTEGER | no       | `DEFAULT 0`.                          |

Index: `(priority, enqueued_at)`. `index_meta(key TEXT PRIMARY KEY, value
TEXT)` stores per-brain index bookkeeping (e.g. the last completed scan).

---

## 4. Migrations

- Migrations are **numbered SQL files** under `migrations/`: the global store
  uses `migrations/global/` (`0001_initial.sql`, `0002_brains_lifecycle.sql`)
  and each brain uses `migrations/brain/` (`0001_initial.sql`,
  `0002_chunks_fts_external_content.sql`).
- The kernel applies pending migrations on store open and records each applied
  version in `schema_version`.
- Applying migrations is **idempotent**: an already-migrated store reopens
  without re-applying any migration.
- **Newer-schema refusal:** if the on-disk `schema_version` is greater than the
  kernel's known maximum, the kernel refuses to open the store and returns a
  clear error rather than risking corruption.

---

## 5. Audit log tamper-evidence

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
