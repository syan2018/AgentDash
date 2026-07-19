-- Complete Agent Runtime recovery retains immutable target generations inside the Host fact graph.
-- Fresh and upgraded databases converge before the Host repository decodes its first snapshot.

UPDATE agent_runtime_host_revision
SET facts = facts || '{"runtime_target_recoveries": {}}'::jsonb
WHERE NOT facts ? 'runtime_target_recoveries';
