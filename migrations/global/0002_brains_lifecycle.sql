-- Fergents global storage schema, migration 0002.
-- Phase 1 — brain lifecycle (BRAIN.md §9). Archiving a brain removes it from the
-- active list while keeping its files on disk, so the registry records when a
-- brain was archived (NULL = active).

ALTER TABLE brains ADD COLUMN archived_at INTEGER;
