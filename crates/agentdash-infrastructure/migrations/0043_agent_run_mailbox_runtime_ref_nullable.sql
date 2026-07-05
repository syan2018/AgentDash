ALTER TABLE agent_run_mailbox_messages
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_messages_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_states
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_states_runtime_session_id_fkey;

ALTER TABLE agent_run_mailbox_messages
    ALTER COLUMN runtime_session_id DROP NOT NULL;

ALTER TABLE agent_run_mailbox_states
    ALTER COLUMN runtime_session_id DROP NOT NULL;

ALTER TABLE agent_run_mailbox_messages
    ADD CONSTRAINT agent_run_mailbox_messages_runtime_session_id_fkey
    FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE SET NULL;

ALTER TABLE agent_run_mailbox_states
    ADD CONSTRAINT agent_run_mailbox_states_runtime_session_id_fkey
    FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE SET NULL;

DROP INDEX IF EXISTS idx_agent_run_mailbox_messages_runtime_status;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_runtime_status
    ON agent_run_mailbox_messages USING btree (runtime_session_id, status, barrier, drain_mode)
    WHERE runtime_session_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_claim_owner
    ON agent_run_mailbox_messages USING btree (
        run_id,
        agent_id,
        status,
        barrier,
        drain_mode,
        priority DESC,
        order_key ASC
    )
    WHERE status IN ('accepted', 'queued', 'ready_to_consume');

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_states_runtime_ref
    ON agent_run_mailbox_states USING btree (runtime_session_id)
    WHERE runtime_session_id IS NOT NULL;
