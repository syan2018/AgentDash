DROP INDEX IF EXISTS idx_agent_run_mailbox_messages_runtime_status;
DROP INDEX IF EXISTS idx_agent_run_mailbox_messages_delivery_runtime_status;
DROP INDEX IF EXISTS idx_agent_run_mailbox_states_runtime_ref;
DROP INDEX IF EXISTS idx_agent_run_mailbox_states_delivery_runtime_ref;

ALTER TABLE agent_run_mailbox_messages
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_messages_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_messages
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_messages_delivery_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_states
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_states_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_states
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_states_delivery_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_messages
    RENAME COLUMN runtime_session_id TO delivery_runtime_session_id;

ALTER TABLE agent_run_mailbox_states
    RENAME COLUMN runtime_session_id TO delivery_runtime_session_id;

ALTER TABLE agent_run_mailbox_messages
    ALTER COLUMN delivery_runtime_session_id DROP NOT NULL;

ALTER TABLE agent_run_mailbox_states
    ALTER COLUMN delivery_runtime_session_id DROP NOT NULL;

ALTER TABLE agent_run_mailbox_messages
    ADD CONSTRAINT agent_run_mailbox_messages_delivery_runtime_session_id_fkey
    FOREIGN KEY (delivery_runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL;

ALTER TABLE agent_run_mailbox_states
    ADD CONSTRAINT agent_run_mailbox_states_delivery_runtime_session_id_fkey
    FOREIGN KEY (delivery_runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_delivery_runtime_status
    ON agent_run_mailbox_messages USING btree (
        delivery_runtime_session_id,
        status,
        barrier,
        drain_mode
    )
    WHERE delivery_runtime_session_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_states_delivery_runtime_ref
    ON agent_run_mailbox_states USING btree (delivery_runtime_session_id)
    WHERE delivery_runtime_session_id IS NOT NULL;
