-- Integration Driver Host facts. The 0061 binding/source tables are the minimal referential
-- anchors consumed by Managed Runtime; Host-owned service, offer, binding detail, and lease facts
-- remain behind the Host repository.

CREATE TABLE agent_runtime_service_instance (
    id text PRIMARY KEY,
    definition_id text NOT NULL,
    definition_build_digest text NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    config jsonb NOT NULL,
    credentials jsonb NOT NULL,
    placement jsonb NOT NULL,
    desired_state text NOT NULL CHECK (desired_state IN ('active', 'inactive')),
    observed_state jsonb NOT NULL,
    active_generation bigint NOT NULL DEFAULT 0 CHECK (active_generation >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, revision)
);

CREATE TABLE agent_runtime_service_instance_revision (
    service_instance_id text NOT NULL REFERENCES agent_runtime_service_instance(id) ON DELETE CASCADE,
    revision bigint NOT NULL CHECK (revision > 0),
    instance_snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (service_instance_id, revision)
);

ALTER TABLE agent_runtime_service_instance
    ADD CONSTRAINT agent_runtime_service_instance_current_revision_fkey
    FOREIGN KEY (id, revision)
    REFERENCES agent_runtime_service_instance_revision(service_instance_id, revision)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE agent_runtime_service_activation (
    service_instance_id text NOT NULL,
    instance_revision bigint NOT NULL CHECK (instance_revision > 0),
    driver_generation bigint NOT NULL CHECK (driver_generation > 0),
    protocol_revision integer NOT NULL CHECK (protocol_revision > 0),
    effective_profile jsonb NOT NULL,
    profile_digest text NOT NULL,
    conformance_evidence jsonb NOT NULL,
    instance_snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (service_instance_id, driver_generation),
    FOREIGN KEY (service_instance_id, instance_revision)
        REFERENCES agent_runtime_service_instance_revision(service_instance_id, revision)
        ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_offer (
    id text PRIMARY KEY,
    service_instance_id text NOT NULL,
    instance_revision bigint NOT NULL CHECK (instance_revision > 0),
    driver_generation bigint NOT NULL CHECK (driver_generation > 0),
    profile_digest text NOT NULL,
    available boolean NOT NULL,
    offer jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, profile_digest, driver_generation),
    UNIQUE (id, service_instance_id, instance_revision, driver_generation, profile_digest),
    FOREIGN KEY (service_instance_id, driver_generation)
        REFERENCES agent_runtime_service_activation(service_instance_id, driver_generation)
        ON DELETE RESTRICT
);

CREATE INDEX idx_agent_runtime_offer_available
    ON agent_runtime_offer(service_instance_id, available, driver_generation DESC);

CREATE TABLE agent_runtime_host_binding (
    binding_id text PRIMARY KEY REFERENCES agent_runtime_binding(id) ON DELETE CASCADE,
    thread_id text NOT NULL UNIQUE,
    offer_id text NOT NULL REFERENCES agent_runtime_offer(id) ON DELETE RESTRICT,
    service_instance_id text NOT NULL,
    instance_revision bigint NOT NULL CHECK (instance_revision > 0),
    driver_generation bigint NOT NULL CHECK (driver_generation > 0),
    profile_digest text NOT NULL,
    state text NOT NULL CHECK (state IN ('pending', 'active', 'desynchronized', 'lost', 'closed', 'failed')),
    lease_epoch bigint NOT NULL CHECK (lease_epoch >= 0),
    binding jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (binding_id, driver_generation)
        REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE CASCADE,
    FOREIGN KEY (
        offer_id, service_instance_id, instance_revision, driver_generation, profile_digest
    ) REFERENCES agent_runtime_offer(
        id, service_instance_id, instance_revision, driver_generation, profile_digest
    ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_driver_lease (
    binding_id text PRIMARY KEY REFERENCES agent_runtime_host_binding(binding_id) ON DELETE CASCADE,
    driver_generation bigint NOT NULL CHECK (driver_generation > 0),
    owner text NOT NULL,
    token text NOT NULL,
    epoch bigint NOT NULL CHECK (epoch > 0),
    expires_at timestamptz NOT NULL,
    lease jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (binding_id, driver_generation)
        REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE CASCADE
);

CREATE INDEX idx_agent_runtime_driver_lease_expiry
    ON agent_runtime_driver_lease(expires_at);

CREATE TABLE agent_runtime_driver_coordinate (
    binding_id text NOT NULL,
    driver_generation bigint NOT NULL CHECK (driver_generation > 0),
    coordinate_kind text NOT NULL CHECK (coordinate_kind IN ('turn', 'item')),
    runtime_id text NOT NULL,
    source_id text NOT NULL,
    coordinate jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (binding_id, driver_generation, coordinate_kind, runtime_id),
    UNIQUE (binding_id, driver_generation, coordinate_kind, source_id),
    FOREIGN KEY (binding_id, driver_generation)
        REFERENCES agent_runtime_binding(id, driver_generation) ON DELETE CASCADE
);
