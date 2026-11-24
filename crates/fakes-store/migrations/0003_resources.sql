CREATE TABLE IF NOT EXISTS resources (
    workspace_id TEXT NOT NULL,
    service_id   TEXT NOT NULL,
    collection   TEXT NOT NULL,
    id           TEXT NOT NULL,
    data         TEXT NOT NULL,
    created_ms   INTEGER NOT NULL,
    updated_ms   INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, service_id, collection, id)
);

CREATE INDEX IF NOT EXISTS idx_resources_col ON resources (workspace_id, service_id, collection, created_ms);
