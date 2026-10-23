-- Fergents global storage schema, migration 0008.
-- Phase 2 — chat sessions, FTS search, and the prompt library
-- (spec: Chat Sessions, Messages, and Streaming; Appendix B).

ALTER TABLE agent_sessions ADD COLUMN project_id TEXT;
ALTER TABLE agent_sessions ADD COLUMN brain_id TEXT;
ALTER TABLE agent_sessions ADD COLUMN parent_session_id TEXT;
ALTER TABLE agent_sessions ADD COLUMN model TEXT;
ALTER TABLE agent_sessions ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE agent_sessions ADD COLUMN tokens_in INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN tokens_out INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN cost_usd REAL NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN closed_at INTEGER;

ALTER TABLE agent_messages ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_messages ADD COLUMN tool_calls TEXT;
ALTER TABLE agent_messages ADD COLUMN tokens INTEGER NOT NULL DEFAULT 0;

-- Backfill `seq` (1..n per session) before enforcing uniqueness.
UPDATE agent_messages SET seq = (
    SELECT COUNT(*) FROM agent_messages AS older
    WHERE older.session_id = agent_messages.session_id
      AND older.rowid <= agent_messages.rowid
);

CREATE UNIQUE INDEX idx_agent_messages_seq ON agent_messages (session_id, seq);

-- External-content FTS5 index over message content (decision D11).
CREATE VIRTUAL TABLE agent_messages_fts USING fts5(
    content,
    content='agent_messages',
    content_rowid='rowid',
    tokenize='unicode61'
);

CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts (rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER agent_messages_fts_ad AFTER DELETE ON agent_messages BEGIN
    INSERT INTO agent_messages_fts (agent_messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER agent_messages_fts_au AFTER UPDATE ON agent_messages BEGIN
    INSERT INTO agent_messages_fts (agent_messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
    INSERT INTO agent_messages_fts (rowid, content) VALUES (new.rowid, new.content);
END;

INSERT INTO agent_messages_fts (rowid, content) SELECT rowid, content FROM agent_messages;

CREATE TABLE chat_prompts (
    id         TEXT    PRIMARY KEY,
    title      TEXT    NOT NULL,
    body       TEXT    NOT NULL,
    tags       TEXT,
    created_at INTEGER NOT NULL
);
