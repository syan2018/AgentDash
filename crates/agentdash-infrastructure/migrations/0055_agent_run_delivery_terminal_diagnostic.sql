ALTER TABLE agent_run_delivery_bindings
    ADD COLUMN IF NOT EXISTS terminal_diagnostic jsonb;
