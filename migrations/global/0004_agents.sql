-- Fergents global storage schema, migration 0004.
-- Phase 2 — agent runtime (spec: Agent Runtime / Chat Sessions; AGENTS.md §15).

CREATE TABLE agent_sessions (
    id          TEXT    PRIMARY KEY,
    agent_id    TEXT    NOT NULL,
    title       TEXT,
    state       TEXT    NOT NULL DEFAULT 'created',
    summary     TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_agent_sessions_agent ON agent_sessions (agent_id);

CREATE TABLE agent_messages (
    id          TEXT    PRIMARY KEY,
    session_id  TEXT    NOT NULL,
    role        TEXT    NOT NULL,
    content     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_agent_messages_session ON agent_messages (session_id);
