CREATE TABLE IF NOT EXISTS agent_run_delivery_command_receipts (
    id text NOT NULL,
    scope_kind text NOT NULL,
    scope_key text NOT NULL,
    client_command_id text NOT NULL,
    request_digest text NOT NULL,
    status text NOT NULL,
    run_id text,
    agent_id text,
    frame_id text,
    frame_revision integer,
    runtime_session_id text,
    turn_id text,
    error_message text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    accepted_at timestamp with time zone,
    failed_at timestamp with time zone,
    CONSTRAINT agent_run_delivery_command_receipts_status_check CHECK (
        status = ANY (ARRAY['pending'::text, 'accepted'::text, 'terminal_failed'::text])
    )
);

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_scope_command_key UNIQUE (
        scope_kind,
        scope_key,
        client_command_id
    );

CREATE INDEX IF NOT EXISTS idx_agent_run_delivery_command_receipts_scope
    ON agent_run_delivery_command_receipts USING btree (scope_kind, scope_key, updated_at);

CREATE INDEX IF NOT EXISTS idx_agent_run_delivery_command_receipts_run_agent
    ON agent_run_delivery_command_receipts USING btree (run_id, agent_id, updated_at);

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_run_id_fkey
    FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_agent_id_fkey
    FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_frame_id_fkey
    FOREIGN KEY (frame_id) REFERENCES agent_frames(id) ON DELETE SET NULL;

ALTER TABLE ONLY agent_run_delivery_command_receipts
    ADD CONSTRAINT agent_run_delivery_command_receipts_runtime_session_id_fkey
    FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE SET NULL;
