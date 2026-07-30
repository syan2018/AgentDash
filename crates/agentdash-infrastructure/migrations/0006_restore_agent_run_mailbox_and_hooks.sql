CREATE TABLE agent_run_mailbox_receipts (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    client_command_id text NOT NULL,
    request_digest text NOT NULL,
    mailbox_message_id text,
    agent_effect_id text,
    agent_idempotency_key text,
    status text NOT NULL,
    duplicate boolean NOT NULL DEFAULT false,
    result_json jsonb,
    last_error text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    CONSTRAINT agent_run_mailbox_receipts_status_check CHECK (
        status = ANY (ARRAY[
            'accepted'::text,
            'dispatching'::text,
            'applied'::text,
            'not_applied'::text,
            'unknown'::text,
            'failed'::text
        ])
    ),
    CONSTRAINT agent_run_mailbox_receipts_client_command_id_check CHECK (
        btrim(client_command_id) <> ''
    ),
    CONSTRAINT agent_run_mailbox_receipts_request_digest_check CHECK (
        btrim(request_digest) <> ''
    ),
    CONSTRAINT agent_run_mailbox_receipts_owner_command_unique UNIQUE (
        run_id,
        agent_id,
        client_command_id
    )
);

CREATE TABLE agent_run_mailbox_messages (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    delivery_runtime_thread_id text,
    delivery_source_coordinate text,
    delivery_binding_generation bigint,
    delivery_snapshot_revision bigint,
    origin text NOT NULL,
    source_namespace text NOT NULL,
    source_kind text NOT NULL,
    source_ref text,
    source_correlation_ref text,
    source_actor text NOT NULL,
    source_route text,
    source_display_label_key text NOT NULL,
    source_metadata jsonb,
    delivery text NOT NULL,
    delivery_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    barrier text NOT NULL,
    drain_mode text NOT NULL,
    status text NOT NULL,
    priority integer NOT NULL DEFAULT 0,
    order_key bigint NOT NULL,
    source_dedup_key text,
    queued_agent_run_turn_id text,
    consuming_agent_run_turn_id text,
    expected_active_agent_run_turn_id text,
    accepted_agent_run_turn_id text,
    accepted_protocol_turn_id text,
    claim_token text,
    claim_owner text,
    claimed_at timestamp with time zone,
    claim_expires_at timestamp with time zone,
    command_receipt_id text REFERENCES agent_run_mailbox_receipts(id) ON DELETE SET NULL,
    payload_json jsonb,
    executor_config_json jsonb,
    launch_planning_input jsonb,
    preview text NOT NULL DEFAULT '',
    has_images boolean NOT NULL DEFAULT false,
    retain_payload boolean NOT NULL DEFAULT false,
    attempt_count integer NOT NULL DEFAULT 0,
    last_error text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    deleted_at timestamp with time zone,
    CONSTRAINT agent_run_mailbox_messages_origin_check CHECK (
        origin = ANY (ARRAY[
            'user'::text,
            'system'::text,
            'hook'::text,
            'companion'::text,
            'workflow'::text
        ])
    ),
    CONSTRAINT agent_run_mailbox_messages_delivery_check CHECK (
        delivery = ANY (ARRAY[
            'launch_or_continue_turn'::text,
            'steer_active_turn'::text,
            'resume_launch_source'::text
        ])
    ),
    CONSTRAINT agent_run_mailbox_messages_barrier_check CHECK (
        barrier = ANY (ARRAY[
            'immediate_if_idle'::text,
            'agent_loop_turn_boundary'::text,
            'agent_run_turn_boundary'::text,
            'manual_resume'::text
        ])
    ),
    CONSTRAINT agent_run_mailbox_messages_drain_mode_check CHECK (
        drain_mode = ANY (ARRAY['one'::text, 'all'::text])
    ),
    CONSTRAINT agent_run_mailbox_messages_status_check CHECK (
        status = ANY (ARRAY[
            'accepted'::text,
            'queued'::text,
            'ready_to_consume'::text,
            'consuming'::text,
            'dispatched'::text,
            'steered'::text,
            'paused'::text,
            'blocked'::text,
            'failed'::text,
            'deleted'::text
        ])
    ),
    CONSTRAINT agent_run_mailbox_messages_delivery_binding_generation_check CHECK (
        delivery_binding_generation IS NULL OR delivery_binding_generation >= 0
    ),
    CONSTRAINT agent_run_mailbox_messages_delivery_snapshot_revision_check CHECK (
        delivery_snapshot_revision IS NULL OR delivery_snapshot_revision >= 0
    ),
    CONSTRAINT agent_run_mailbox_messages_attempt_count_check CHECK (attempt_count >= 0)
);

ALTER TABLE agent_run_mailbox_receipts
    ADD CONSTRAINT agent_run_mailbox_receipts_message_fkey
    FOREIGN KEY (mailbox_message_id)
    REFERENCES agent_run_mailbox_messages(id)
    ON DELETE SET NULL;

CREATE UNIQUE INDEX agent_run_mailbox_messages_source_dedup
    ON agent_run_mailbox_messages (run_id, agent_id, source_dedup_key)
    WHERE source_dedup_key IS NOT NULL;

CREATE INDEX agent_run_mailbox_messages_owner_order
    ON agent_run_mailbox_messages (
        run_id,
        agent_id,
        priority DESC,
        order_key ASC
    );

CREATE INDEX agent_run_mailbox_messages_claimable
    ON agent_run_mailbox_messages (
        run_id,
        agent_id,
        status,
        barrier,
        drain_mode,
        priority DESC,
        order_key ASC
    )
    WHERE status IN ('accepted', 'queued', 'ready_to_consume');

CREATE INDEX agent_run_mailbox_messages_expired_claim
    ON agent_run_mailbox_messages (claim_expires_at)
    WHERE status = 'consuming';

CREATE TABLE agent_run_mailbox_states (
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    delivery_runtime_thread_id text,
    paused boolean NOT NULL DEFAULT false,
    pause_reason text,
    pause_message text,
    backend_selection_preference jsonb,
    updated_at timestamp with time zone NOT NULL,
    PRIMARY KEY (run_id, agent_id)
);

CREATE TABLE agent_run_mailbox_dispatcher_leases (
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    owner_id text NOT NULL,
    lease_token text NOT NULL,
    fencing_generation bigint NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    PRIMARY KEY (run_id, agent_id),
    CONSTRAINT agent_run_mailbox_dispatcher_owner_id_check CHECK (btrim(owner_id) <> ''),
    CONSTRAINT agent_run_mailbox_dispatcher_lease_token_check CHECK (btrim(lease_token) <> ''),
    CONSTRAINT agent_run_mailbox_dispatcher_generation_check CHECK (fencing_generation > 0)
);

CREATE INDEX agent_run_mailbox_dispatcher_leases_due
    ON agent_run_mailbox_dispatcher_leases (expires_at);

CREATE TABLE agent_run_hook_plans (
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    frame_id text NOT NULL,
    surface_coordinate text NOT NULL,
    plan_digest text NOT NULL,
    plan_json jsonb NOT NULL,
    revision bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    PRIMARY KEY (run_id, agent_id),
    CONSTRAINT agent_run_hook_plans_surface_coordinate_check CHECK (
        btrim(surface_coordinate) <> ''
    ),
    CONSTRAINT agent_run_hook_plans_plan_digest_check CHECK (btrim(plan_digest) <> ''),
    CONSTRAINT agent_run_hook_plans_revision_check CHECK (revision > 0)
);

CREATE TABLE agent_run_hook_runs (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    hook_kind text NOT NULL,
    hook_definition_id text NOT NULL,
    plan_digest text NOT NULL,
    runtime_thread_id text NOT NULL,
    source_coordinate text NOT NULL,
    binding_generation bigint NOT NULL,
    source_turn_id text,
    source_item_id text,
    source_interaction_id text,
    source_sequence bigint,
    status text NOT NULL,
    outcome_json jsonb,
    effect_set_digest text,
    last_error jsonb,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    terminal_at timestamp with time zone,
    CONSTRAINT agent_run_hook_runs_status_check CHECK (
        status = ANY (ARRAY[
            'accepted'::text,
            'running'::text,
            'succeeded'::text,
            'failed'::text
        ])
    ),
    CONSTRAINT agent_run_hook_runs_binding_generation_check CHECK (binding_generation >= 0),
    CONSTRAINT agent_run_hook_runs_identity_unique UNIQUE (
        run_id,
        agent_id,
        hook_kind,
        hook_definition_id,
        runtime_thread_id,
        source_turn_id,
        source_sequence
    )
);

CREATE TABLE agent_run_hook_effects (
    id text PRIMARY KEY,
    hook_run_id text NOT NULL REFERENCES agent_run_hook_runs(id) ON DELETE CASCADE,
    effect_kind text NOT NULL,
    payload_digest text NOT NULL,
    payload_json jsonb NOT NULL,
    idempotency_key text NOT NULL UNIQUE,
    retry_policy_json jsonb NOT NULL,
    status text NOT NULL,
    mailbox_message_id text REFERENCES agent_run_mailbox_messages(id) ON DELETE SET NULL,
    attempt_count integer NOT NULL DEFAULT 0,
    next_attempt_at timestamp with time zone,
    last_error jsonb,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    terminal_at timestamp with time zone,
    CONSTRAINT agent_run_hook_effects_status_check CHECK (
        status = ANY (ARRAY[
            'pending'::text,
            'delivering'::text,
            'applied'::text,
            'unknown'::text,
            'failed'::text
        ])
    ),
    CONSTRAINT agent_run_hook_effects_attempt_count_check CHECK (attempt_count >= 0)
);

CREATE INDEX agent_run_hook_effects_due
    ON agent_run_hook_effects (status, next_attempt_at)
    WHERE status IN ('pending', 'unknown');

CREATE TABLE agent_run_hook_work_leases (
    work_kind text NOT NULL,
    work_id text NOT NULL,
    owner_id text NOT NULL,
    lease_token text NOT NULL,
    fencing_generation bigint NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    PRIMARY KEY (work_kind, work_id),
    CONSTRAINT agent_run_hook_work_leases_kind_check CHECK (
        work_kind = ANY (ARRAY['hook_run'::text, 'hook_effect'::text])
    ),
    CONSTRAINT agent_run_hook_work_leases_generation_check CHECK (fencing_generation > 0)
);

CREATE INDEX agent_run_hook_work_leases_due
    ON agent_run_hook_work_leases (expires_at);
