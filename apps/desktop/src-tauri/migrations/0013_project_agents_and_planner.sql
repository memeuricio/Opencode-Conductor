CREATE TABLE agent_profiles (
    role TEXT PRIMARY KEY CHECK (role IN ('planner', 'builder')),
    instructions TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO agent_profiles (role, instructions)
VALUES
    ('planner', ''),
    ('builder', '');

CREATE TABLE project_agents (
    project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('planner', 'builder')),
    agent_id TEXT NOT NULL CHECK (length(trim(agent_id)) BETWEEN 1 AND 200),
    provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 200),
    model_id TEXT NOT NULL CHECK (length(trim(model_id)) BETWEEN 1 AND 300),
    planner_session_id TEXT,
    planner_session_directory TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (project_id, role),
    UNIQUE (planner_session_id),
    CHECK (role = 'planner' OR (planner_session_id IS NULL AND planner_session_directory IS NULL)),
    CHECK ((planner_session_id IS NULL) = (planner_session_directory IS NULL))
);

ALTER TABLE tasks ADD COLUMN origin TEXT NOT NULL DEFAULT 'user'
    CHECK (origin IN ('user', 'planner'));

CREATE TABLE planner_publications (
    session_id TEXT NOT NULL,
    publication_key TEXT NOT NULL CHECK (length(trim(publication_key)) BETWEEN 1 AND 128),
    project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL,
    task_ids_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (session_id, publication_key)
);
