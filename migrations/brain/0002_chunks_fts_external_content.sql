-- Fergents per-brain index schema, migration 0002 (Task 10).
--
-- Recreate `chunks_fts` as an FTS5 **external-content** table linked to
-- `chunks` (spec Appendix B: "FTS5 virtual table over `content` (+
-- `heading_path`)"). The migration-0001 table stored its own copy of the
-- content; an external-content index shares the `chunks` rows via
-- `content_rowid = 'rowid'` instead of duplicating text, and is maintained by
-- the indexing pipeline (insert on chunk write, `'delete'` on removal).
--
-- `rebuild` repopulates the index from `chunks` so that brains indexed under
-- migration 0001 keep a usable FTS index without a full re-index.

DROP TABLE chunks_fts;

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    heading_path,
    content = 'chunks',
    content_rowid = 'rowid',
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO chunks_fts (chunks_fts) VALUES ('rebuild');
