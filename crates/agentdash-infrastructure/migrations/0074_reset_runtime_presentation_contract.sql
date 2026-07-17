-- Session presentation now persists the producer-owned immutable protocol body beside Runtime
-- carrier metadata. Existing pre-release Runtime rows cannot be upgraded without reconstructing
-- source identity, explicit nulls, timestamps, and ordering, so the Runtime owner graph and its
-- derived application-effect ledgers are reprovisioned from product/Lifecycle facts.

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
