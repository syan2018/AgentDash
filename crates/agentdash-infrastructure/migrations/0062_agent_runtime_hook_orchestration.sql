-- Managed Runtime owns adopted Hook plans, canonical HookRun projections, and durable typed
-- effects. Business Surface compiles plans; adapters execute only routes assigned to them.

CREATE TABLE agent_runtime_hook_plan (
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    revision bigint NOT NULL CHECK (revision > 0),
    digest text NOT NULL,
    binding jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id, revision),
    UNIQUE (thread_id, revision, digest)
);

ALTER TABLE agent_runtime_item
    ADD CONSTRAINT uq_agent_runtime_item_hook_coordinate
    UNIQUE (thread_id, turn_id, id);

ALTER TABLE agent_runtime_interaction
    ADD CONSTRAINT uq_agent_runtime_interaction_hook_coordinate
    UNIQUE (thread_id, turn_id, id);

CREATE TABLE agent_runtime_hook_run (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    definition_id text NOT NULL,
    point text NOT NULL,
    plan_revision bigint NOT NULL CHECK (plan_revision > 0),
    plan_digest text NOT NULL,
    operation_id text,
    turn_id text,
    item_id text,
    interaction_id text,
    status text NOT NULL CHECK (status IN (
        'accepted', 'running', 'completed', 'blocked', 'failed', 'stopped', 'cancelled'
    )),
    record jsonb NOT NULL,
    recovery_attempt_count integer NOT NULL DEFAULT 0 CHECK (recovery_attempt_count >= 0),
    recovery_claim_token text,
    recovery_claim_owner text,
    recovery_claim_expires_at_ms bigint,
    recovery_last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thread_id, id),
    FOREIGN KEY (thread_id, plan_revision, plan_digest)
        REFERENCES agent_runtime_hook_plan(thread_id, revision, digest) ON DELETE RESTRICT,
    FOREIGN KEY (thread_id, operation_id)
        REFERENCES agent_runtime_operation(thread_id, id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, turn_id)
        REFERENCES agent_runtime_turn(thread_id, id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, turn_id, item_id)
        REFERENCES agent_runtime_item(thread_id, turn_id, id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, turn_id, interaction_id)
        REFERENCES agent_runtime_interaction(thread_id, turn_id, id) ON DELETE CASCADE,
    CHECK (item_id IS NULL OR turn_id IS NOT NULL),
    CHECK (interaction_id IS NULL OR turn_id IS NOT NULL),
    CHECK ((recovery_claim_token IS NULL) = (recovery_claim_owner IS NULL)),
    CHECK ((recovery_claim_token IS NULL) = (recovery_claim_expires_at_ms IS NULL))
);

CREATE INDEX idx_agent_runtime_hook_run_recovery
    ON agent_runtime_hook_run (status, recovery_claim_expires_at_ms, updated_at)
    WHERE status IN ('accepted', 'running');

CREATE TABLE agent_runtime_hook_effect (
    id text PRIMARY KEY,
    hook_run_id text NOT NULL REFERENCES agent_runtime_hook_run(id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    idempotency_key text NOT NULL,
    effect_type text NOT NULL,
    schema_version integer NOT NULL CHECK (schema_version > 0),
    target_authority text NOT NULL,
    retry_limit integer NOT NULL CHECK (retry_limit >= 0),
    payload_digest text NOT NULL,
    record jsonb NOT NULL,
    dispatched_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_token text,
    claim_owner text,
    claim_expires_at_ms bigint,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thread_id, id),
    UNIQUE (hook_run_id, idempotency_key),
    FOREIGN KEY (thread_id, hook_run_id)
        REFERENCES agent_runtime_hook_run(thread_id, id) ON DELETE CASCADE,
    CHECK ((claim_token IS NULL) = (claim_owner IS NULL)),
    CHECK ((claim_token IS NULL) = (claim_expires_at_ms IS NULL)),
    CHECK (attempt_count <= retry_limit + 1)
);

CREATE INDEX idx_agent_runtime_hook_effect_claim
    ON agent_runtime_hook_effect (dispatched_at, claim_expires_at_ms, created_at);
