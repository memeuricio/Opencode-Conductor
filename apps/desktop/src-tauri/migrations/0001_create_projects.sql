CREATE TABLE projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    description TEXT,
    root_path TEXT NOT NULL,
    root_path_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    archived_at TEXT
);

CREATE INDEX projects_active_by_name
    ON projects (name COLLATE NOCASE)
    WHERE archived_at IS NULL;
