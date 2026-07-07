ALTER TABLE IF EXISTS agent_run_command_receipts
    DROP CONSTRAINT IF EXISTS agent_run_command_receipts_command_kind_check;

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD CONSTRAINT agent_run_command_receipts_command_kind_check CHECK (
        command_kind = ANY (ARRAY[
            'message_submit'::text,
            'project_agent_start'::text,
            'agent_run_fork'::text,
            'agent_run_fork_submit'::text,
            'mailbox_promote'::text,
            'mailbox_delete'::text,
            'mailbox_move'::text,
            'mailbox_resume'::text,
            'cancel'::text,
            'context_compact'::text
        ])
    );

CREATE TABLE IF NOT EXISTS runtime_session_compaction_requests (
    id text NOT NULL,
    session_id text NOT NULL,
    run_id text NOT NULL,
    agent_id text NOT NULL,
    command_receipt_id text NOT NULL,
    status text NOT NULL,
    requested_mode text NOT NULL,
    keep_last_n integer,
    reserve_tokens integer,
    request_metadata jsonb,
    result_metadata jsonb,
    requested_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    consumed_turn_id text,
    completed_compaction_id text,
    compacted_until_ref jsonb,
    first_kept_ref jsonb,
    CONSTRAINT runtime_session_compaction_requests_pkey PRIMARY KEY (id),
    CONSTRAINT runtime_session_compaction_requests_status_check CHECK (
        status = ANY (ARRAY[
            'requested'::text,
            'consumed'::text,
            'completed'::text,
            'noop'::text,
            'failed'::text
        ])
    ),
    CONSTRAINT runtime_session_compaction_requests_requested_mode_check CHECK (
        requested_mode = ANY (ARRAY[
            'next_turn'::text,
            'compact_only'::text
        ])
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_session_compaction_requests_command_receipt
    ON runtime_session_compaction_requests (command_receipt_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_session_compaction_requests_requested_session
    ON runtime_session_compaction_requests (session_id)
    WHERE status = 'requested';

CREATE INDEX IF NOT EXISTS idx_runtime_session_compaction_requests_session_status
    ON runtime_session_compaction_requests (session_id, status, requested_at);

CREATE INDEX IF NOT EXISTS idx_runtime_session_compaction_requests_run_agent
    ON runtime_session_compaction_requests (run_id, agent_id, requested_at DESC);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_compaction_requests_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_compaction_requests
            ADD CONSTRAINT runtime_session_compaction_requests_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_compaction_requests_run_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_compaction_requests
            ADD CONSTRAINT runtime_session_compaction_requests_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_compaction_requests_agent_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_compaction_requests
            ADD CONSTRAINT runtime_session_compaction_requests_agent_id_fkey
            FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'runtime_session_compaction_requests_command_receipt_id_fkey'
    ) THEN
        ALTER TABLE ONLY runtime_session_compaction_requests
            ADD CONSTRAINT runtime_session_compaction_requests_command_receipt_id_fkey
            FOREIGN KEY (command_receipt_id) REFERENCES agent_run_command_receipts(id) ON DELETE CASCADE;
    END IF;
END $$;
