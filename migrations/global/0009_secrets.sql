-- Fergents global storage schema, migration 0009.
-- Phase 2 — secret vault (spec D10, SECURITY.md §14).
-- Values live ONLY in the OS keychain; this table stores references.

CREATE TABLE secret_refs (
    name         TEXT    PRIMARY KEY,
    keychain_ref TEXT    NOT NULL,
    provider     TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_secret_refs_provider ON secret_refs (provider);
