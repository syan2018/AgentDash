DO $$
BEGIN
    IF to_regclass('agent_run_delivery_command_receipts') IS NOT NULL
        AND to_regclass('agent_run_command_receipts') IS NULL
    THEN
        ALTER TABLE agent_run_delivery_command_receipts
            RENAME TO agent_run_command_receipts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'agent_run_command_receipts'
          AND column_name = 'turn_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'agent_run_command_receipts'
          AND column_name = 'agent_run_turn_id'
    ) THEN
        ALTER TABLE agent_run_command_receipts
            RENAME COLUMN turn_id TO agent_run_turn_id;
    END IF;
END $$;

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD COLUMN IF NOT EXISTS command_kind text NOT NULL DEFAULT 'message_submit';

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD COLUMN IF NOT EXISTS mailbox_message_id text;

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD COLUMN IF NOT EXISTS protocol_turn_id text;

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD COLUMN IF NOT EXISTS result_json jsonb;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_command_receipts_command_kind_check'
    ) THEN
        ALTER TABLE ONLY agent_run_command_receipts
            ADD CONSTRAINT agent_run_command_receipts_command_kind_check CHECK (
                command_kind = ANY (ARRAY[
                    'message_submit'::text,
                    'project_agent_start'::text,
                    'mailbox_promote'::text,
                    'mailbox_delete'::text,
                    'mailbox_resume'::text,
                    'cancel'::text
                ])
            );
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_agent_run_command_receipts_mailbox_message
    ON agent_run_command_receipts USING btree (mailbox_message_id);

CREATE TABLE IF NOT EXISTS agent_run_mailbox_messages (
    id text NOT NULL,
    run_id text NOT NULL,
    agent_id text NOT NULL,
    runtime_session_id text NOT NULL,
    origin text NOT NULL,
    source text NOT NULL,
    delivery text NOT NULL,
    delivery_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    barrier text NOT NULL,
    drain_mode text NOT NULL,
    status text NOT NULL,
    priority integer NOT NULL DEFAULT 0,
    order_key bigint NOT NULL,
    source_dedup_key text,
    queued_agent_run_turn_id text,
    consuming_agent_run_turn_id text,
    expected_active_agent_run_turn_id text,
    accepted_agent_run_turn_id text,
    accepted_protocol_turn_id text,
    claim_token text,
    claimed_at timestamp with time zone,
    claim_expires_at timestamp with time zone,
    command_receipt_id text,
    payload_json jsonb,
    executor_config_json jsonb,
    preview text NOT NULL DEFAULT '',
    has_images boolean NOT NULL DEFAULT false,
    retain_payload boolean NOT NULL DEFAULT false,
    attempt_count integer NOT NULL DEFAULT 0,
    last_error text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    deleted_at timestamp with time zone,
    CONSTRAINT agent_run_mailbox_messages_origin_check CHECK (
        origin = ANY (ARRAY['user'::text, 'system'::text, 'hook'::text, 'companion'::text, 'workflow'::text])
    ),
    CONSTRAINT agent_run_mailbox_messages_delivery_check CHECK (
        delivery = ANY (ARRAY['launch_or_continue_turn'::text, 'steer_active_turn'::text, 'resume_launch_source'::text])
    ),
    CONSTRAINT agent_run_mailbox_messages_barrier_check CHECK (
        barrier = ANY (ARRAY['immediate_if_idle'::text, 'agent_loop_turn_boundary'::text, 'agent_run_turn_boundary'::text, 'manual_resume'::text])
    ),
    CONSTRAINT agent_run_mailbox_messages_drain_mode_check CHECK (
        drain_mode = ANY (ARRAY['one'::text, 'all'::text])
    ),
    CONSTRAINT agent_run_mailbox_messages_status_check CHECK (
        status = ANY (ARRAY[
            'accepted'::text,
            'queued'::text,
            'ready_to_consume'::text,
            'consuming'::text,
            'dispatched'::text,
            'steered'::text,
            'paused'::text,
            'blocked'::text,
            'failed'::text,
            'deleted'::text
        ])
    )
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_pkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_pkey PRIMARY KEY (id);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_source_check'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_source_check CHECK (
                source = ANY (ARRAY[
                    'composer'::text,
                    'draft_start'::text,
                    'hook_after_turn'::text,
                    'hook_before_stop'::text,
                    'hook_auto_resume'::text,
                    'companion_parent_resume'::text,
                    'workflow_orchestrator'::text,
                    'routine_executor'::text,
                    'local_relay_prompt'::text
                ])
            );
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_source_dedup
    ON agent_run_mailbox_messages USING btree (run_id, agent_id, source_dedup_key)
    WHERE source_dedup_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_run_agent_order
    ON agent_run_mailbox_messages USING btree (run_id, agent_id, priority DESC, order_key ASC);

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_runtime_status
    ON agent_run_mailbox_messages USING btree (runtime_session_id, status, barrier, drain_mode);

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_claim
    ON agent_run_mailbox_messages USING btree (status, claim_expires_at);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_run_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_agent_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_agent_id_fkey
            FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_messages_command_receipt_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_command_receipt_id_fkey
            FOREIGN KEY (command_receipt_id) REFERENCES agent_run_command_receipts(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_command_receipts_mailbox_message_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_command_receipts
            ADD CONSTRAINT agent_run_command_receipts_mailbox_message_id_fkey
            FOREIGN KEY (mailbox_message_id) REFERENCES agent_run_mailbox_messages(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS agent_run_mailbox_states (
    run_id text NOT NULL,
    agent_id text NOT NULL,
    runtime_session_id text NOT NULL,
    paused boolean NOT NULL DEFAULT false,
    pause_reason text,
    pause_message text,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT agent_run_mailbox_states_pkey PRIMARY KEY (run_id, agent_id)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_states_run_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_states
            ADD CONSTRAINT agent_run_mailbox_states_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES lifecycle_runs(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_states_agent_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_states
            ADD CONSTRAINT agent_run_mailbox_states_agent_id_fkey
            FOREIGN KEY (agent_id) REFERENCES lifecycle_agents(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_run_mailbox_states_runtime_session_id_fkey'
    ) THEN
        ALTER TABLE ONLY agent_run_mailbox_states
            ADD CONSTRAINT agent_run_mailbox_states_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES sessions(id) ON DELETE CASCADE;
    END IF;
END $$;
