CREATE TABLE IF NOT EXISTS executions (
    id             TEXT PRIMARY KEY,
    workspace_id   TEXT NOT NULL,
    service_id     TEXT NOT NULL,
    workflow_id    TEXT NOT NULL,
    workflow_name  TEXT NOT NULL,
    status         TEXT NOT NULL,
    trigger_kind   TEXT NOT NULL,
    parent_id      TEXT,
    depth          INTEGER NOT NULL DEFAULT 0,
    pc             INTEGER NOT NULL DEFAULT 0,
    phase          INTEGER NOT NULL DEFAULT 0,
    attempt        INTEGER NOT NULL DEFAULT 0,
    steps_executed INTEGER NOT NULL DEFAULT 0,
    current_step   TEXT,
    wake_at_ms     INTEGER,
    waiting_for    TEXT,
    context        TEXT NOT NULL,
    definition     TEXT NOT NULL,
    error          TEXT,
    created_ms     INTEGER NOT NULL,
    updated_ms     INTEGER NOT NULL,
    finished_ms    INTEGER
);

CREATE INDEX IF NOT EXISTS idx_exec_ws_created ON executions (workspace_id, created_ms DESC);
CREATE INDEX IF NOT EXISTS idx_exec_ws_status ON executions (workspace_id, status);

CREATE TABLE IF NOT EXISTS execution_events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_id TEXT NOT NULL REFERENCES executions (id) ON DELETE CASCADE,
    ts_ms        INTEGER NOT NULL,
    kind         TEXT NOT NULL,
    step_id      TEXT,
    message      TEXT NOT NULL,
    data         TEXT
);

CREATE INDEX IF NOT EXISTS idx_exec_events ON execution_events (execution_id, seq);
