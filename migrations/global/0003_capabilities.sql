-- Fergents global storage schema, migration 0003.
-- Phase 2 — capability registry (spec Appendix B, decisions D1–D3).
-- Every product feature is a registered capability; `enabled` is the
-- kernel-authoritative switch. `default_enabled` seeds the initial state.

CREATE TABLE capabilities (
    id              TEXT    PRIMARY KEY,
    name            TEXT    NOT NULL,
    version         TEXT    NOT NULL,
    source          TEXT    NOT NULL,
    description     TEXT,
    dependencies    TEXT    NOT NULL DEFAULT '[]',
    resources       TEXT    NOT NULL DEFAULT '[]',
    default_enabled INTEGER NOT NULL DEFAULT 0,
    enabled         INTEGER NOT NULL DEFAULT 0,
    hot_toggle      INTEGER NOT NULL DEFAULT 0,
    config          TEXT,
    registered_at   INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX idx_capabilities_enabled ON capabilities (enabled);
