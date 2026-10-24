-- Phase 3.3: the language-server registry (spec Appendix B).
CREATE TABLE IF NOT EXISTS lsp_servers (
    id            TEXT PRIMARY KEY,
    language_id   TEXT NOT NULL,
    command       TEXT NOT NULL,
    args          TEXT,
    env           TEXT,
    root_strategy TEXT NOT NULL DEFAULT 'project',
    enabled       INTEGER NOT NULL DEFAULT 1,
    source        TEXT NOT NULL DEFAULT 'user',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_lsp_servers_language_command
    ON lsp_servers (language_id, command);
