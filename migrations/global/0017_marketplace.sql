-- Phase 4.4: marketplace sources and cached index entries.
CREATE TABLE IF NOT EXISTS marketplace_sources (
    id                TEXT PRIMARY KEY,
    url               TEXT NOT NULL,
    kind              TEXT NOT NULL,
    key_id            TEXT,
    enabled           INTEGER NOT NULL DEFAULT 1,
    last_refreshed_at INTEGER,
    etag              TEXT
);

CREATE TABLE IF NOT EXISTS marketplace_cache (
    source_id  TEXT NOT NULL,
    entry_id   TEXT NOT NULL,
    kind       TEXT NOT NULL,
    entry      TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (source_id, kind, entry_id)
);
