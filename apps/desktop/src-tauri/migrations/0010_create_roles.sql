CREATE TABLE roles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    name_key TEXT NOT NULL UNIQUE,
    description TEXT,
    instructions TEXT,
    agent_id TEXT NOT NULL CHECK (length(trim(agent_id)) BETWEEN 1 AND 200),
    provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 200),
    model_id TEXT NOT NULL CHECK (length(trim(model_id)) BETWEEN 1 AND 300),
    fallback_provider_id TEXT,
    fallback_model_id TEXT,
    file_scope TEXT NOT NULL DEFAULT '',
    match_keywords TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    archived_at TEXT
);

CREATE INDEX roles_active_by_name
    ON roles (name COLLATE NOCASE)
    WHERE archived_at IS NULL;
