CREATE TABLE agent_run_product_command_receipts (
    id text PRIMARY KEY,
    scope_kind text NOT NULL,
    scope_key text NOT NULL,
    command_kind text NOT NULL CHECK (
        command_kind IN (
            'message_submit',
            'project_agent_start',
            'agent_run_fork',
            'agent_run_fork_submit',
            'mailbox_promote',
            'mailbox_delete',
            'mailbox_move',
            'mailbox_resume',
            'cancel',
            'context_compact'
        )
    ),
    client_command_id text NOT NULL,
    request_digest text NOT NULL,
    status text NOT NULL CHECK (status IN ('pending', 'accepted', 'terminal_failed')),
    mailbox_message_id text REFERENCES agent_run_mailbox_messages(id) ON DELETE SET NULL,
    run_id text REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    frame_id text REFERENCES agent_frames(id) ON DELETE SET NULL,
    frame_revision integer,
    runtime_thread_id text REFERENCES agent_runtime_thread(id) ON DELETE SET NULL,
    runtime_operation_id text,
    result_json jsonb,
    error_message text,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    accepted_at timestamptz,
    failed_at timestamptz,
    UNIQUE (scope_kind, scope_key, client_command_id),
    FOREIGN KEY (runtime_thread_id, runtime_operation_id)
        REFERENCES agent_runtime_operation(thread_id, id) ON DELETE SET NULL,
    CHECK (runtime_operation_id IS NULL OR runtime_thread_id IS NOT NULL)
);

CREATE INDEX idx_agent_run_product_command_receipts_scope
    ON agent_run_product_command_receipts(scope_kind, scope_key, updated_at);

CREATE INDEX idx_agent_run_product_command_receipts_run_agent
    ON agent_run_product_command_receipts(run_id, agent_id, updated_at);

CREATE INDEX idx_agent_run_product_command_receipts_mailbox_message
    ON agent_run_product_command_receipts(mailbox_message_id);

CREATE INDEX idx_agent_run_product_command_receipts_runtime_operation
    ON agent_run_product_command_receipts(runtime_thread_id, runtime_operation_id);
