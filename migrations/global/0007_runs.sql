-- Fergents global storage schema, migration 0007.
-- Phase 2 — runs, run steps, and approvals (spec: Spec / Plan / Tasks Workflow).

CREATE TABLE runs (
    id          TEXT    PRIMARY KEY,
    session_id  TEXT,
    kind        TEXT    NOT NULL,
    mode        TEXT    NOT NULL,
    state       TEXT    NOT NULL,
    budget      TEXT,
    artifacts   TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_runs_session ON runs (session_id, created_at);

CREATE TABLE run_steps (
    id          TEXT    PRIMARY KEY,
    run_id      TEXT    NOT NULL,
    seq         INTEGER NOT NULL,
    kind        TEXT    NOT NULL,
    state       TEXT    NOT NULL,
    payload     TEXT,
    started_at  INTEGER,
    finished_at INTEGER
);
CREATE INDEX idx_run_steps_run ON run_steps (run_id, seq);

CREATE TABLE approvals (
    id          TEXT    PRIMARY KEY,
    run_id      TEXT    NOT NULL,
    gate        TEXT    NOT NULL,
    ref         TEXT,
    status      TEXT    NOT NULL,
    payload     TEXT,
    decided_by  TEXT,
    decided_at  INTEGER,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_approvals_run ON approvals (run_id, gate);
