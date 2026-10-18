-- Fergents per-brain index schema, migration 0001 (spec Appendix B, normative).
-- Applied by the Rust migration runner, which also inserts the schema_version row.
-- Columns marked NULL in Appendix B are nullable; the rest are NOT NULL.

CREATE TABLE schema_version (
    version     INTEGER PRIMARY KEY,
    description TEXT    NOT NULL,
    checksum    TEXT    NOT NULL,
    applied_at  INTEGER NOT NULL
);

CREATE TABLE documents (
    id             TEXT PRIMARY KEY,
    path           TEXT    NOT NULL UNIQUE,
    kind           TEXT    NOT NULL,
    title          TEXT    NOT NULL,
    mime           TEXT    NOT NULL,
    size           INTEGER NOT NULL,
    hash           TEXT    NOT NULL,
    frontmatter    TEXT,
    word_count     INTEGER NOT NULL DEFAULT 0,
    reading_time_s INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    indexed_at     INTEGER NOT NULL,
    deleted_at     INTEGER
);
CREATE INDEX idx_documents_path       ON documents (path);
CREATE INDEX idx_documents_kind       ON documents (kind);
CREATE INDEX idx_documents_deleted_at ON documents (deleted_at);

CREATE TABLE notes_meta (
    document_id TEXT PRIMARY KEY REFERENCES documents (id),
    aliases     TEXT    NOT NULL DEFAULT '[]',
    cssclasses  TEXT    NOT NULL DEFAULT '[]',
    publish     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE chunks (
    id           TEXT    PRIMARY KEY,
    document_id  TEXT    NOT NULL REFERENCES documents (id),
    ordinal      INTEGER NOT NULL,
    content      TEXT    NOT NULL,
    token_count  INTEGER NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset   INTEGER NOT NULL,
    heading_path TEXT,
    language     TEXT,
    symbol       TEXT
);
CREATE INDEX idx_chunks_document_ordinal ON chunks (document_id, ordinal);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    heading_path,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE vec_chunks USING vec0(
    chunk_id TEXT PRIMARY KEY,
    embedding float[1024]
);

CREATE TABLE embedding_meta (
    chunk_id     TEXT    PRIMARY KEY REFERENCES chunks (id),
    model        TEXT    NOT NULL,
    dim          INTEGER NOT NULL,
    content_hash TEXT    NOT NULL,
    sparse       TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_embedding_meta_content_hash ON embedding_meta (content_hash);

CREATE TABLE tags (
    id     TEXT PRIMARY KEY,
    name   TEXT NOT NULL UNIQUE,
    parent TEXT
);

CREATE TABLE document_tags (
    document_id TEXT NOT NULL REFERENCES documents (id),
    tag_id      TEXT NOT NULL REFERENCES tags (id),
    PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE links (
    id              TEXT    PRIMARY KEY,
    src_document_id TEXT    NOT NULL REFERENCES documents (id),
    dst_document_id TEXT,
    dst_path        TEXT,
    dst_heading     TEXT,
    dst_block       TEXT,
    display_text    TEXT,
    anchor          TEXT,
    kind            TEXT    NOT NULL,
    unresolved      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_links_src_document  ON links (src_document_id);
CREATE INDEX idx_links_dst_document  ON links (dst_document_id);
CREATE INDEX idx_links_dst_path      ON links (dst_path);

CREATE TABLE entities (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    type            TEXT NOT NULL,
    canonical_name  TEXT,
    normalized_name TEXT NOT NULL,
    metadata        TEXT,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL
);
CREATE INDEX idx_entities_normalized_name_type ON entities (normalized_name, type);

CREATE TABLE relations (
    id         TEXT PRIMARY KEY,
    src        TEXT NOT NULL,
    dst        TEXT NOT NULL,
    kind       TEXT NOT NULL,
    weight     REAL NOT NULL DEFAULT 0,
    confidence REAL NOT NULL DEFAULT 0,
    metadata   TEXT,
    source_doc TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_relations_src_kind ON relations (src, kind);
CREATE INDEX idx_relations_dst_kind ON relations (dst, kind);

CREATE TABLE entity_mentions (
    id           TEXT PRIMARY KEY,
    entity_id    TEXT NOT NULL REFERENCES entities (id),
    document_id  TEXT NOT NULL REFERENCES documents (id),
    chunk_id     TEXT,
    start_offset INTEGER NOT NULL DEFAULT 0,
    end_offset   INTEGER NOT NULL DEFAULT 0,
    confidence   REAL NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_entity_mentions_entity_document_offset
    ON entity_mentions (entity_id, document_id, start_offset);

CREATE TABLE index_jobs (
    id          TEXT PRIMARY KEY,
    path        TEXT    NOT NULL,
    op          TEXT    NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0,
    enqueued_at INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_index_jobs_priority_enqueued ON index_jobs (priority, enqueued_at);

CREATE TABLE index_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
