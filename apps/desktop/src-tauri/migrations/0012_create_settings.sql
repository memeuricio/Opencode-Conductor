CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES ('activity_retention_days', '30');

CREATE INDEX task_activity_by_created ON task_activity (created_at);
