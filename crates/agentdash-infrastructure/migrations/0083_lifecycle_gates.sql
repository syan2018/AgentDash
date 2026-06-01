-- Lifecycle Gates: durable 等待机制，替代 in-memory CompanionWaitRegistry。
-- 支持 companion_request(wait=true) 的持久化等待与异步 resolve。

CREATE TABLE IF NOT EXISTS lifecycle_gates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL,
    agent_id UUID NOT NULL,
    gate_kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_pending
    ON lifecycle_gates(run_id, agent_id)
    WHERE status = 'pending';
