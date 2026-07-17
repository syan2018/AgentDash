-- Immutable materializations and the product-to-runtime anchor used by the AgentRun facade.
-- The same migration is the WP08 cutover boundary; legacy RuntimeSession objects are removed
-- after every production reader has moved to these canonical coordinates.

CREATE TABLE agent_runtime_surface_snapshot (
    binding_id text PRIMARY KEY,
    surface_revision bigint NOT NULL CHECK (surface_revision > 0),
    surface_digest text NOT NULL,
    tool_set_revision bigint NOT NULL CHECK (tool_set_revision > 0),
    tool_set_digest text NOT NULL,
    hook_plan_revision bigint NOT NULL CHECK (hook_plan_revision > 0),
    hook_plan_digest text NOT NULL,
    materialized jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (binding_id, surface_revision, surface_digest),
    UNIQUE (binding_id, tool_set_revision, tool_set_digest)
);

CREATE TABLE agent_run_runtime_binding (
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    runtime_thread_id text NOT NULL UNIQUE,
    runtime_binding_id text NOT NULL UNIQUE
        REFERENCES agent_runtime_host_binding(binding_id) ON DELETE RESTRICT,
    binding jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, agent_id),
    UNIQUE (run_id, agent_id, runtime_thread_id)
);

CREATE INDEX idx_agent_run_runtime_binding_thread
    ON agent_run_runtime_binding(runtime_thread_id);

ALTER TABLE agent_run_mailbox_messages
    ADD COLUMN accepted_runtime_operation_id text
        REFERENCES agent_runtime_operation(id) ON DELETE SET NULL;

ALTER TABLE agent_run_mailbox_messages
    DROP COLUMN command_receipt_id,
    DROP COLUMN delivery_runtime_session_id,
    DROP COLUMN queued_agent_run_turn_id,
    DROP COLUMN consuming_agent_run_turn_id,
    DROP COLUMN expected_active_agent_run_turn_id,
    DROP COLUMN accepted_agent_run_turn_id,
    DROP COLUMN accepted_protocol_turn_id;

ALTER TABLE agent_run_mailbox_states
    DROP COLUMN delivery_runtime_session_id;

ALTER TABLE permission_grants
    DROP COLUMN source_runtime_session_id,
    ADD COLUMN source_runtime_operation_id text
        REFERENCES agent_runtime_operation(id) ON DELETE RESTRICT;

ALTER TABLE gate_result_delivery_markers
    RENAME COLUMN command_receipt_id TO accepted_runtime_operation_id;

DROP TABLE agent_run_delivery_bindings;
DROP TABLE runtime_session_compaction_requests;
DROP TABLE agent_run_command_receipts;
DROP TABLE runtime_session_execution_anchors;
DROP TABLE runtime_session_delivery_commands;
DROP TABLE runtime_session_projection_segments;
DROP TABLE runtime_session_projection_heads;
DROP TABLE runtime_session_lineage;
DROP TABLE runtime_session_compactions;
DROP TABLE runtime_session_events;
DROP TABLE runtime_sessions;
