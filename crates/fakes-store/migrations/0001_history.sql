CREATE TABLE IF NOT EXISTS requests (
    id                      TEXT PRIMARY KEY,
    ts_ms                   INTEGER NOT NULL,
    workspace_id            TEXT NOT NULL,
    service_id              TEXT,
    endpoint_id             TEXT,
    scenario_id             TEXT,
    method                  TEXT NOT NULL,
    path                    TEXT NOT NULL,
    query                   TEXT NOT NULL DEFAULT '',
    remote_addr             TEXT,
    request_headers         TEXT NOT NULL,
    request_body            BLOB NOT NULL,
    request_body_size       INTEGER NOT NULL,
    request_body_truncated  INTEGER NOT NULL DEFAULT 0,
    status                  INTEGER NOT NULL,
    response_headers        TEXT NOT NULL,
    response_body           BLOB NOT NULL,
    response_body_size      INTEGER NOT NULL,
    response_body_truncated INTEGER NOT NULL DEFAULT 0,
    duration_ms             INTEGER NOT NULL,
    outcome                 TEXT NOT NULL,
    message                 TEXT
);

CREATE INDEX IF NOT EXISTS idx_requests_ws_ts ON requests (workspace_id, ts_ms DESC);
CREATE INDEX IF NOT EXISTS idx_requests_ws_service ON requests (workspace_id, service_id, ts_ms DESC);

CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
