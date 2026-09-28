ALTER TABLE worktrees ADD COLUMN opencode_session_id TEXT;
ALTER TABLE worktrees ADD COLUMN opencode_agent_id TEXT;
ALTER TABLE worktrees ADD COLUMN opencode_provider_id TEXT;
ALTER TABLE worktrees ADD COLUMN opencode_model_id TEXT;
ALTER TABLE worktrees ADD COLUMN opencode_location_directory TEXT;
ALTER TABLE worktrees ADD COLUMN opencode_location_matches INTEGER;

CREATE UNIQUE INDEX worktrees_opencode_session_id
    ON worktrees (opencode_session_id)
    WHERE opencode_session_id IS NOT NULL;
