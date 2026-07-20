-- A Complete Agent attachment is callable only inside the Host incarnation that materialized it.
-- Durable execution facts therefore freeze the exact attachment target while live descriptor,
-- verification, offer, placement availability, and service handles remain process-local.

ALTER TABLE agent_run_product_runtime_binding
    DROP CONSTRAINT IF EXISTS agent_run_product_runtime_binding_host_generation_fkey;

-- Product Runtime bindings and recovery sagas refer to the Host graph being hard-cut. Product
-- profiles, Provider configuration, and the canonical Managed Runtime journal are independent
-- authorities and remain intact.
DELETE FROM agent_run_product_runtime_binding;

ALTER TABLE agent_run_product_runtime_binding
    ADD COLUMN execution_profile JSONB NOT NULL
        CHECK (jsonb_typeof(execution_profile) = 'object');

DROP TABLE agent_runtime_callback_outcome;
DROP TABLE agent_runtime_callback_reservation;
DROP TABLE agent_runtime_callback_route_tombstone;
DROP TABLE agent_runtime_effect_attempt_history;
DROP TABLE agent_runtime_lease;
DROP TABLE agent_runtime_lease_epoch;
DROP TABLE agent_runtime_effect;
DROP TABLE agent_runtime_callback_route;
DROP TABLE agent_runtime_source_coordinate;
DROP TABLE agent_runtime_lifecycle_effect;
DROP TABLE agent_runtime_binding;
DROP TABLE agent_runtime_lifecycle_target;

DROP TABLE agent_runtime_remote_binding;
DROP TABLE agent_runtime_placement;
DROP TABLE agent_runtime_offer;
DROP TABLE agent_service_verification;
DROP TABLE agent_service_instance;

UPDATE agent_runtime_host_revision
SET revision = 0,
    facts = '{
        "bindings": {},
        "source_coordinates": {},
        "callback_routes": {},
        "revoked_callback_routes": [],
        "effects": {},
        "leases": {},
        "lease_epochs": {},
        "runtime_targets": {},
        "runtime_target_provisionings": {},
        "runtime_target_recoveries": {},
        "lifecycle_effects": {}
    }'::JSONB
WHERE singleton = TRUE;

UPDATE agent_runtime_callback_revision
SET revision = 0,
    facts = '{"callbacks": []}'::JSONB
WHERE singleton = TRUE;

CREATE TABLE agent_runtime_lifecycle_target (
    runtime_thread_id TEXT PRIMARY KEY CHECK (btrim(runtime_thread_id) <> ''),
    logical_instance_id TEXT NOT NULL CHECK (btrim(logical_instance_id) <> ''),
    live_attachment_id TEXT NOT NULL CHECK (btrim(live_attachment_id) <> ''),
    host_incarnation_id TEXT NOT NULL CHECK (btrim(host_incarnation_id) <> ''),
    definition_id TEXT NOT NULL CHECK (btrim(definition_id) <> ''),
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    profile_digest TEXT NOT NULL CHECK (btrim(profile_digest) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    target_snapshot JSONB NOT NULL CHECK (jsonb_typeof(target_snapshot) = 'object'),
    target JSONB NOT NULL CHECK (jsonb_typeof(target) = 'object'),
    UNIQUE (runtime_thread_id, live_attachment_id, host_incarnation_id, generation)
);

CREATE TABLE agent_runtime_lifecycle_effect (
    effect_id TEXT PRIMARY KEY CHECK (btrim(effect_id) <> ''),
    runtime_thread_id TEXT NOT NULL
        REFERENCES agent_runtime_lifecycle_target(runtime_thread_id) ON DELETE RESTRICT,
    child_thread_id TEXT,
    operation_kind TEXT NOT NULL CHECK (
        operation_kind IN ('create', 'resume', 'rebind', 'fork', 'execute')
    ),
    live_attachment_id TEXT NOT NULL CHECK (btrim(live_attachment_id) <> ''),
    host_incarnation_id TEXT NOT NULL CHECK (btrim(host_incarnation_id) <> ''),
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    target_snapshot JSONB NOT NULL CHECK (jsonb_typeof(target_snapshot) = 'object'),
    initial_context_digest TEXT,
    fork_cutoff JSONB,
    outcome JSONB,
    effect JSONB NOT NULL CHECK (jsonb_typeof(effect) = 'object'),
    CHECK (
        (
            operation_kind = 'fork'
            AND child_thread_id IS NOT NULL
            AND btrim(child_thread_id) <> ''
            AND fork_cutoff IS NOT NULL
        )
        OR (
            operation_kind <> 'fork'
            AND child_thread_id IS NULL
            AND fork_cutoff IS NULL
        )
    )
);

CREATE TABLE agent_runtime_binding (
    binding_id TEXT PRIMARY KEY CHECK (btrim(binding_id) <> ''),
    logical_instance_id TEXT NOT NULL CHECK (btrim(logical_instance_id) <> ''),
    live_attachment_id TEXT NOT NULL CHECK (btrim(live_attachment_id) <> ''),
    host_incarnation_id TEXT NOT NULL CHECK (btrim(host_incarnation_id) <> ''),
    definition_id TEXT NOT NULL CHECK (btrim(definition_id) <> ''),
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    profile_digest TEXT NOT NULL CHECK (btrim(profile_digest) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    target_snapshot JSONB NOT NULL CHECK (jsonb_typeof(target_snapshot) = 'object'),
    state TEXT NOT NULL CHECK (
        state IN ('pending_surface', 'available', 'desynchronized', 'lost', 'closed')
    ),
    binding JSONB NOT NULL CHECK (jsonb_typeof(binding) = 'object'),
    UNIQUE (live_attachment_id, host_incarnation_id, generation, source_coordinate),
    UNIQUE (binding_id, generation, source_coordinate, bound_surface_digest),
    UNIQUE (binding_id, generation)
);

CREATE TABLE agent_runtime_source_coordinate (
    binding_id TEXT PRIMARY KEY,
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    UNIQUE (binding_id, generation, source_coordinate),
    FOREIGN KEY (binding_id, generation)
        REFERENCES agent_runtime_binding(binding_id, generation) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_callback_route (
    route_id TEXT PRIMARY KEY CHECK (btrim(route_id) <> ''),
    binding_id TEXT NOT NULL REFERENCES agent_runtime_binding(binding_id) ON DELETE RESTRICT,
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    delivery TEXT NOT NULL CHECK (delivery = 'agent_native_callback'),
    default_deadline_ms NUMERIC(20, 0) NOT NULL
        CHECK (default_deadline_ms BETWEEN 1 AND 18446744073709551615),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    route JSONB NOT NULL CHECK (jsonb_typeof(route) = 'object'),
    UNIQUE (route_id, generation, source_coordinate, bound_surface_digest),
    FOREIGN KEY (binding_id, generation, source_coordinate, bound_surface_digest)
        REFERENCES agent_runtime_binding(
            binding_id,
            generation,
            source_coordinate,
            bound_surface_digest
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_callback_route_tombstone (
    route_id TEXT PRIMARY KEY REFERENCES agent_runtime_callback_route(route_id) ON DELETE RESTRICT,
    tombstoned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE agent_runtime_effect (
    effect_id TEXT PRIMARY KEY CHECK (btrim(effect_id) <> ''),
    command_id TEXT NOT NULL CHECK (btrim(command_id) <> ''),
    binding_id TEXT NOT NULL REFERENCES agent_runtime_binding(binding_id) ON DELETE RESTRICT,
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    payload_digest TEXT NOT NULL CHECK (btrim(payload_digest) <> ''),
    delivery_epoch NUMERIC(20, 0) NOT NULL
        CHECK (delivery_epoch BETWEEN 0 AND 18446744073709551615),
    dispatch_attempt NUMERIC(20, 0) NOT NULL
        CHECK (dispatch_attempt BETWEEN 0 AND 18446744073709551615),
    state TEXT NOT NULL CHECK (
        state IN ('dispatching', 'accepted', 'applied', 'rejected', 'not_applied', 'unknown', 'lost')
    ),
    effect JSONB NOT NULL CHECK (jsonb_typeof(effect) = 'object'),
    FOREIGN KEY (binding_id, generation, source_coordinate)
        REFERENCES agent_runtime_source_coordinate(
            binding_id,
            generation,
            source_coordinate
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_effect_attempt_history (
    effect_id TEXT NOT NULL REFERENCES agent_runtime_effect(effect_id) ON DELETE CASCADE,
    dispatch_attempt NUMERIC(20, 0) NOT NULL
        CHECK (dispatch_attempt BETWEEN 1 AND 18446744073709551615),
    delivery_epoch NUMERIC(20, 0) NOT NULL
        CHECK (delivery_epoch BETWEEN 0 AND 18446744073709551615),
    state TEXT NOT NULL CHECK (
        state IN ('dispatching', 'accepted', 'applied', 'rejected', 'not_applied', 'unknown', 'lost')
    ),
    evidence JSONB NOT NULL CHECK (jsonb_typeof(evidence) = 'object'),
    PRIMARY KEY (effect_id, dispatch_attempt)
);

CREATE TABLE agent_runtime_lease_epoch (
    binding_id TEXT NOT NULL REFERENCES agent_runtime_binding(binding_id) ON DELETE CASCADE,
    epoch NUMERIC(20, 0) NOT NULL
        CHECK (epoch BETWEEN 0 AND 18446744073709551615),
    PRIMARY KEY (binding_id, epoch)
);

CREATE TABLE agent_runtime_lease (
    binding_id TEXT PRIMARY KEY,
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    owner TEXT NOT NULL CHECK (btrim(owner) <> ''),
    token TEXT NOT NULL CHECK (btrim(token) <> ''),
    epoch NUMERIC(20, 0) NOT NULL
        CHECK (epoch BETWEEN 1 AND 18446744073709551615),
    expires_at_ms NUMERIC(20, 0) NOT NULL
        CHECK (expires_at_ms BETWEEN 1 AND 18446744073709551615),
    FOREIGN KEY (binding_id, generation)
        REFERENCES agent_runtime_binding(binding_id, generation) ON DELETE CASCADE,
    FOREIGN KEY (binding_id, epoch)
        REFERENCES agent_runtime_lease_epoch(binding_id, epoch) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_callback_reservation (
    route_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    callback_kind TEXT NOT NULL CHECK (callback_kind IN ('tool', 'hook')),
    request_digest TEXT NOT NULL CHECK (btrim(request_digest) <> ''),
    generation NUMERIC(20, 0) NOT NULL
        CHECK (generation BETWEEN 1 AND 18446744073709551615),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    deadline_at_ms NUMERIC(20, 0) NOT NULL
        CHECK (deadline_at_ms BETWEEN 1 AND 18446744073709551615),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'inspection_required', 'unknown', 'settled')
    ),
    reservation JSONB NOT NULL CHECK (jsonb_typeof(reservation) = 'object'),
    PRIMARY KEY (route_id, idempotency_key),
    FOREIGN KEY (route_id, generation, source_coordinate, bound_surface_digest)
        REFERENCES agent_runtime_callback_route(
            route_id,
            generation,
            source_coordinate,
            bound_surface_digest
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_callback_outcome (
    route_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    outcome JSONB NOT NULL,
    PRIMARY KEY (route_id, idempotency_key),
    FOREIGN KEY (route_id, idempotency_key)
        REFERENCES agent_runtime_callback_reservation(route_id, idempotency_key)
        ON DELETE CASCADE
);
