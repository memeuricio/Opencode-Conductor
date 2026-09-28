CREATE TABLE worktrees (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects (id),
    label TEXT NOT NULL CHECK (length(trim(label)) BETWEEN 1 AND 80),
    branch_name TEXT NOT NULL UNIQUE,
    directory TEXT NOT NULL UNIQUE,
    base_commit TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('creating', 'ready', 'failed')),
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX worktrees_by_project
    ON worktrees (project_id, created_at DESC);
