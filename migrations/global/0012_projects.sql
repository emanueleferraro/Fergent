-- Fergents global storage schema, migration 0012.
-- Phase 3 — project layer (spec Appendix B, decision D3).
--
-- Migration 0001 already introduced a minimal `projects` table (project_id,
-- name, path, created_at) used by the brain manager to link brains to
-- folders. Phase 3 extends that same table rather than introducing a second
-- notion of project: a project *is* a code workspace, and it may link a
-- brain. New projects are recorded untrusted; nothing executable may run
-- until the user trusts them.

ALTER TABLE projects ADD COLUMN vcs            TEXT;
ALTER TABLE projects ADD COLUMN brain_id        TEXT;
ALTER TABLE projects ADD COLUMN trust           TEXT    NOT NULL DEFAULT 'untrusted';
ALTER TABLE projects ADD COLUMN settings        TEXT;
ALTER TABLE projects ADD COLUMN last_opened_at  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN updated_at      INTEGER NOT NULL DEFAULT 0;

-- `path` is the identity of a workspace (canonical absolute path).
CREATE INDEX IF NOT EXISTS idx_projects_path ON projects (path);
CREATE INDEX IF NOT EXISTS idx_projects_last_opened ON projects (last_opened_at);

-- One layout tree (splits + tabs) per project (spec decision D14).
CREATE TABLE editor_layouts (
    project_id TEXT    PRIMARY KEY,
    layout     TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
);
