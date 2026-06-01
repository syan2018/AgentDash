ALTER TABLE activity_execution_claims
    ADD COLUMN graph_instance_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

DROP INDEX IF EXISTS ux_activity_execution_claims_active_attempt;
CREATE UNIQUE INDEX ux_activity_execution_claims_active_attempt
    ON activity_execution_claims(run_id, graph_instance_id, activity_key, attempt)
    WHERE status IN ('claiming', 'running');
