-- Phase 5: the workflow engine (spec `.trae/specs/workflow-phase-5/spec.md`
-- Appendix B). Forward-only, idempotent, and additive: no existing table is
-- altered.

CREATE TABLE IF NOT EXISTS workflows (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    version     TEXT    NOT NULL DEFAULT '0.1.0',
    path        TEXT    NOT NULL,
    digest      TEXT    NOT NULL,
    app         TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_versions (
    id          TEXT    PRIMARY KEY,
    workflow_id TEXT    NOT NULL,
    version     INTEGER NOT NULL,
    yaml        TEXT    NOT NULL,
    digest      TEXT    NOT NULL,
    author      TEXT,
    message     TEXT,
    created_at  INTEGER NOT NULL,
    UNIQUE (workflow_id, version)
);

CREATE TABLE IF NOT EXISTS workflow_runs (
    id                 TEXT    PRIMARY KEY,
    workflow_id        TEXT    NOT NULL,
    workflow_version   INTEGER NOT NULL,
    parent_run_id      TEXT,
    trigger_kind       TEXT    NOT NULL,
    trigger_id         TEXT,
    state              TEXT    NOT NULL,
    actor              TEXT    NOT NULL,
    dry_run            INTEGER NOT NULL DEFAULT 0,
    inputs             TEXT,
    context            TEXT,
    error              TEXT,
    started_at         INTEGER,
    finished_at        INTEGER,
    created_at         INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_steps (
    id          TEXT    PRIMARY KEY,
    run_id      TEXT    NOT NULL,
    node_id     TEXT    NOT NULL,
    iteration   INTEGER,
    state       TEXT    NOT NULL,
    attempt     INTEGER NOT NULL DEFAULT 1,
    inputs      TEXT,
    outputs     TEXT,
    error       TEXT,
    started_at  INTEGER,
    finished_at INTEGER
);

CREATE TABLE IF NOT EXISTS workflow_checkpoints (
    run_id     TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    node_id    TEXT,
    context    TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (run_id, seq)
);

CREATE TABLE IF NOT EXISTS workflow_triggers (
    workflow_id   TEXT    NOT NULL,
    trigger_id    TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_fired_at INTEGER,
    next_fire_at  INTEGER,
    last_error    TEXT,
    PRIMARY KEY (workflow_id, trigger_id)
);

CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow
    ON workflow_runs (workflow_id, created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_run
    ON workflow_steps (run_id, node_id);
CREATE INDEX IF NOT EXISTS idx_workflow_versions_workflow
    ON workflow_versions (workflow_id, version);
