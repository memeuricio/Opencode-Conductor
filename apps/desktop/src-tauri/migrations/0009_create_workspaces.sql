CREATE TABLE workspaces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    name_key TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    archived_at TEXT
);

-- Un proyecto puede vivir en varios espacios (p. ej. backend compartido
-- entre "App finanzas" y otro espacio). El espacio es una lente de
-- agrupación: worktrees, sesiones y tareas siguen perteneciendo al proyecto.
CREATE TABLE workspace_projects (
    workspace_id INTEGER NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (workspace_id, project_id)
);

CREATE INDEX workspace_projects_by_project
    ON workspace_projects (project_id, workspace_id);

CREATE INDEX workspaces_active_by_name
    ON workspaces (name COLLATE NOCASE)
    WHERE archived_at IS NULL;
