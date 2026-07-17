-- Runtime terminal application effect 与权威 turn_terminal presentation record 在同一事务产生。
-- 专用 typed outbox 让 application worker 可以重试副作用，而不必从最新 Runtime 或
-- AgentRun 状态反向猜测 terminal identity。

CREATE TABLE agent_runtime_terminal_application_effect_outbox (
    effect_id text PRIMARY KEY,
    runtime_thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    terminal_event_sequence bigint NOT NULL CHECK (terminal_event_sequence > 0),
    record jsonb NOT NULL,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_token text,
    claim_owner text,
    claim_expires_at_ms bigint,
    completed_at timestamptz,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (runtime_thread_id, terminal_event_sequence),
    FOREIGN KEY (runtime_thread_id, terminal_event_sequence)
        REFERENCES agent_runtime_event(thread_id, event_sequence) ON DELETE CASCADE,
    CHECK ((claim_token IS NULL) = (claim_owner IS NULL)),
    CHECK ((claim_token IS NULL) = (claim_expires_at_ms IS NULL))
);

CREATE INDEX idx_agent_runtime_terminal_application_effect_claim
    ON agent_runtime_terminal_application_effect_outbox
    (completed_at, claim_expires_at_ms, created_at);
