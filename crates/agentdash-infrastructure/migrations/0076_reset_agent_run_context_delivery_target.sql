-- AgentRun Runtime bindings now persist the connector/executor delivery target used to enrich
-- ContextFrame delivery metadata. Existing pre-release lineage documents cannot derive that
-- target from Managed Runtime or Host profile facts without guessing across product boundaries.
-- Rebuild the complete derived Runtime owner graph from Lifecycle/AgentFrame facts so every new
-- binding and immutable surface artifact is compiled with one authoritative delivery target.

DELETE FROM agent_run_control_effects;
DELETE FROM agent_runtime_terminal_application_effect_outbox;
DELETE FROM agent_run_runtime_binding_lineage;
DELETE FROM agent_run_runtime_recovery_intent;
DELETE FROM agent_run_runtime_thread_anchor;
UPDATE agent_run_mailbox_messages
SET accepted_runtime_operation_id = NULL
WHERE accepted_runtime_operation_id IS NOT NULL;
DELETE FROM permission_grants
WHERE source_runtime_operation_id IS NOT NULL;
DELETE FROM agent_runtime_thread;
DELETE FROM agent_runtime_binding;
TRUNCATE TABLE agent_runtime_surface_snapshot;
