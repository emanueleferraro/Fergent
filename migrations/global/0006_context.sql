-- Fergents global storage schema, migration 0006.
-- Phase 2 — context snapshots (spec: Context Viewer; Appendix B).

CREATE TABLE context_snapshots (
    id           TEXT    PRIMARY KEY,
    session_id   TEXT    NOT NULL,
    turn_seq     INTEGER NOT NULL,
    sections     TEXT    NOT NULL,
    total_tokens INTEGER NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_context_snapshots_session ON context_snapshots (session_id, turn_seq);
