CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects (id),
    worktree_id INTEGER REFERENCES worktrees (id),
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 120),
    objective TEXT NOT NULL CHECK (length(trim(objective)) BETWEEN 1 AND 8000),
    status TEXT NOT NULL CHECK (status IN ('pending', 'ready', 'working', 'blocked', 'review', 'completed', 'failed')),
    agent_id TEXT NOT NULL CHECK (length(trim(agent_id)) BETWEEN 1 AND 200),
    provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 200),
    model_id TEXT NOT NULL CHECK (length(trim(model_id)) BETWEEN 1 AND 300),
    file_scope TEXT NOT NULL DEFAULT '',
    blocker_reason TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX tasks_by_project ON tasks (project_id, created_at DESC, id DESC);

-- Un worktree solo puede sostener una tarea activa a la vez.
CREATE UNIQUE INDEX tasks_active_per_worktree
    ON tasks (worktree_id)
    WHERE worktree_id IS NOT NULL AND status IN ('working', 'blocked', 'review');
