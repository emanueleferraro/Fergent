CREATE TABLE mcp_servers (
    id         TEXT    PRIMARY KEY,
    command    TEXT,
    args       TEXT    NOT NULL,
    allow      INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
