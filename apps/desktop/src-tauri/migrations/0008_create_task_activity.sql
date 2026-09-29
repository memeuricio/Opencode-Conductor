CREATE TABLE task_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    kind TEXT NOT NULL CHECK (length(trim(kind)) BETWEEN 1 AND 60),
    detail TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX task_activity_by_task ON task_activity (task_id, created_at DESC, id DESC);
