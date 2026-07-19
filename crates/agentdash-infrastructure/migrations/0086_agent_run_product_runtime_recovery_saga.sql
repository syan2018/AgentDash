-- Product-owned durable recovery for an AgentRun binding after a Complete-Agent Host generation
-- changes. The immutable saga payload freezes Runtime command identities and revision fences
-- before any external effect is dispatched, so background replay does not depend on Host Lost
-- discovery remaining observable.

CREATE TABLE agent_run_product_runtime_recovery_saga (
    recovery_id TEXT PRIMARY KEY CHECK (btrim(recovery_id) <> ''),
    target_run_id UUID NOT NULL,
    target_agent_id UUID NOT NULL,
    client_command_id TEXT NOT NULL CHECK (
        btrim(client_command_id) <> '' AND length(client_command_id) <= 256
    ),
    runtime_thread_id TEXT NOT NULL CHECK (btrim(runtime_thread_id) <> ''),
    phase TEXT NOT NULL CHECK (
        phase IN (
            'requested',
            'rebind_applied',
            'product_binding_prepared',
            'resource_materialized',
            'runtime_activated',
            'succeeded'
        )
    ),
    version BIGINT NOT NULL CHECK (version > 0),
    saga JSONB NOT NULL CHECK (jsonb_typeof(saga) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (target_run_id, target_agent_id, client_command_id),
    FOREIGN KEY (target_run_id, target_agent_id)
        REFERENCES agent_run_product_runtime_binding(target_run_id, target_agent_id)
        ON DELETE CASCADE,
    FOREIGN KEY (runtime_thread_id)
        REFERENCES agent_runtime_thread_binding(thread_id) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_run_product_runtime_recovery_recoverable
    ON agent_run_product_runtime_recovery_saga(updated_at, recovery_id)
    WHERE phase <> 'succeeded';
