-- Phase 3.4: terminal sessions (spec Appendix B).
CREATE TABLE IF NOT EXISTS terminal_sessions (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    shell      TEXT NOT NULL,
    cwd        TEXT NOT NULL,
    cols       INTEGER NOT NULL,
    rows       INTEGER NOT NULL,
    state      TEXT NOT NULL,
    exit_code  INTEGER,
    actor      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    closed_at  INTEGER
);

CREATE INDEX IF NOT EXISTS idx_terminal_sessions_project
    ON terminal_sessions (project_id, created_at);
