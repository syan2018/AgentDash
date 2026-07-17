ALTER TABLE agent_run_mailbox_messages
    DROP COLUMN accepted_agent_run_turn_id,
    DROP COLUMN accepted_protocol_turn_id,
    ADD COLUMN reconcile_required boolean NOT NULL DEFAULT false,
    ADD COLUMN delivery_request_digest text;

UPDATE agent_run_mailbox_messages
SET delivery_request_digest = 'pre-0079:' || id;

ALTER TABLE agent_run_mailbox_messages
    ALTER COLUMN delivery_request_digest SET NOT NULL;

ALTER TABLE agent_run_product_command_receipts
    ADD COLUMN acceptance_results_json jsonb;

ALTER TABLE agent_run_mailbox_states
    DROP COLUMN backend_selection_preference;

ALTER TABLE agent_run_mailbox_messages
    DROP COLUMN executor_config_json;

CREATE UNIQUE INDEX agent_run_product_command_receipts_mailbox_uidx
    ON agent_run_product_command_receipts(mailbox_message_id)
    WHERE mailbox_message_id IS NOT NULL;
