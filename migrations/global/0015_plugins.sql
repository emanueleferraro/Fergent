-- Phase 4.2: plugin registry, permissions, storage, and call metrics.
CREATE TABLE IF NOT EXISTS plugins (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    version          TEXT NOT NULL,
    api_version      INTEGER NOT NULL,
    kind             TEXT NOT NULL,
    source           TEXT NOT NULL,
    path             TEXT NOT NULL,
    manifest_hash    TEXT NOT NULL,
    signature_status TEXT NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 0,
    dev              INTEGER NOT NULL DEFAULT 0,
    degraded         INTEGER NOT NULL DEFAULT 0,
    config           TEXT,
    installed_at     INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS plugin_permissions (
    plugin_id  TEXT NOT NULL,
    category   TEXT NOT NULL,
    scope      TEXT,
    granted    INTEGER NOT NULL,
    granted_at INTEGER,
    PRIMARY KEY (plugin_id, category)
);

CREATE TABLE IF NOT EXISTS plugin_storage (
    plugin_id  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      BLOB NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

CREATE TABLE IF NOT EXISTS plugin_calls (
    id          TEXT PRIMARY KEY,
    plugin_id   TEXT NOT NULL,
    tool        TEXT NOT NULL,
    status      TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    error_code  TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_plugin_calls_plugin
    ON plugin_calls (plugin_id, created_at);
