ALTER TABLE agent_run_delivery_bindings
    ADD COLUMN IF NOT EXISTS active_turn_id text,
    ADD COLUMN IF NOT EXISTS last_turn_id text,
    ADD COLUMN IF NOT EXISTS terminal_state text,
    ADD COLUMN IF NOT EXISTS terminal_message text;

CREATE INDEX IF NOT EXISTS idx_agent_run_delivery_bindings_active_turn
    ON agent_run_delivery_bindings (active_turn_id)
    WHERE active_turn_id IS NOT NULL;
