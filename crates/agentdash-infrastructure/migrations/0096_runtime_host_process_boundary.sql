-- Product persists the stable concrete-Agent association. Runtime and Host coordination are
-- rebuilt in memory from Product intent plus Complete Agent read/inspect, so their process state
-- has no database owner.

DELETE FROM workflow_agent_call_product_graph_effects;
DELETE FROM workflow_agent_call_product_effects;
DELETE FROM workflow_agent_call_product_sagas;
DELETE FROM companion_continuation_saga;
DELETE FROM companion_fresh_saga;
DELETE FROM agent_run_fork_saga;
DELETE FROM agent_run_product_runtime_binding;

DROP TABLE agent_run_product_runtime_command_claim;
DROP TABLE agent_run_product_runtime_recovery_saga;
DROP TABLE agent_run_product_mailbox_command_receipt;
DROP TABLE agent_run_product_mailbox_change;
DROP TABLE agent_run_product_mailbox_head;
DROP TABLE agent_runtime_callback_revision;
DROP TABLE agent_runtime_host_revision;
DROP TABLE agent_runtime_state_revision;

-- Dash owns one canonical source document. History, branches, commands, effects, and changes are
-- validated inside that document instead of being mirrored into relational projections.
DROP TABLE dash_agent_change;
DROP TABLE dash_agent_effect;
DROP TABLE dash_agent_command;
DROP TABLE dash_agent_branch CASCADE;
DROP TABLE dash_agent_history CASCADE;

ALTER TABLE dash_agent_session
    DROP COLUMN branch_id,
    DROP COLUMN head_revision,
    DROP COLUMN head_entry_id,
    DROP COLUMN history_digest;

ALTER TABLE dash_complete_source
    DROP COLUMN repository_revision;

ALTER TABLE dash_complete_effect
    DROP COLUMN request_fingerprint,
    DROP COLUMN receipt,
    DROP COLUMN inspection;

ALTER TABLE agent_run_product_runtime_binding
    DROP COLUMN change_delivery_state,
    DROP CONSTRAINT agent_run_product_runtime_binding_document_coordinates_match,
    ADD CONSTRAINT agent_run_product_runtime_binding_document_coordinates_match
    CHECK (
        binding #>> '{target,run_id}' = target_run_id
        AND binding #>> '{target,agent_id}' = target_agent_id
        AND binding ->> 'runtime_thread_id' = runtime_thread_id
        AND binding #>> '{launch_frame,frame_id}' = launch_frame_id
        AND binding #>> '{launch_frame,agent_id}' = target_agent_id
        AND COALESCE(btrim(binding #>> '{agent,service_instance_id}'), '') <> ''
        AND COALESCE(btrim(binding #>> '{agent,source}'), '') <> ''
    );
