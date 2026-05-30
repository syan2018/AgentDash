-- Permission Grants table for the Agent Permission System.
-- Tracks capability grant requests, policy decisions, and lifecycle.

CREATE TABLE IF NOT EXISTS permission_grants (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source_turn_id TEXT,
    source_tool_call_id TEXT,
    requested_paths JSONB NOT NULL,
    reason TEXT NOT NULL,
    grant_scope TEXT NOT NULL,
    expires_at TIMESTAMPTZ,
    scope_escalation_intent JSONB,
    status TEXT NOT NULL DEFAULT 'created',
    policy_decision JSONB,
    approved_by TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_permission_grants_session_active
    ON permission_grants(session_id)
    WHERE status IN ('applied', 'scope_escalated');

CREATE INDEX IF NOT EXISTS idx_permission_grants_run
    ON permission_grants(run_id);

CREATE INDEX IF NOT EXISTS idx_permission_grants_status
    ON permission_grants(status)
    WHERE status IN ('applied', 'scope_escalated', 'pending_user_approval');
