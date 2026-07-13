ALTER TABLE agent_run_mailbox_messages
    ADD COLUMN accepted_agent_run_turn_id text,
    ADD COLUMN accepted_protocol_turn_id text;
