-- Phase 4.3: skill registry, permissions, storage, cache, and invocations.
CREATE TABLE IF NOT EXISTS skills (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    version       TEXT NOT NULL,
    kind          TEXT NOT NULL,
    source        TEXT NOT NULL,
    path          TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    config        TEXT,
    installed_at  INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_permissions (
    skill_id TEXT NOT NULL,
    category TEXT NOT NULL,
    scope    TEXT,
    granted  INTEGER NOT NULL,
    PRIMARY KEY (skill_id, category)
);

CREATE TABLE IF NOT EXISTS skill_storage (
    skill_id   TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      BLOB NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (skill_id, key)
);

CREATE TABLE IF NOT EXISTS skill_cache (
    skill_id   TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    output     TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (skill_id, input_hash)
);

CREATE INDEX IF NOT EXISTS idx_skill_cache_expiry ON skill_cache (expires_at);

CREATE TABLE IF NOT EXISTS skill_invocations (
    id          TEXT PRIMARY KEY,
    skill_id    TEXT NOT NULL,
    parent_id   TEXT,
    depth       INTEGER NOT NULL,
    status      TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skill_invocations_skill
    ON skill_invocations (skill_id, created_at);
