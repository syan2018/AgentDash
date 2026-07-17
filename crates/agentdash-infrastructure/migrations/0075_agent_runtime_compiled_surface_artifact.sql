-- Compiled artifacts are the new immutable source of truth. Pre-release surface snapshots cannot
-- be upgraded because they never carried the presentation half of the compilation result.
TRUNCATE TABLE agent_runtime_surface_snapshot;

ALTER TABLE agent_runtime_surface_snapshot
    ADD COLUMN business_snapshot jsonb NOT NULL,
    ADD COLUMN presentation_plan jsonb NOT NULL;
