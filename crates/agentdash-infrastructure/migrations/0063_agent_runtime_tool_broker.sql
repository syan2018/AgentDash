-- Platform Tool Broker accepts a call durably before invoking a local callback or MCP tool.
-- The canonical ToolCall item remains owned by Managed Runtime; this table owns broker
-- idempotency and side-effect terminal evidence.

CREATE TABLE agent_runtime_tool_call (
    item_id text PRIMARY KEY,
    thread_id text NOT NULL,
    turn_id text NOT NULL,
    binding_id text NOT NULL,
    binding_generation bigint NOT NULL CHECK (binding_generation >= 0),
    tool_set_revision bigint NOT NULL CHECK (tool_set_revision > 0),
    tool_name text NOT NULL,
    invocation_digest text NOT NULL,
    capability_key text NOT NULL,
    tool_path text NOT NULL,
    channel text NOT NULL CHECK (channel IN ('direct_callback', 'mcp_facade', 'driver_native')),
    status text NOT NULL CHECK (status IN (
        'accepted', 'awaiting_approval', 'running', 'completed', 'failed', 'cancelled', 'timed_out'
    )),
    pending_interaction_id text,
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((status = 'awaiting_approval') = (pending_interaction_id IS NOT NULL)),
    FOREIGN KEY (thread_id, turn_id, item_id)
        REFERENCES agent_runtime_item(thread_id, turn_id, id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, turn_id, pending_interaction_id)
        REFERENCES agent_runtime_interaction(thread_id, turn_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (binding_id, binding_generation)
        REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_runtime_tool_call_recovery
    ON agent_runtime_tool_call (status, updated_at)
    WHERE status IN ('accepted', 'awaiting_approval', 'running');
