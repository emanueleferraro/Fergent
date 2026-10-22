-- Fergents global storage schema, migration 0005.
-- Phase 2 — agent memory (spec: Agent Memory; AGENTS.md §8).

CREATE TABLE agent_memory (
    id           TEXT    PRIMARY KEY,
    scope        TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    content      TEXT    NOT NULL,
    importance   REAL    NOT NULL DEFAULT 0.5,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX idx_agent_memory_scope ON agent_memory (scope);
CREATE INDEX idx_agent_memory_kind ON agent_memory (kind);
