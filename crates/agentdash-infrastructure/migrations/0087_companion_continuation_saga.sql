CREATE TABLE companion_continuation_saga (
    request_id UUID PRIMARY KEY,
    dispatch_id TEXT NOT NULL CHECK (btrim(dispatch_id) <> ''),
    runtime_protocol_request_id UUID NOT NULL,
    child_run_id UUID NOT NULL,
    child_agent_id UUID NOT NULL,
    runtime_thread_id TEXT NOT NULL CHECK (btrim(runtime_thread_id) <> ''),
    phase TEXT NOT NULL CHECK (
        phase IN (
            'requested',
            'runtime_ready',
            'first_input_converged',
            'gate_converged',
            'channel_converged',
            'task_converged',
            'succeeded'
        )
    ),
    version BIGINT NOT NULL CHECK (version >= 1),
    saga JSONB NOT NULL CHECK (jsonb_typeof(saga) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (dispatch_id),
    UNIQUE (runtime_protocol_request_id)
);

CREATE INDEX idx_companion_continuation_saga_recovery
    ON companion_continuation_saga (updated_at, request_id)
    WHERE phase <> 'succeeded';
