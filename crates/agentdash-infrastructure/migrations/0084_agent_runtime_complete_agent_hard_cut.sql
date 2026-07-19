-- Agent Runtime S5 hard cut.
--
-- The project is not deployed, so runtime-era development facts are intentionally discarded.
-- Product graph rows remain in their owning lifecycle tables; every Runtime, Host, callback and
-- Dash fact below starts from the final owner-specific schema.

DROP TABLE IF EXISTS agent_runtime_terminal_application_effect_outbox CASCADE;
DROP TABLE IF EXISTS agent_run_runtime_binding CASCADE;
DROP TABLE IF EXISTS agent_context_activation_dispatch CASCADE;
DROP TABLE IF EXISTS agent_context_activation CASCADE;
DROP TABLE IF EXISTS agent_context_candidate CASCADE;
DROP TABLE IF EXISTS agent_context_preparation CASCADE;
DROP TABLE IF EXISTS agent_context_head CASCADE;
DROP TABLE IF EXISTS agent_context_checkpoint CASCADE;
DROP TABLE IF EXISTS agent_runtime_hook_effect CASCADE;
DROP TABLE IF EXISTS agent_runtime_hook_run CASCADE;
DROP TABLE IF EXISTS agent_runtime_hook_plan CASCADE;
DROP TABLE IF EXISTS agent_runtime_tool_call CASCADE;
DROP TABLE IF EXISTS agent_runtime_quarantine CASCADE;
DROP TABLE IF EXISTS agent_runtime_event CASCADE;
DROP TABLE IF EXISTS agent_runtime_interaction CASCADE;
DROP TABLE IF EXISTS agent_runtime_item CASCADE;
DROP TABLE IF EXISTS agent_runtime_turn CASCADE;
DROP TABLE IF EXISTS agent_runtime_outbox CASCADE;
DROP TABLE IF EXISTS agent_runtime_operation CASCADE;
DROP TABLE IF EXISTS agent_runtime_source_coordinate CASCADE;
DROP TABLE IF EXISTS agent_runtime_binding CASCADE;
DROP TABLE IF EXISTS agent_runtime_thread CASCADE;
DROP TABLE IF EXISTS agent_runtime_driver_coordinate CASCADE;
DROP TABLE IF EXISTS agent_runtime_driver_lease CASCADE;
DROP TABLE IF EXISTS agent_runtime_host_binding CASCADE;
DROP TABLE IF EXISTS agent_runtime_offer CASCADE;
DROP TABLE IF EXISTS agent_runtime_service_activation CASCADE;
DROP TABLE IF EXISTS agent_runtime_service_instance_revision CASCADE;
DROP TABLE IF EXISTS agent_runtime_service_instance CASCADE;
DROP TABLE IF EXISTS agent_runtime_surface_snapshot CASCADE;
DROP TABLE IF EXISTS agent_run_runtime_recovery_intent CASCADE;
DROP TABLE IF EXISTS agent_run_runtime_binding_lineage CASCADE;
DROP TABLE IF EXISTS agent_run_runtime_thread_anchor CASCADE;

ALTER TABLE lifecycle_agents
    ADD CONSTRAINT lifecycle_agents_id_run_project_key UNIQUE (id, run_id, project_id);

CREATE TABLE workspace_module_presentation_head (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    latest_change_sequence BIGINT NOT NULL CHECK (latest_change_sequence >= 0),
    PRIMARY KEY (target_run_id, target_agent_id),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE
);

CREATE TABLE workspace_module_presentation_intent (
    intent_id TEXT PRIMARY KEY CHECK (btrim(intent_id) <> ''),
    effect_id TEXT NOT NULL UNIQUE CHECK (btrim(effect_id) <> ''),
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'fulfilled', 'expired')),
    presentation_digest TEXT NOT NULL CHECK (btrim(presentation_digest) <> ''),
    module_id TEXT NOT NULL CHECK (btrim(module_id) <> ''),
    view_key TEXT NOT NULL CHECK (btrim(view_key) <> ''),
    renderer_kind TEXT NOT NULL CHECK (btrim(renderer_kind) <> ''),
    presentation_uri TEXT NOT NULL CHECK (btrim(presentation_uri) <> ''),
    runtime_thread_id TEXT NOT NULL CHECK (btrim(runtime_thread_id) <> ''),
    runtime_operation_id TEXT,
    runtime_turn_id TEXT NOT NULL CHECK (btrim(runtime_turn_id) <> ''),
    runtime_item_id TEXT NOT NULL CHECK (btrim(runtime_item_id) <> ''),
    source_ref TEXT NOT NULL CHECK (btrim(source_ref) <> ''),
    source_committed_revision BIGINT NOT NULL CHECK (source_committed_revision >= 0),
    source_applied_surface_revision BIGINT NOT NULL CHECK (source_applied_surface_revision >= 0),
    source_activated_revision BIGINT CHECK (
        source_activated_revision IS NULL OR source_activated_revision >= 0
    ),
    currentness_fence JSONB NOT NULL CHECK (jsonb_typeof(currentness_fence) = 'object'),
    intent JSONB NOT NULL CHECK (jsonb_typeof(intent) = 'object'),
    committed_at_ms BIGINT NOT NULL CHECK (committed_at_ms >= 0),
    UNIQUE (intent_id, target_run_id, target_agent_id),
    UNIQUE (intent_id, effect_id, target_run_id, target_agent_id),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE
);

CREATE TABLE workspace_module_presentation_change (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    change_sequence BIGINT NOT NULL CHECK (change_sequence > 0),
    change_id TEXT NOT NULL UNIQUE CHECK (btrim(change_id) <> ''),
    intent_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'fulfilled', 'expired')),
    change JSONB NOT NULL CHECK (jsonb_typeof(change) = 'object'),
    PRIMARY KEY (target_run_id, target_agent_id, change_sequence),
    UNIQUE (target_run_id, target_agent_id, revision),
    FOREIGN KEY (intent_id, target_run_id, target_agent_id)
        REFERENCES workspace_module_presentation_intent(
            intent_id,
            target_run_id,
            target_agent_id
        ) ON DELETE RESTRICT
);

CREATE TABLE workspace_module_presentation_ack (
    ack_id TEXT PRIMARY KEY CHECK (btrim(ack_id) <> ''),
    intent_id TEXT NOT NULL UNIQUE,
    effect_id TEXT NOT NULL,
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    acknowledged_change_sequence BIGINT NOT NULL CHECK (acknowledged_change_sequence > 0),
    fulfilled_at_ms BIGINT NOT NULL CHECK (fulfilled_at_ms >= 0),
    acknowledgement JSONB NOT NULL CHECK (jsonb_typeof(acknowledgement) = 'object'),
    FOREIGN KEY (intent_id, effect_id, target_run_id, target_agent_id)
        REFERENCES workspace_module_presentation_intent(
            intent_id,
            effect_id,
            target_run_id,
            target_agent_id
        ) ON DELETE RESTRICT
);

CREATE TABLE workspace_module_presentation_outbox (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    change_sequence BIGINT NOT NULL CHECK (change_sequence > 0),
    effect_id TEXT NOT NULL,
    change_id TEXT NOT NULL UNIQUE,
    entry JSONB NOT NULL CHECK (jsonb_typeof(entry) = 'object'),
    PRIMARY KEY (target_run_id, target_agent_id, change_sequence),
    FOREIGN KEY (target_run_id, target_agent_id, change_sequence)
        REFERENCES workspace_module_presentation_change(
            target_run_id,
            target_agent_id,
            change_sequence
        ) ON DELETE CASCADE,
    FOREIGN KEY (effect_id)
        REFERENCES workspace_module_presentation_intent(effect_id) ON DELETE RESTRICT,
    FOREIGN KEY (change_id)
        REFERENCES workspace_module_presentation_change(change_id) ON DELETE RESTRICT
);

CREATE TABLE agent_run_terminal_projection_head (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    latest_change_sequence BIGINT NOT NULL CHECK (latest_change_sequence >= 0),
    PRIMARY KEY (target_run_id, target_agent_id),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE
);

CREATE TABLE agent_run_terminal_projection (
    terminal_id TEXT PRIMARY KEY CHECK (btrim(terminal_id) <> ''),
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    terminal_owner_epoch_id TEXT NOT NULL CHECK (btrim(terminal_owner_epoch_id) <> ''),
    runtime_thread_id TEXT NOT NULL CHECK (btrim(runtime_thread_id) <> ''),
    source_ref TEXT NOT NULL CHECK (btrim(source_ref) <> ''),
    source_committed_revision BIGINT NOT NULL CHECK (source_committed_revision >= 0),
    source_applied_surface_revision BIGINT NOT NULL CHECK (source_applied_surface_revision >= 0),
    source_activated_revision BIGINT CHECK (
        source_activated_revision IS NULL OR source_activated_revision >= 0
    ),
    backend_id TEXT NOT NULL CHECK (btrim(backend_id) <> ''),
    process_state TEXT NOT NULL CHECK (
        process_state IN ('starting', 'running', 'exited', 'killed', 'lost')
    ),
    availability TEXT NOT NULL CHECK (availability IN ('online', 'offline', 'reconciling')),
    latest_source_sequence BIGINT NOT NULL CHECK (latest_source_sequence >= 0),
    next_output_sequence BIGINT NOT NULL CHECK (next_output_sequence >= 0),
    max_output_bytes BIGINT NOT NULL CHECK (max_output_bytes >= 0),
    projection JSONB NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
    UNIQUE (terminal_id, target_run_id, target_agent_id),
    UNIQUE (terminal_owner_epoch_id, terminal_id),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE
);

CREATE TABLE agent_run_terminal_projection_change (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    change_sequence BIGINT NOT NULL CHECK (change_sequence > 0),
    change_id TEXT NOT NULL UNIQUE CHECK (btrim(change_id) <> ''),
    terminal_id TEXT NOT NULL CHECK (btrim(terminal_id) <> ''),
    terminal_owner_epoch_id TEXT NOT NULL CHECK (btrim(terminal_owner_epoch_id) <> ''),
    source_sequence BIGINT CHECK (source_sequence IS NULL OR source_sequence > 0),
    output_sequence BIGINT CHECK (output_sequence IS NULL OR output_sequence >= 0),
    payload_digest TEXT NOT NULL CHECK (btrim(payload_digest) <> ''),
    delta_kind TEXT NOT NULL CHECK (btrim(delta_kind) <> ''),
    change JSONB NOT NULL CHECK (jsonb_typeof(change) = 'object'),
    PRIMARY KEY (target_run_id, target_agent_id, change_sequence),
    UNIQUE (target_run_id, target_agent_id, revision),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX agent_run_terminal_projection_change_source_key
    ON agent_run_terminal_projection_change(terminal_owner_epoch_id, source_sequence)
    WHERE source_sequence IS NOT NULL;
CREATE UNIQUE INDEX agent_run_terminal_projection_change_output_key
    ON agent_run_terminal_projection_change(terminal_id, output_sequence)
    WHERE output_sequence IS NOT NULL;

CREATE TABLE agent_run_terminal_control_correlation (
    correlation_id TEXT PRIMARY KEY CHECK (btrim(correlation_id) <> ''),
    terminal_id TEXT NOT NULL CHECK (btrim(terminal_id) <> ''),
    terminal_owner_epoch_id TEXT NOT NULL CHECK (btrim(terminal_owner_epoch_id) <> ''),
    change_id TEXT NOT NULL UNIQUE,
    control_kind TEXT NOT NULL CHECK (btrim(control_kind) <> ''),
    control_status TEXT NOT NULL CHECK (btrim(control_status) <> ''),
    correlation JSONB NOT NULL CHECK (jsonb_typeof(correlation) = 'object'),
    FOREIGN KEY (change_id)
        REFERENCES agent_run_terminal_projection_change(change_id) ON DELETE CASCADE
);

CREATE TABLE agent_run_terminal_projection_outbox (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    change_sequence BIGINT NOT NULL CHECK (change_sequence > 0),
    change_id TEXT NOT NULL UNIQUE,
    entry JSONB NOT NULL CHECK (jsonb_typeof(entry) = 'object'),
    PRIMARY KEY (target_run_id, target_agent_id, change_sequence),
    FOREIGN KEY (target_run_id, target_agent_id, change_sequence)
        REFERENCES agent_run_terminal_projection_change(
            target_run_id,
            target_agent_id,
            change_sequence
        ) ON DELETE CASCADE,
    FOREIGN KEY (change_id)
        REFERENCES agent_run_terminal_projection_change(change_id) ON DELETE RESTRICT
);

CREATE TABLE agent_run_fork_saga (
    request_id UUID PRIMARY KEY,
    version BIGINT NOT NULL CHECK (version > 0),
    phase TEXT NOT NULL CHECK (
        phase IN (
            'requested',
            'runtime_admitted',
            'runtime_provisioned',
            'product_graph_committed',
            'runtime_activated',
            'succeeded'
        )
    ),
    durable_runtime_dispatch JSONB CHECK (
        durable_runtime_dispatch IS NULL OR jsonb_typeof(durable_runtime_dispatch) = 'object'
    ),
    runtime_thread_id TEXT NOT NULL UNIQUE CHECK (btrim(runtime_thread_id) <> ''),
    graph_commit_revision BIGINT CHECK (graph_commit_revision IS NULL OR graph_commit_revision > 0),
    saga JSONB NOT NULL CHECK (jsonb_typeof(saga) = 'object'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (
        (phase IN ('requested', 'runtime_admitted', 'runtime_provisioned')
            AND graph_commit_revision IS NULL)
        OR (phase IN ('product_graph_committed', 'runtime_activated', 'succeeded')
            AND graph_commit_revision IS NOT NULL)
    ),
    CHECK (
        durable_runtime_dispatch IS NULL
        OR (
            durable_runtime_dispatch #>> '{identity,request_id}' IS NOT NULL
            AND durable_runtime_dispatch #>> '{identity,request_id}' = request_id::TEXT
            AND durable_runtime_dispatch #>> '{identity,operation}' IN ('fork', 'activate')
            AND COALESCE(
                btrim(durable_runtime_dispatch #>> '{identity,runtime_operation_id}'),
                ''
            ) <> ''
        )
    ),
    UNIQUE (request_id, graph_commit_revision)
);
CREATE UNIQUE INDEX agent_run_fork_saga_active_runtime_operation_key
    ON agent_run_fork_saga ((durable_runtime_dispatch #>> '{identity,runtime_operation_id}'))
    WHERE durable_runtime_dispatch IS NOT NULL;

CREATE TABLE agent_run_fork_graph (
    request_id UUID PRIMARY KEY,
    graph_commit_revision BIGINT NOT NULL CHECK (graph_commit_revision > 0),
    payload_digest TEXT NOT NULL CHECK (btrim(payload_digest) <> ''),
    graph JSONB NOT NULL CHECK (jsonb_typeof(graph) = 'object'),
    committed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, payload_digest),
    FOREIGN KEY (request_id, graph_commit_revision)
        REFERENCES agent_run_fork_saga(request_id, graph_commit_revision) ON DELETE RESTRICT
);

CREATE TABLE companion_fresh_saga (
    request_id UUID PRIMARY KEY,
    version BIGINT NOT NULL CHECK (version > 0),
    phase TEXT NOT NULL CHECK (
        phase IN ('requested', 'agent_created', 'activated', 'first_input_submitted', 'succeeded')
    ),
    runtime_thread_id TEXT NOT NULL CHECK (btrim(runtime_thread_id) <> ''),
    create_effect_id UUID NOT NULL UNIQUE,
    activation_effect_id UUID NOT NULL UNIQUE,
    first_input_effect_id UUID NOT NULL UNIQUE,
    durable_dispatch JSONB CHECK (
        durable_dispatch IS NULL OR jsonb_typeof(durable_dispatch) = 'object'
    ),
    context_application_evidence JSONB CHECK (
        context_application_evidence IS NULL
        OR jsonb_typeof(context_application_evidence) = 'object'
    ),
    first_input_receipt JSONB CHECK (
        first_input_receipt IS NULL OR jsonb_typeof(first_input_receipt) = 'object'
    ),
    saga JSONB NOT NULL CHECK (jsonb_typeof(saga) = 'object'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (runtime_thread_id),
    CHECK (
        create_effect_id <> activation_effect_id
        AND create_effect_id <> first_input_effect_id
        AND activation_effect_id <> first_input_effect_id
    ),
    CHECK (
        durable_dispatch IS NULL
        OR (
            durable_dispatch #>> '{identity,request_id}' IS NOT NULL
            AND durable_dispatch #>> '{identity,request_id}' = request_id::TEXT
            AND (
                (
                    durable_dispatch #>> '{identity,operation}' = 'create_with_context_package'
                    AND (durable_dispatch #>> '{identity,effect_id}')::UUID = create_effect_id
                )
                OR (
                    durable_dispatch #>> '{identity,operation}' = 'activate'
                    AND (durable_dispatch #>> '{identity,effect_id}')::UUID = activation_effect_id
                )
                OR (
                    durable_dispatch #>> '{identity,operation}' = 'submit_first_input'
                    AND (durable_dispatch #>> '{identity,effect_id}')::UUID = first_input_effect_id
                )
            )
            AND COALESCE(
                btrim(durable_dispatch #>> '{identity,runtime_operation_id}'),
                ''
            ) <> ''
        )
    )
);

CREATE TABLE agent_runtime_state_revision (
    thread_id TEXT PRIMARY KEY CHECK (btrim(thread_id) <> ''),
    revision BIGINT NOT NULL CHECK (revision > 0),
    facts JSONB NOT NULL CHECK (jsonb_typeof(facts) = 'object')
);

CREATE TABLE agent_runtime_source_projection (
    thread_id TEXT PRIMARY KEY REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    projection_revision BIGINT NOT NULL CHECK (projection_revision >= 0),
    authority TEXT NOT NULL CHECK (btrim(authority) <> ''),
    fidelity TEXT NOT NULL CHECK (btrim(fidelity) <> ''),
    source_revision TEXT,
    source_cursor TEXT,
    projection_digest TEXT NOT NULL CHECK (btrim(projection_digest) <> ''),
    projection JSONB NOT NULL
);

CREATE TABLE agent_runtime_source_identity (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    identity_kind TEXT NOT NULL CHECK (btrim(identity_kind) <> ''),
    source_identity TEXT NOT NULL CHECK (btrim(source_identity) <> ''),
    runtime_identity TEXT NOT NULL CHECK (btrim(runtime_identity) <> ''),
    PRIMARY KEY (thread_id, identity_kind, source_identity),
    UNIQUE (thread_id, identity_kind, runtime_identity)
);

CREATE TABLE agent_runtime_source_change (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    projection_revision BIGINT NOT NULL CHECK (projection_revision >= 0),
    observation_digest TEXT NOT NULL CHECK (btrim(observation_digest) <> ''),
    source_revision TEXT,
    source_cursor TEXT,
    changed_sections JSONB NOT NULL CHECK (jsonb_typeof(changed_sections) = 'array'),
    change JSONB NOT NULL,
    PRIMARY KEY (thread_id, source_sequence)
);

CREATE TABLE agent_runtime_projection (
    thread_id TEXT PRIMARY KEY REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    projection_revision BIGINT NOT NULL CHECK (projection_revision >= 0),
    change_head BIGINT NOT NULL CHECK (change_head >= 0),
    projection JSONB NOT NULL
);

CREATE TABLE agent_runtime_thread_binding (
    thread_id TEXT PRIMARY KEY REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    source_ref JSONB NOT NULL,
    binding JSONB NOT NULL,
    committed_at_revision BIGINT NOT NULL CHECK (committed_at_revision >= 0),
    activated_at_revision BIGINT CHECK (
        activated_at_revision IS NULL OR activated_at_revision >= committed_at_revision
    )
);

CREATE TABLE agent_run_product_runtime_binding (
    target_run_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    runtime_thread_id TEXT NOT NULL UNIQUE CHECK (btrim(runtime_thread_id) <> ''),
    source_ref TEXT NOT NULL CHECK (btrim(source_ref) <> ''),
    source_committed_revision BIGINT NOT NULL CHECK (source_committed_revision >= 0),
    source_applied_surface_revision BIGINT NOT NULL CHECK (source_applied_surface_revision >= 0),
    source_activated_revision BIGINT CHECK (
        source_activated_revision IS NULL OR source_activated_revision >= 0
    ),
    binding JSONB NOT NULL CHECK (jsonb_typeof(binding) = 'object'),
    PRIMARY KEY (target_run_id, target_agent_id),
    FOREIGN KEY (target_agent_id, target_run_id, project_id)
        REFERENCES lifecycle_agents(id, run_id, project_id) ON DELETE CASCADE,
    FOREIGN KEY (runtime_thread_id)
        REFERENCES agent_runtime_thread_binding(thread_id) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_operation (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL CHECK (btrim(operation_id) <> ''),
    command_kind TEXT NOT NULL CHECK (btrim(command_kind) <> ''),
    command JSONB NOT NULL,
    receipt JSONB NOT NULL,
    operation JSONB NOT NULL,
    PRIMARY KEY (thread_id, operation_id)
);

CREATE TABLE agent_runtime_idempotency (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    operation_id TEXT NOT NULL,
    PRIMARY KEY (thread_id, idempotency_key),
    UNIQUE (thread_id, operation_id),
    FOREIGN KEY (thread_id, operation_id)
        REFERENCES agent_runtime_operation(thread_id, operation_id) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_pending_command (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL,
    effect_id TEXT NOT NULL CHECK (btrim(effect_id) <> ''),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'delivered', 'inspection_required', 'settled', 'lost')
    ),
    command JSONB NOT NULL,
    claim_owner TEXT,
    claim_epoch BIGINT NOT NULL CHECK (claim_epoch >= 0),
    PRIMARY KEY (thread_id, operation_id),
    UNIQUE (effect_id),
    FOREIGN KEY (thread_id, operation_id)
        REFERENCES agent_runtime_operation(thread_id, operation_id) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_change (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    operation_id TEXT,
    change JSONB NOT NULL,
    PRIMARY KEY (thread_id, sequence),
    FOREIGN KEY (thread_id, operation_id)
        REFERENCES agent_runtime_operation(thread_id, operation_id) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_outbox (
    thread_id TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    operation_id TEXT,
    change JSONB NOT NULL,
    PRIMARY KEY (thread_id, sequence),
    FOREIGN KEY (thread_id, sequence)
        REFERENCES agent_runtime_change(thread_id, sequence) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_product_change_delivery (
    thread_id TEXT NOT NULL,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'claimed', 'delivered')),
    claim_owner TEXT,
    claim_token UUID,
    claim_expires_at TIMESTAMPTZ,
    attempt_count BIGINT NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    delivered_at TIMESTAMPTZ,
    PRIMARY KEY (thread_id, sequence),
    FOREIGN KEY (thread_id, sequence)
        REFERENCES agent_runtime_outbox(thread_id, sequence) ON DELETE CASCADE,
    CHECK (
        (
            status = 'claimed'
            AND claim_owner IS NOT NULL
            AND btrim(claim_owner) <> ''
            AND claim_token IS NOT NULL
            AND claim_expires_at IS NOT NULL
            AND delivered_at IS NULL
        )
        OR (
            status = 'pending'
            AND claim_owner IS NULL
            AND claim_token IS NULL
            AND claim_expires_at IS NULL
            AND delivered_at IS NULL
        )
        OR (
            status = 'delivered'
            AND claim_owner IS NULL
            AND claim_token IS NULL
            AND claim_expires_at IS NULL
            AND delivered_at IS NOT NULL
        )
    )
);

CREATE TABLE agent_runtime_surface_snapshot (
    thread_id TEXT NOT NULL REFERENCES agent_runtime_state_revision(thread_id) ON DELETE CASCADE,
    surface_revision BIGINT NOT NULL CHECK (surface_revision >= 0),
    surface_digest TEXT NOT NULL CHECK (btrim(surface_digest) <> ''),
    surface JSONB NOT NULL,
    PRIMARY KEY (thread_id, surface_revision)
);

CREATE TABLE agent_runtime_host_revision (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision BIGINT NOT NULL CHECK (revision >= 0),
    facts JSONB NOT NULL CHECK (jsonb_typeof(facts) = 'object')
);
INSERT INTO agent_runtime_host_revision(singleton, revision, facts)
VALUES (
    TRUE,
    0,
    '{
        "service_instances": {},
        "service_verifications": {},
        "offers": {},
        "placements": {},
        "remote_bindings": {},
        "bindings": {},
        "source_coordinates": {},
        "callback_routes": {},
        "revoked_callback_routes": [],
        "effects": {},
        "leases": {},
        "lease_epochs": {},
        "runtime_targets": {},
        "lifecycle_effects": {}
    }'::JSONB
);

CREATE TABLE agent_service_instance (
    service_instance_id TEXT PRIMARY KEY CHECK (btrim(service_instance_id) <> ''),
    descriptor_digest TEXT NOT NULL CHECK (btrim(descriptor_digest) <> ''),
    descriptor JSONB NOT NULL
);

CREATE TABLE agent_service_verification (
    service_instance_id TEXT PRIMARY KEY
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    publisher_integration TEXT NOT NULL CHECK (btrim(publisher_integration) <> ''),
    service_version TEXT NOT NULL CHECK (btrim(service_version) <> ''),
    verifier_identity TEXT NOT NULL CHECK (btrim(verifier_identity) <> ''),
    verifier_revision TEXT NOT NULL CHECK (btrim(verifier_revision) <> ''),
    verification_method TEXT NOT NULL CHECK (
        verification_method IN ('pinned_builtin', 'remote_transport_attestation')
    ),
    verified_profile_digest TEXT NOT NULL CHECK (btrim(verified_profile_digest) <> ''),
    claimed_conformance_suite_revision TEXT NOT NULL
        CHECK (btrim(claimed_conformance_suite_revision) <> ''),
    claimed_build_digest TEXT NOT NULL CHECK (btrim(claimed_build_digest) <> ''),
    evidence_digest TEXT NOT NULL CHECK (btrim(evidence_digest) <> ''),
    verification JSONB NOT NULL CHECK (jsonb_typeof(verification) = 'object')
);

CREATE TABLE agent_runtime_offer (
    service_instance_id TEXT PRIMARY KEY
        REFERENCES agent_service_instance(service_instance_id) ON DELETE CASCADE,
    profile_digest TEXT NOT NULL CHECK (btrim(profile_digest) <> ''),
    offer JSONB NOT NULL
);

CREATE TABLE agent_runtime_placement (
    service_instance_id TEXT PRIMARY KEY
        REFERENCES agent_service_instance(service_instance_id) ON DELETE CASCADE,
    placement_kind TEXT NOT NULL CHECK (
        placement_kind IN ('in_process', 'local_process', 'remote')
    ),
    host_id TEXT,
    transport_id TEXT,
    host_incarnation_id TEXT NOT NULL CHECK (btrim(host_incarnation_id) <> ''),
    placement JSONB NOT NULL,
    UNIQUE (service_instance_id, transport_id, host_incarnation_id),
    CHECK (
        (placement_kind = 'in_process' AND host_id IS NULL AND transport_id IS NULL)
        OR (
            placement_kind = 'local_process'
            AND host_id IS NOT NULL
            AND btrim(host_id) <> ''
            AND transport_id IS NULL
        )
        OR (
            placement_kind = 'remote'
            AND host_id IS NOT NULL
            AND btrim(host_id) <> ''
            AND transport_id IS NOT NULL
            AND btrim(transport_id) <> ''
        )
    )
);

CREATE TABLE agent_runtime_remote_binding (
    local_service_instance_id TEXT PRIMARY KEY,
    local_binding_generation BIGINT NOT NULL CHECK (local_binding_generation > 0),
    remote_service_instance_id TEXT NOT NULL CHECK (btrim(remote_service_instance_id) <> ''),
    remote_binding_generation BIGINT NOT NULL CHECK (remote_binding_generation > 0),
    host_incarnation_id TEXT NOT NULL CHECK (btrim(host_incarnation_id) <> ''),
    transport_id TEXT NOT NULL CHECK (btrim(transport_id) <> ''),
    mapping JSONB NOT NULL CHECK (jsonb_typeof(mapping) = 'object'),
    FOREIGN KEY (local_service_instance_id)
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (local_service_instance_id, transport_id, host_incarnation_id)
        REFERENCES agent_runtime_placement(
            service_instance_id,
            transport_id,
            host_incarnation_id
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_lifecycle_target (
    runtime_thread_id TEXT PRIMARY KEY CHECK (btrim(runtime_thread_id) <> ''),
    service_instance_id TEXT NOT NULL
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    generation BIGINT NOT NULL CHECK (generation > 0),
    profile_digest TEXT NOT NULL CHECK (btrim(profile_digest) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    target JSONB NOT NULL,
    UNIQUE (runtime_thread_id, service_instance_id, generation)
);

CREATE TABLE agent_runtime_lifecycle_effect (
    effect_id TEXT PRIMARY KEY CHECK (btrim(effect_id) <> ''),
    runtime_thread_id TEXT NOT NULL
        REFERENCES agent_runtime_lifecycle_target(runtime_thread_id) ON DELETE RESTRICT,
    child_thread_id TEXT,
    operation_kind TEXT NOT NULL CHECK (
        operation_kind IN ('create', 'resume', 'fork', 'execute')
    ),
    service_instance_id TEXT NOT NULL
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    generation BIGINT NOT NULL CHECK (generation > 0),
    initial_context_digest TEXT,
    fork_cutoff JSONB,
    outcome JSONB,
    effect JSONB NOT NULL,
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
    ),
    FOREIGN KEY (runtime_thread_id, service_instance_id, generation)
        REFERENCES agent_runtime_lifecycle_target(
            runtime_thread_id,
            service_instance_id,
            generation
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_binding (
    binding_id TEXT PRIMARY KEY CHECK (btrim(binding_id) <> ''),
    service_instance_id TEXT NOT NULL
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    generation BIGINT NOT NULL CHECK (generation > 0),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    profile_digest TEXT NOT NULL CHECK (btrim(profile_digest) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    state TEXT NOT NULL CHECK (
        state IN ('pending_surface', 'available', 'desynchronized', 'lost', 'closed')
    ),
    binding JSONB NOT NULL,
    UNIQUE (service_instance_id, generation, source_coordinate),
    UNIQUE (binding_id, service_instance_id, generation, source_coordinate),
    UNIQUE (binding_id, generation, source_coordinate, bound_surface_digest),
    UNIQUE (binding_id, generation)
);

CREATE TABLE agent_runtime_source_coordinate (
    binding_id TEXT PRIMARY KEY,
    service_instance_id TEXT NOT NULL,
    generation BIGINT NOT NULL,
    source_coordinate TEXT NOT NULL,
    UNIQUE (service_instance_id, generation, source_coordinate),
    FOREIGN KEY (binding_id, service_instance_id, generation, source_coordinate)
        REFERENCES agent_runtime_binding(
            binding_id,
            service_instance_id,
            generation,
            source_coordinate
        )
        ON DELETE CASCADE
);

CREATE TABLE agent_runtime_callback_route (
    route_id TEXT PRIMARY KEY CHECK (btrim(route_id) <> ''),
    binding_id TEXT NOT NULL REFERENCES agent_runtime_binding(binding_id) ON DELETE RESTRICT,
    generation BIGINT NOT NULL CHECK (generation > 0),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    delivery TEXT NOT NULL CHECK (delivery = 'agent_native_callback'),
    default_deadline_ms BIGINT NOT NULL CHECK (default_deadline_ms > 0),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    route JSONB NOT NULL,
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
    service_instance_id TEXT NOT NULL
        REFERENCES agent_service_instance(service_instance_id) ON DELETE RESTRICT,
    generation BIGINT NOT NULL CHECK (generation > 0),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    payload_digest TEXT NOT NULL CHECK (btrim(payload_digest) <> ''),
    delivery_epoch BIGINT NOT NULL CHECK (delivery_epoch >= 0),
    dispatch_attempt BIGINT NOT NULL CHECK (dispatch_attempt >= 0),
    state TEXT NOT NULL CHECK (
        state IN ('dispatching', 'accepted', 'applied', 'rejected', 'not_applied', 'unknown', 'lost')
    ),
    effect JSONB NOT NULL,
    FOREIGN KEY (binding_id, service_instance_id, generation, source_coordinate)
        REFERENCES agent_runtime_binding(
            binding_id,
            service_instance_id,
            generation,
            source_coordinate
        ) ON DELETE RESTRICT
);

CREATE TABLE agent_runtime_effect_attempt_history (
    effect_id TEXT NOT NULL REFERENCES agent_runtime_effect(effect_id) ON DELETE CASCADE,
    dispatch_attempt BIGINT NOT NULL CHECK (dispatch_attempt > 0),
    delivery_epoch BIGINT NOT NULL CHECK (delivery_epoch >= 0),
    state TEXT NOT NULL CHECK (
        state IN ('dispatching', 'accepted', 'applied', 'rejected', 'not_applied', 'unknown', 'lost')
    ),
    evidence JSONB NOT NULL,
    PRIMARY KEY (effect_id, dispatch_attempt)
);

CREATE TABLE agent_runtime_lease_epoch (
    binding_id TEXT NOT NULL REFERENCES agent_runtime_binding(binding_id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL CHECK (epoch >= 0),
    PRIMARY KEY (binding_id, epoch)
);

CREATE TABLE agent_runtime_lease (
    binding_id TEXT PRIMARY KEY,
    generation BIGINT NOT NULL CHECK (generation > 0),
    owner TEXT NOT NULL CHECK (btrim(owner) <> ''),
    token TEXT NOT NULL CHECK (btrim(token) <> ''),
    epoch BIGINT NOT NULL CHECK (epoch > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (binding_id, generation)
        REFERENCES agent_runtime_binding(binding_id, generation) ON DELETE CASCADE,
    FOREIGN KEY (binding_id, epoch)
        REFERENCES agent_runtime_lease_epoch(binding_id, epoch) ON DELETE CASCADE
);

CREATE TABLE agent_runtime_callback_revision (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision BIGINT NOT NULL CHECK (revision >= 0),
    facts JSONB NOT NULL CHECK (jsonb_typeof(facts) = 'object')
);
INSERT INTO agent_runtime_callback_revision(singleton, revision, facts)
VALUES (TRUE, 0, '{"callbacks": {}}'::JSONB);

CREATE TABLE agent_runtime_callback_reservation (
    route_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    callback_kind TEXT NOT NULL CHECK (callback_kind IN ('tool', 'hook')),
    request_digest TEXT NOT NULL CHECK (btrim(request_digest) <> ''),
    generation BIGINT NOT NULL CHECK (generation > 0),
    source_coordinate TEXT NOT NULL CHECK (btrim(source_coordinate) <> ''),
    bound_surface_digest TEXT NOT NULL CHECK (btrim(bound_surface_digest) <> ''),
    deadline_at TIMESTAMPTZ NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'inspection_required', 'unknown', 'settled')
    ),
    reservation JSONB NOT NULL,
    PRIMARY KEY (route_id, idempotency_key),
    FOREIGN KEY (route_id, generation, source_coordinate, bound_surface_digest)
        REFERENCES agent_runtime_callback_route(
            route_id,
            generation,
            source_coordinate,
            bound_surface_digest
        )
        ON DELETE RESTRICT
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

CREATE TABLE dash_agent_session (
    source_coordinate TEXT PRIMARY KEY CHECK (btrim(source_coordinate) <> ''),
    repository_revision BIGINT NOT NULL CHECK (repository_revision > 0),
    branch_id TEXT NOT NULL CHECK (btrim(branch_id) <> ''),
    head_revision BIGINT NOT NULL CHECK (head_revision >= 0),
    head_entry_id TEXT,
    history_digest TEXT NOT NULL CHECK (btrim(history_digest) <> ''),
    repository JSONB NOT NULL CHECK (jsonb_typeof(repository) = 'object'),
    CHECK (
        (head_revision = 0 AND head_entry_id IS NULL)
        OR (head_revision > 0 AND head_entry_id IS NOT NULL)
    ),
    UNIQUE (source_coordinate, branch_id)
);

CREATE TABLE dash_agent_branch (
    source_coordinate TEXT NOT NULL
        REFERENCES dash_agent_session(source_coordinate) ON DELETE CASCADE,
    branch_id TEXT NOT NULL CHECK (btrim(branch_id) <> ''),
    parent_source_coordinate TEXT,
    parent_branch_id TEXT,
    source_head TEXT,
    source_digest TEXT,
    fork_cutoff JSONB,
    head_revision BIGINT NOT NULL CHECK (head_revision >= 0),
    head_entry_id TEXT,
    branch JSONB NOT NULL CHECK (jsonb_typeof(branch) = 'object'),
    CHECK (
        (
            parent_source_coordinate IS NULL
            AND parent_branch_id IS NULL
            AND source_head IS NULL
            AND source_digest IS NULL
            AND fork_cutoff IS NULL
        )
        OR (
            parent_source_coordinate IS NOT NULL
            AND btrim(parent_source_coordinate) <> ''
            AND source_coordinate <> parent_source_coordinate
            AND parent_branch_id IS NOT NULL
            AND btrim(parent_branch_id) <> ''
            AND source_digest IS NOT NULL
            AND btrim(source_digest) <> ''
            AND fork_cutoff IS NOT NULL
        )
    ),
    CHECK (
        (head_revision = 0 AND head_entry_id IS NULL)
        OR (head_revision > 0 AND head_entry_id IS NOT NULL)
    ),
    PRIMARY KEY (source_coordinate, branch_id),
    FOREIGN KEY (parent_source_coordinate, parent_branch_id)
        REFERENCES dash_agent_branch(source_coordinate, branch_id) ON DELETE RESTRICT
);

CREATE TABLE dash_agent_history (
    source_coordinate TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    ordinal BIGINT NOT NULL CHECK (ordinal > 0),
    entry_id TEXT NOT NULL CHECK (btrim(entry_id) <> ''),
    parent_entry_id TEXT,
    entry JSONB NOT NULL CHECK (jsonb_typeof(entry) = 'object'),
    CHECK (
        (ordinal = 1 AND parent_entry_id IS NULL)
        OR (ordinal > 1 AND parent_entry_id IS NOT NULL AND btrim(parent_entry_id) <> '')
    ),
    PRIMARY KEY (source_coordinate, branch_id, ordinal),
    UNIQUE (source_coordinate, entry_id),
    UNIQUE (source_coordinate, ordinal),
    FOREIGN KEY (source_coordinate, branch_id)
        REFERENCES dash_agent_branch(source_coordinate, branch_id) ON DELETE CASCADE,
    FOREIGN KEY (source_coordinate, parent_entry_id)
        REFERENCES dash_agent_history(source_coordinate, entry_id) ON DELETE RESTRICT
);
ALTER TABLE dash_agent_branch
    ADD FOREIGN KEY (parent_source_coordinate, source_head)
    REFERENCES dash_agent_history(source_coordinate, entry_id) ON DELETE RESTRICT;
ALTER TABLE dash_agent_session
    ADD FOREIGN KEY (source_coordinate, head_entry_id)
    REFERENCES dash_agent_history(source_coordinate, entry_id)
    DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE dash_agent_branch
    ADD FOREIGN KEY (source_coordinate, head_entry_id)
    REFERENCES dash_agent_history(source_coordinate, entry_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE dash_agent_command (
    source_coordinate TEXT NOT NULL
        REFERENCES dash_agent_session(source_coordinate) ON DELETE CASCADE,
    command_id TEXT NOT NULL CHECK (btrim(command_id) <> ''),
    command JSONB NOT NULL CHECK (jsonb_typeof(command) = 'object'),
    PRIMARY KEY (source_coordinate, command_id)
);

CREATE TABLE dash_agent_effect (
    source_coordinate TEXT NOT NULL
        REFERENCES dash_agent_session(source_coordinate) ON DELETE CASCADE,
    effect_id TEXT NOT NULL CHECK (btrim(effect_id) <> ''),
    effect JSONB NOT NULL CHECK (jsonb_typeof(effect) = 'object'),
    PRIMARY KEY (source_coordinate, effect_id)
);

CREATE TABLE dash_agent_change (
    source_coordinate TEXT NOT NULL
        REFERENCES dash_agent_session(source_coordinate) ON DELETE CASCADE,
    revision BIGINT NOT NULL CHECK (revision > 0),
    ordinal BIGINT NOT NULL CHECK (ordinal IN (0, 1)),
    change JSONB NOT NULL CHECK (jsonb_typeof(change) = 'object'),
    PRIMARY KEY (source_coordinate, revision, ordinal),
    FOREIGN KEY (source_coordinate, revision)
        REFERENCES dash_agent_history(source_coordinate, ordinal) ON DELETE CASCADE
);

CREATE TABLE dash_complete_source (
    source_coordinate TEXT PRIMARY KEY CHECK (btrim(source_coordinate) <> ''),
    repository_revision BIGINT NOT NULL CHECK (repository_revision > 0),
    metadata JSONB NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
    FOREIGN KEY (source_coordinate)
        REFERENCES dash_agent_session(source_coordinate) ON DELETE CASCADE
);

CREATE TABLE dash_complete_effect (
    effect_id TEXT PRIMARY KEY CHECK (btrim(effect_id) <> ''),
    request_fingerprint TEXT NOT NULL CHECK (btrim(request_fingerprint) <> ''),
    receipt JSONB NOT NULL CHECK (jsonb_typeof(receipt) = 'object'),
    inspection JSONB NOT NULL CHECK (jsonb_typeof(inspection) = 'object'),
    record JSONB NOT NULL CHECK (jsonb_typeof(record) = 'object')
);
