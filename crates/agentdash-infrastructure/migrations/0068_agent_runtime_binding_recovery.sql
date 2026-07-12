-- Same-thread Agent Runtime binding recovery keeps RuntimeThread journal/cursor identity stable
-- while retaining every Host binding epoch as immutable lineage.

ALTER TABLE agent_runtime_host_binding
    DROP CONSTRAINT agent_runtime_host_binding_thread_id_key;

CREATE UNIQUE INDEX uq_agent_runtime_host_binding_thread_nonterminal
    ON agent_runtime_host_binding(thread_id)
    WHERE state IN ('pending', 'active', 'desynchronized');

ALTER TABLE agent_runtime_source_coordinate
    DROP CONSTRAINT agent_runtime_source_coordinate_thread_id_key;

CREATE INDEX idx_agent_runtime_source_coordinate_thread
    ON agent_runtime_source_coordinate(thread_id, binding_id);

CREATE TABLE agent_run_runtime_thread_anchor (
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    runtime_thread_id text NOT NULL UNIQUE,
    bootstrap_runtime_binding_id text NOT NULL UNIQUE
        REFERENCES agent_runtime_host_binding(binding_id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, agent_id)
);

CREATE TABLE agent_run_runtime_binding_lineage (
    run_id text NOT NULL,
    agent_id text NOT NULL,
    binding_epoch bigint NOT NULL CHECK (binding_epoch > 0),
    runtime_binding_id text NOT NULL UNIQUE
        REFERENCES agent_runtime_host_binding(binding_id) ON DELETE RESTRICT,
    binding jsonb NOT NULL,
    recovery_intent_id text,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, agent_id, binding_epoch),
    FOREIGN KEY (run_id, agent_id)
        REFERENCES agent_run_runtime_thread_anchor(run_id, agent_id) ON DELETE CASCADE
);

INSERT INTO agent_run_runtime_thread_anchor (
    run_id, agent_id, runtime_thread_id, bootstrap_runtime_binding_id, created_at
)
SELECT run_id, agent_id, runtime_thread_id, runtime_binding_id, created_at
FROM agent_run_runtime_binding;

INSERT INTO agent_run_runtime_binding_lineage (
    run_id, agent_id, binding_epoch, runtime_binding_id, binding, created_at
)
SELECT run_id, agent_id, 1, runtime_binding_id,
       jsonb_set(binding, '{binding_epoch}', '1'::jsonb, true), created_at
FROM agent_run_runtime_binding;

-- RuntimeThread projections created before binding epochs existed belong to the
-- bootstrap binding. Keep the persisted JSON ready for strict typed decoding.
UPDATE agent_runtime_thread
SET projection = jsonb_set(projection, '{binding_epoch}', '1'::jsonb, true)
WHERE NOT projection ? 'binding_epoch';

CREATE TABLE agent_run_runtime_recovery_intent (
    id text PRIMARY KEY,
    run_id text NOT NULL,
    agent_id text NOT NULL,
    runtime_thread_id text NOT NULL,
    expected_old_binding_id text NOT NULL,
    expected_old_generation bigint NOT NULL CHECK (expected_old_generation > 0),
    expected_runtime_revision bigint NOT NULL CHECK (expected_runtime_revision >= 0),
    binding_epoch bigint NOT NULL CHECK (binding_epoch > 1),
    proposed_binding_id text NOT NULL UNIQUE,
    selected_offer_id text NOT NULL REFERENCES agent_runtime_offer(id) ON DELETE RESTRICT,
    source_thread_id text NOT NULL,
    state text NOT NULL CHECK (state IN ('prepared', 'host_bound', 'committed', 'failed')),
    failure_reason text,
    intent jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (run_id, agent_id)
        REFERENCES agent_run_runtime_thread_anchor(run_id, agent_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_agent_run_runtime_recovery_intent_active
    ON agent_run_runtime_recovery_intent(run_id, agent_id)
    WHERE state IN ('prepared', 'host_bound');

ALTER TABLE agent_run_runtime_binding_lineage
    ADD CONSTRAINT agent_run_runtime_binding_lineage_recovery_intent_fkey
    FOREIGN KEY (recovery_intent_id)
    REFERENCES agent_run_runtime_recovery_intent(id) ON DELETE RESTRICT;

DROP TABLE agent_run_runtime_binding;
