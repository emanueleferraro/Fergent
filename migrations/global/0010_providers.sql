CREATE TABLE provider_configs (
    id          TEXT    PRIMARY KEY,
    kind        TEXT    NOT NULL,
    base_url    TEXT,
    secret_name TEXT,
    models      TEXT    NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE provider_routes (
    task       TEXT    PRIMARY KEY,
    models     TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
);
