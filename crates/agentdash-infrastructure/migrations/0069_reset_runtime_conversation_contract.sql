-- Runtime journal and projections are JSONB facts whose item payload changed from the lossy
-- RuntimeItemContent mirror to the owned conversation protocol. The project is pre-release, so
-- rebuilding canonical runtime bindings is safer than retaining an old-payload reader. Product
-- lifecycle/mailbox facts and the service/offer catalog remain; only accepted operation refs and
-- runtime-derived permission grants are detached before the Runtime owner graph is rebuilt.

DELETE FROM agent_run_runtime_binding_lineage;
DELETE FROM agent_run_runtime_recovery_intent;
DELETE FROM agent_run_runtime_thread_anchor;
UPDATE agent_run_mailbox_messages SET accepted_runtime_operation_id = NULL
WHERE accepted_runtime_operation_id IS NOT NULL;
DELETE FROM permission_grants WHERE source_runtime_operation_id IS NOT NULL;
DELETE FROM agent_runtime_thread;
DELETE FROM agent_runtime_binding;
