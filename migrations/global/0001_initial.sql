-- Fergents global storage schema, migration 0001.
-- Applied by the Rust migration runner, which also inserts the schema_version row.

CREATE TABLE schema_version (
    version     INTEGER PRIMARY KEY,
    description TEXT    NOT NULL,
    checksum    TEXT    NOT NULL,
    applied_at  INTEGER NOT NULL
);

CREATE TABLE events (
    id             TEXT PRIMARY KEY,
    ts             INTEGER NOT NULL,
    kind           TEXT    NOT NULL,
    source         TEXT    NOT NULL,
    correlation_id TEXT,
    brain_id       TEXT,
    project_id     TEXT,
    payload        TEXT    NOT NULL DEFAULT '{}',
    version        INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_events_kind_ts     ON events (kind, ts);
CREATE INDEX idx_events_correlation ON events (correlation_id);
CREATE INDEX idx_events_ts          ON events (ts);

CREATE TABLE audit_log (
    id             TEXT PRIMARY KEY,
    ts             INTEGER NOT NULL,
    actor_type     TEXT    NOT NULL,
    actor_id       TEXT    NOT NULL,
    session_id     TEXT,
    action         TEXT    NOT NULL,
    target         TEXT,
    args_json      TEXT    NOT NULL DEFAULT '{}',
    result         TEXT    NOT NULL,
    reason         TEXT,
    correlation_id TEXT,
    reversible     INTEGER NOT NULL DEFAULT 0,
    revert_ref     TEXT,
    prev_hash      TEXT    NOT NULL,
    hash           TEXT    NOT NULL
);
CREATE INDEX idx_audit_actor  ON audit_log (actor_type, actor_id, ts);
CREATE INDEX idx_audit_action ON audit_log (action, ts);
CREATE INDEX idx_audit_ts     ON audit_log (ts);

CREATE TABLE sessions (
    id               TEXT PRIMARY KEY,
    client_kind      TEXT    NOT NULL,
    client_version   TEXT,
    protocol_version INTEGER NOT NULL,
    capabilities     TEXT    NOT NULL DEFAULT '{}',
    started_at       INTEGER NOT NULL,
    last_seen_at     INTEGER NOT NULL,
    closed_at        INTEGER
);

CREATE TABLE brains (
    brain_id       TEXT PRIMARY KEY,
    name           TEXT    NOT NULL,
    path           TEXT    NOT NULL,
    role           TEXT,
    schema_version INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    last_opened_at INTEGER
);
CREATE UNIQUE INDEX idx_brains_path ON brains (path);

CREATE TABLE projects (
    project_id TEXT PRIMARY KEY,
    name       TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE project_brains (
    project_id TEXT NOT NULL REFERENCES projects (project_id) ON DELETE CASCADE,
    brain_id   TEXT NOT NULL REFERENCES brains (brain_id) ON DELETE CASCADE,
    role       TEXT NOT NULL DEFAULT 'primary',
    PRIMARY KEY (project_id, brain_id)
);

CREATE TABLE llm_requests (
    id            TEXT PRIMARY KEY,
    ts            INTEGER NOT NULL,
    provider      TEXT,
    model         TEXT,
    task          TEXT,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd      REAL    NOT NULL DEFAULT 0,
    latency_ms    INTEGER,
    agent_id      TEXT,
    session_id    TEXT,
    status        TEXT    NOT NULL DEFAULT 'ok'
);
CREATE INDEX idx_llm_requests_ts       ON llm_requests (ts);
CREATE INDEX idx_llm_requests_provider ON llm_requests (provider, model, ts);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
);
