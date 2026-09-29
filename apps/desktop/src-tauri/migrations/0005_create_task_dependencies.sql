CREATE TABLE task_dependencies (
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    depends_on_task_id INTEGER NOT NULL REFERENCES tasks (id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
);

CREATE INDEX task_dependencies_by_dependency
    ON task_dependencies (depends_on_task_id);
