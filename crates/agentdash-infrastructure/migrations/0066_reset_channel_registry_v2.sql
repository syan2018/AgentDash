-- Channel registry v2 introduces owner-local ChannelKey identity and orthogonal
-- lifetime/retention/origin contracts. The project is pre-release, so runtime
-- registries are rebuilt from their owners instead of preserving obsolete facts.
ALTER TABLE lifecycle_runs
    ALTER COLUMN channel_registry
    SET DEFAULT '{"schema_version":2,"channels":[]}'::jsonb;

UPDATE lifecycle_runs
SET channel_registry = '{"schema_version":2,"channels":[]}'::jsonb,
    updated_at = NOW();
