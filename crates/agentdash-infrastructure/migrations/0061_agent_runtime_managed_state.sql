-- AgentDash-owned Managed Runtime facts. Driver Host seeds binding/source coordinates before
-- Runtime creates a thread, so persistence can reject unbound or stale-generation commits.

CREATE TABLE agent_runtime_thread (
    id text PRIMARY KEY,
    revision bigint NOT NULL CHECK (revision >= 0),
    next_event_sequence bigint NOT NULL CHECK (next_event_sequence >= 0),
    next_operation_sequence bigint NOT NULL CHECK (next_operation_sequence >= 0),
    status text NOT NULL CHECK (status IN ('active', 'suspended', 'desynchronized', 'closed', 'lost')),
    active_turn_id text,
    binding_id text NOT NULL,
    driver_generation bigint NOT NULL CHECK (driver_generation >= 0),
    source_thread_id text NOT NULL,
    profile_digest text NOT NULL,
    active_checkpoint_id text,
    context_revision bigint NOT NULL CHECK (context_revision >= 0),
    settings_revision bigint NOT NULL CHECK (settings_revision >= 0),
    tool_set_revision bigint NOT NULL CHECK (tool_set_revision >= 0),
    projection jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (binding_id, source_thread_id),
    UNIQUE (id, driver_generation)
);

CREATE TABLE agent_runtime_binding (
    id text PRIMARY KEY,
    driver_generation bigint NOT NULL CHECK (driver_generation >= 0),
    profile_digest text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, driver_generation)
);

CREATE TABLE agent_runtime_source_coordinate (
    binding_id text NOT NULL REFERENCES agent_runtime_binding(id) ON DELETE CASCADE,
    source_thread_id text NOT NULL,
    thread_id text NOT NULL,
    PRIMARY KEY (binding_id, source_thread_id),
    UNIQUE (thread_id),
    UNIQUE (binding_id, source_thread_id, thread_id)
);

ALTER TABLE agent_runtime_thread
    ADD CONSTRAINT agent_runtime_thread_binding_generation_fkey
    FOREIGN KEY (binding_id, driver_generation)
    REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE RESTRICT,
    ADD CONSTRAINT agent_runtime_thread_source_coordinate_fkey
    FOREIGN KEY (binding_id, source_thread_id, id)
    REFERENCES agent_runtime_source_coordinate(binding_id, source_thread_id, thread_id)
    ON DELETE RESTRICT;

CREATE TABLE agent_runtime_operation (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    operation_sequence bigint NOT NULL CHECK (operation_sequence > 0),
    idempotency_key text NOT NULL,
    accepted_revision bigint NOT NULL CHECK (accepted_revision >= 0),
    status text NOT NULL CHECK (status IN ('active', 'succeeded', 'failed', 'lost')),
    actor jsonb NOT NULL,
    command jsonb NOT NULL,
    terminal jsonb,
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thread_id, operation_sequence),
    UNIQUE (thread_id, id),
    UNIQUE (thread_id, idempotency_key),
    CHECK ((status = 'active') = (terminal IS NULL))
);

CREATE TABLE agent_runtime_event (
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    event_sequence bigint NOT NULL CHECK (event_sequence > 0),
    revision bigint NOT NULL CHECK (revision >= 0),
    event_kind text NOT NULL,
    envelope jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id, event_sequence)
);

CREATE TABLE agent_runtime_turn (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    phase text NOT NULL CHECK (phase IN ('active', 'terminal')),
    state jsonb NOT NULL,
    UNIQUE (thread_id, id)
);

CREATE TABLE agent_runtime_item (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    turn_id text NOT NULL,
    sort_order bigint NOT NULL CHECK (sort_order >= 0),
    phase text NOT NULL CHECK (phase IN ('active', 'terminal')),
    state jsonb NOT NULL,
    UNIQUE (thread_id, sort_order),
    FOREIGN KEY (thread_id, turn_id)
        REFERENCES agent_runtime_turn(thread_id, id) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_interaction (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    turn_id text NOT NULL,
    phase text NOT NULL CHECK (phase IN ('active', 'terminal')),
    state jsonb NOT NULL,
    FOREIGN KEY (thread_id, turn_id)
        REFERENCES agent_runtime_turn(thread_id, id) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_outbox (
    operation_id text PRIMARY KEY REFERENCES agent_runtime_operation(id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    driver_generation bigint NOT NULL CHECK (driver_generation >= 0),
    payload jsonb NOT NULL,
    dispatched_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_token text,
    claim_owner text,
    claim_expires_at_ms bigint,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((claim_token IS NULL) = (claim_owner IS NULL)),
    CHECK ((claim_token IS NULL) = (claim_expires_at_ms IS NULL))
);

ALTER TABLE agent_runtime_outbox
    ADD CONSTRAINT agent_runtime_outbox_thread_generation_fkey
    FOREIGN KEY (thread_id, driver_generation)
    REFERENCES agent_runtime_thread(id, driver_generation) ON DELETE CASCADE;

CREATE INDEX idx_agent_runtime_outbox_claim
    ON agent_runtime_outbox (dispatched_at, claim_expires_at_ms, created_at);

CREATE TABLE agent_runtime_quarantine (
    id bigserial PRIMARY KEY,
    thread_id text,
    binding_id text,
    driver_generation bigint,
    reason_kind text NOT NULL,
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE agent_context_checkpoint (
    id text PRIMARY KEY,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    revision bigint NOT NULL CHECK (revision > 0),
    digest text NOT NULL,
    fidelity text NOT NULL CHECK (fidelity IN ('opaque', 'event_projected', 'agent_replay', 'driver_exact', 'platform_exact')),
    settings_revision bigint NOT NULL CHECK (settings_revision >= 0),
    tool_set_revision bigint NOT NULL CHECK (tool_set_revision >= 0),
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thread_id, revision),
    UNIQUE (thread_id, id),
    UNIQUE (thread_id, id, revision, digest, fidelity, settings_revision, tool_set_revision)
);

ALTER TABLE agent_runtime_thread
    ADD CONSTRAINT agent_runtime_thread_active_checkpoint_fkey
    FOREIGN KEY (id, active_checkpoint_id)
    REFERENCES agent_context_checkpoint(thread_id, id) DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE agent_context_preparation (
    compaction_id text PRIMARY KEY,
    operation_id text NOT NULL UNIQUE REFERENCES agent_runtime_operation(id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    trigger_kind text NOT NULL CHECK (trigger_kind IN ('manual', 'automatic')),
    expected_base_checkpoint_id text REFERENCES agent_context_checkpoint(id),
    expected_base_revision bigint NOT NULL CHECK (expected_base_revision >= 0),
    status text NOT NULL CHECK (status IN ('pending', 'prepared', 'terminal')),
    record jsonb NOT NULL,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_token text,
    claim_owner text,
    claim_expires_at_ms bigint,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((claim_token IS NULL) = (claim_owner IS NULL)),
    CHECK ((claim_token IS NULL) = (claim_expires_at_ms IS NULL)),
    UNIQUE (compaction_id, thread_id, operation_id),
    FOREIGN KEY (thread_id, operation_id)
        REFERENCES agent_runtime_operation(thread_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_agent_context_preparation_active_thread
    ON agent_context_preparation (thread_id)
    WHERE status IN ('pending', 'prepared');

CREATE INDEX idx_agent_context_preparation_claim
    ON agent_context_preparation (status, claim_expires_at_ms, created_at);

CREATE TABLE agent_context_candidate (
    id text PRIMARY KEY,
    compaction_id text NOT NULL UNIQUE REFERENCES agent_context_preparation(compaction_id) ON DELETE CASCADE,
    operation_id text NOT NULL REFERENCES agent_runtime_operation(id) ON DELETE CASCADE,
    activation_id text NOT NULL UNIQUE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    checkpoint_id text NOT NULL UNIQUE REFERENCES agent_context_checkpoint(id) ON DELETE RESTRICT,
    expected_base_checkpoint_id text REFERENCES agent_context_checkpoint(id),
    expected_base_revision bigint NOT NULL CHECK (expected_base_revision >= 0),
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, compaction_id, thread_id),
    FOREIGN KEY (compaction_id, thread_id, operation_id)
        REFERENCES agent_context_preparation(compaction_id, thread_id, operation_id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, checkpoint_id)
        REFERENCES agent_context_checkpoint(thread_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (thread_id, expected_base_checkpoint_id)
        REFERENCES agent_context_checkpoint(thread_id, id) ON DELETE RESTRICT
);

CREATE TABLE agent_context_activation (
    id text PRIMARY KEY,
    candidate_id text NOT NULL UNIQUE REFERENCES agent_context_candidate(id) ON DELETE CASCADE,
    compaction_id text NOT NULL UNIQUE REFERENCES agent_context_preparation(compaction_id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    status text NOT NULL CHECK (status IN ('prepared', 'applied', 'terminal')),
    applied_digest text,
    driver_context_revision text,
    record jsonb NOT NULL,
    recovery_attempt_count integer NOT NULL DEFAULT 0 CHECK (recovery_attempt_count >= 0),
    recovery_claim_token text,
    recovery_claim_owner text,
    recovery_claim_expires_at_ms bigint,
    recovery_last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((status = 'applied') = (applied_digest IS NOT NULL AND driver_context_revision IS NOT NULL)
           OR status = 'terminal'),
    CHECK ((recovery_claim_token IS NULL) = (recovery_claim_owner IS NULL)),
    CHECK ((recovery_claim_token IS NULL) = (recovery_claim_expires_at_ms IS NULL)),
    UNIQUE (id, candidate_id, compaction_id, thread_id),
    FOREIGN KEY (candidate_id, compaction_id, thread_id)
        REFERENCES agent_context_candidate(id, compaction_id, thread_id) ON DELETE CASCADE
);

ALTER TABLE agent_context_candidate
    ADD CONSTRAINT agent_context_candidate_activation_fkey
    FOREIGN KEY (activation_id) REFERENCES agent_context_activation(id) DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_agent_context_activation_recovery_claim
    ON agent_context_activation (status, recovery_claim_expires_at_ms, updated_at);

CREATE TABLE agent_context_activation_dispatch (
    activation_id text PRIMARY KEY REFERENCES agent_context_activation(id) ON DELETE CASCADE,
    candidate_id text NOT NULL UNIQUE REFERENCES agent_context_candidate(id) ON DELETE CASCADE,
    compaction_id text NOT NULL UNIQUE REFERENCES agent_context_preparation(compaction_id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    binding_id text NOT NULL REFERENCES agent_runtime_binding(id) ON DELETE CASCADE,
    driver_generation bigint NOT NULL CHECK (driver_generation >= 0),
    checkpoint_id text NOT NULL REFERENCES agent_context_checkpoint(id) ON DELETE RESTRICT,
    digest text NOT NULL,
    payload jsonb NOT NULL,
    dispatched_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    claim_token text,
    claim_owner text,
    claim_expires_at_ms bigint,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((claim_token IS NULL) = (claim_owner IS NULL)),
    CHECK ((claim_token IS NULL) = (claim_expires_at_ms IS NULL)),
    FOREIGN KEY (activation_id, candidate_id, compaction_id, thread_id)
        REFERENCES agent_context_activation(id, candidate_id, compaction_id, thread_id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id, checkpoint_id)
        REFERENCES agent_context_checkpoint(thread_id, id) ON DELETE RESTRICT
);

ALTER TABLE agent_context_activation_dispatch
    ADD CONSTRAINT agent_context_activation_dispatch_binding_generation_fkey
    FOREIGN KEY (binding_id, driver_generation)
    REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE RESTRICT;

CREATE INDEX idx_agent_context_activation_dispatch_claim
    ON agent_context_activation_dispatch (dispatched_at, claim_expires_at_ms, created_at);

CREATE TABLE agent_context_head (
    thread_id text PRIMARY KEY REFERENCES agent_runtime_thread(id) ON DELETE CASCADE,
    checkpoint_id text NOT NULL UNIQUE REFERENCES agent_context_checkpoint(id) ON DELETE RESTRICT,
    revision bigint NOT NULL CHECK (revision > 0),
    digest text NOT NULL,
    fidelity text NOT NULL CHECK (fidelity IN ('event_projected', 'agent_replay', 'driver_exact', 'platform_exact')),
    settings_revision bigint NOT NULL CHECK (settings_revision >= 0),
    tool_set_revision bigint NOT NULL CHECK (tool_set_revision >= 0),
    provenance jsonb NOT NULL,
    record jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (thread_id, revision),
    FOREIGN KEY (thread_id, checkpoint_id, revision, digest, fidelity, settings_revision, tool_set_revision)
        REFERENCES agent_context_checkpoint(thread_id, id, revision, digest, fidelity, settings_revision, tool_set_revision)
        ON DELETE RESTRICT
);
