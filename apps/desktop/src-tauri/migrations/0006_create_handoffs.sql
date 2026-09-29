CREATE TABLE handoffs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    kind TEXT NOT NULL CHECK (kind IN ('handoff', 'completion')),
    summary TEXT NOT NULL CHECK (length(trim(summary)) BETWEEN 1 AND 8000),
    artifacts TEXT NOT NULL DEFAULT '[]',
    next_instructions TEXT,
    open_questions TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    reviewed_at TEXT,
    review_decision TEXT CHECK (review_decision IN ('accepted', 'returned')),
    review_note TEXT
);

CREATE INDEX handoffs_by_task ON handoffs (task_id, created_at DESC, id DESC);

-- Solo puede existir una entrega pendiente de revisión por tarea.
CREATE UNIQUE INDEX handoffs_pending_per_task
    ON handoffs (task_id)
    WHERE reviewed_at IS NULL;
