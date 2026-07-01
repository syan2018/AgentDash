ALTER TABLE IF EXISTS agent_run_mailbox_messages
    ADD COLUMN IF NOT EXISTS launch_planning_input jsonb;

ALTER TABLE IF EXISTS agent_run_mailbox_states
    ADD COLUMN IF NOT EXISTS backend_selection_preference jsonb;
