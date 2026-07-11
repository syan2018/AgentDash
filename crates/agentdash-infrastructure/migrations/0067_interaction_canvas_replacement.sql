CREATE TABLE interaction_definitions (
    id uuid PRIMARY KEY,
    project_id text NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    kind text NOT NULL CHECK (kind = 'canvas'),
    current_revision_id uuid NOT NULL,
    status text NOT NULL CHECK (status IN ('active', 'archived')),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CHECK (owner_kind <> 'project' OR owner_id = project_id)
);

CREATE INDEX idx_interaction_definitions_project_catalog
    ON interaction_definitions (project_id, kind, status, updated_at DESC, id);
CREATE INDEX idx_interaction_definitions_owner
    ON interaction_definitions (owner_kind, owner_id, updated_at DESC, id);

CREATE TABLE interaction_source_bundles (
    digest text PRIMARY KEY CHECK (digest ~ '^sha256:[0-9a-fA-F]{64}$'),
    format_version smallint NOT NULL CHECK (format_version = 1),
    entry_file text NOT NULL,
    sandbox jsonb NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE TABLE interaction_source_files (
    source_bundle_digest text NOT NULL REFERENCES interaction_source_bundles(digest) ON DELETE RESTRICT,
    path text NOT NULL,
    content text NOT NULL,
    media_type text,
    PRIMARY KEY (source_bundle_digest, path)
);

CREATE TABLE interaction_definition_revisions (
    revision_id uuid PRIMARY KEY,
    definition_id uuid NOT NULL REFERENCES interaction_definitions(id) ON DELETE RESTRICT,
    revision_number bigint NOT NULL CHECK (revision_number > 0),
    project_id text NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    source_bundle_digest text NOT NULL REFERENCES interaction_source_bundles(digest) ON DELETE RESTRICT,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (definition_id, revision_number),
    UNIQUE (revision_id, definition_id),
    CHECK (owner_kind <> 'project' OR owner_id = project_id)
);

ALTER TABLE interaction_definitions
    ADD CONSTRAINT interaction_definitions_current_revision_fkey
    FOREIGN KEY (current_revision_id, id)
    REFERENCES interaction_definition_revisions(revision_id, definition_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_interaction_definition_revisions_definition
    ON interaction_definition_revisions (definition_id, revision_number DESC);

CREATE TABLE interaction_definition_lineage (
    definition_revision_id uuid PRIMARY KEY REFERENCES interaction_definition_revisions(revision_id) ON DELETE CASCADE,
    lineage_kind text NOT NULL CHECK (lineage_kind IN ('published_from', 'copied_from')),
    source_definition_id uuid NOT NULL REFERENCES interaction_definitions(id) ON DELETE RESTRICT,
    source_revision_id uuid NOT NULL REFERENCES interaction_definition_revisions(revision_id) ON DELETE RESTRICT,
    source_bundle_digest text NOT NULL REFERENCES interaction_source_bundles(digest) ON DELETE RESTRICT
);

CREATE INDEX idx_interaction_definition_lineage_source
    ON interaction_definition_lineage (source_definition_id, lineage_kind, definition_revision_id);

CREATE TABLE interaction_instances (
    id uuid PRIMARY KEY,
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'project')),
    owner_id text NOT NULL,
    definition_id uuid NOT NULL REFERENCES interaction_definitions(id) ON DELETE RESTRICT,
    definition_revision_id uuid NOT NULL REFERENCES interaction_definition_revisions(revision_id) ON DELETE RESTRICT,
    contract_version smallint NOT NULL CHECK (contract_version = 1),
    state_revision bigint NOT NULL CHECK (state_revision >= 0),
    status text NOT NULL CHECK (status IN ('open', 'closed')),
    state jsonb NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    closed_at timestamptz,
    CHECK ((status = 'open' AND closed_at IS NULL) OR (status = 'closed' AND closed_at IS NOT NULL))
);

CREATE INDEX idx_interaction_instances_owner
    ON interaction_instances (owner_kind, owner_id, status, updated_at DESC, id);
CREATE INDEX idx_interaction_instances_definition_revision
    ON interaction_instances (definition_revision_id, status, id);

CREATE TABLE interaction_state_revisions (
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    state_revision bigint NOT NULL CHECK (state_revision >= 0),
    source_event_id uuid,
    state jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (instance_id, state_revision),
    UNIQUE (source_event_id)
);

CREATE TABLE interaction_attachments (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    subject_kind text NOT NULL CHECK (subject_kind IN ('agent_run', 'user_workshop', 'workflow_run')),
    subject_id text NOT NULL,
    role text NOT NULL CHECK (role IN ('editor', 'observer', 'renderer', 'automation')),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    detached_at timestamptz
);

CREATE UNIQUE INDEX interaction_attachments_active_subject_unique
    ON interaction_attachments (instance_id, subject_kind, subject_id)
    WHERE detached_at IS NULL;
CREATE INDEX idx_interaction_attachments_subject
    ON interaction_attachments (subject_kind, subject_id, detached_at, instance_id);

CREATE TABLE interaction_runtime_bindings (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    attachment_id uuid REFERENCES interaction_attachments(id) ON DELETE CASCADE,
    attachment_scope text GENERATED ALWAYS AS (COALESCE(attachment_id::text, '')) STORED,
    slot_key text NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (instance_id, attachment_scope, slot_key)
);

CREATE INDEX idx_interaction_runtime_bindings_instance
    ON interaction_runtime_bindings (instance_id, attachment_scope, slot_key);

CREATE TABLE interaction_presentation_states (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    user_id text NOT NULL,
    presentation_key text NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (instance_id, user_id, presentation_key)
);

CREATE TABLE interaction_renderer_leases (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    renderer_key text NOT NULL,
    user_id text NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    acquired_at timestamptz NOT NULL,
    renewed_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (renewed_at >= acquired_at),
    CHECK (expires_at > renewed_at),
    CHECK (expires_at <= renewed_at + interval '5 minutes'),
    UNIQUE (instance_id, renderer_key)
);

CREATE INDEX idx_interaction_renderer_leases_active
    ON interaction_renderer_leases (instance_id, expires_at);

CREATE TABLE interaction_events (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    sequence bigint NOT NULL CHECK (sequence > 0),
    command_id uuid NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (instance_id, sequence),
    UNIQUE (instance_id, command_id)
);

ALTER TABLE interaction_state_revisions
    ADD CONSTRAINT interaction_state_revisions_source_event_fkey
    FOREIGN KEY (source_event_id) REFERENCES interaction_events(id) ON DELETE RESTRICT;

CREATE TABLE interaction_operation_effect_intents (
    effect_id uuid PRIMARY KEY,
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    source_event_id uuid NOT NULL UNIQUE REFERENCES interaction_events(id) ON DELETE RESTRICT,
    status text NOT NULL CHECK (status IN ('pending', 'claimed', 'succeeded', 'retry_scheduled', 'terminal_failed')),
    next_attempt_at timestamptz NOT NULL,
    claim_token uuid,
    claim_expires_at timestamptz,
    document jsonb NOT NULL,
    CHECK ((status = 'claimed' AND claim_token IS NOT NULL AND claim_expires_at IS NOT NULL)
        OR (status <> 'claimed' AND claim_token IS NULL AND claim_expires_at IS NULL))
);

CREATE INDEX idx_interaction_effect_intents_claim
    ON interaction_operation_effect_intents (status, next_attempt_at, claim_expires_at, effect_id);

CREATE TABLE interaction_command_receipts (
    instance_id uuid NOT NULL REFERENCES interaction_instances(id) ON DELETE CASCADE,
    command_id uuid NOT NULL,
    request_digest text NOT NULL,
    event_id uuid NOT NULL UNIQUE REFERENCES interaction_events(id) ON DELETE RESTRICT,
    effect_id uuid REFERENCES interaction_operation_effect_intents(effect_id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (instance_id, command_id)
);

ALTER TABLE agent_frames
    DROP COLUMN IF EXISTS visible_canvas_mount_ids_json;

DROP TABLE IF EXISTS agent_run_canvas_interaction_snapshots;
DROP TABLE IF EXISTS agent_run_canvas_runtime_observations;
DROP TABLE IF EXISTS canvas_bindings;
DROP TABLE IF EXISTS canvas_files;
DROP TABLE IF EXISTS canvases;
