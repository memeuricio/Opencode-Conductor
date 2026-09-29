CREATE TABLE user_decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    question TEXT NOT NULL CHECK (length(trim(question)) BETWEEN 1 AND 2000),
    context TEXT,
    options TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK (status IN ('open', 'answered', 'cancelled')),
    answer TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    answered_at TEXT
);

CREATE INDEX user_decisions_by_task ON user_decisions (task_id, created_at DESC, id DESC);

-- Solo puede existir una pregunta abierta por tarea.
CREATE UNIQUE INDEX user_decisions_open_per_task
    ON user_decisions (task_id)
    WHERE status = 'open';
