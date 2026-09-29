-- Keep spaces as optional groupings and retain a single Builder assignment per
-- project. The workspace Planner experiment has not been released.
DELETE FROM project_agents WHERE role = 'planner';
DELETE FROM agent_profiles WHERE role = 'planner';
DROP TABLE planner_publications;
