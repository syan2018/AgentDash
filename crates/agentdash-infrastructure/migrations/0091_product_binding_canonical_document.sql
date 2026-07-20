-- Product Runtime binding uses one canonical JSONB document. Scalar coordinates remain only where
-- PostgreSQL needs owner lookup, uniqueness, or a Product-local foreign key; they are constrained
-- to the document and never used to reconstruct it.

-- Binding digest v1 uses recursive lexicographic JSON canonicalization. Existing resource
-- attestations were produced by order-sensitive bytes and therefore cannot remain trusted.
DELETE FROM agent_run_applied_resource_surface_current;
DELETE FROM agent_run_applied_resource_surface_snapshot;
DELETE FROM agent_run_product_runtime_binding;

ALTER TABLE agent_run_product_runtime_binding
    DROP COLUMN launch_frame_revision,
    DROP COLUMN execution_profile_digest,
    DROP COLUMN execution_profile,
    DROP COLUMN source_ref,
    DROP COLUMN source_committed_revision,
    DROP COLUMN source_applied_surface_revision,
    DROP COLUMN source_activated_revision;

ALTER TABLE agent_run_product_runtime_binding
    ADD CONSTRAINT agent_run_product_runtime_binding_document_coordinates_match
    CHECK (
        binding #>> '{target,run_id}' = target_run_id
        AND binding #>> '{target,agent_id}' = target_agent_id
        AND binding ->> 'runtime_thread_id' = runtime_thread_id
        AND binding #>> '{launch_frame,frame_id}' = launch_frame_id
        AND binding #>> '{launch_frame,agent_id}' = target_agent_id
    ),
    ADD CONSTRAINT agent_run_product_runtime_binding_activation_pins_match
    CHECK (
        (
            binding #>> '{source_binding,activated_at_revision}' IS NULL
            AND applied_resource_snapshot_revision IS NULL
            AND applied_resource_binding_id IS NULL
            AND applied_resource_binding_generation IS NULL
        )
        OR (
            binding #>> '{source_binding,activated_at_revision}' IS NOT NULL
            AND applied_resource_snapshot_revision IS NOT NULL
            AND applied_resource_binding_id IS NOT NULL
            AND btrim(applied_resource_binding_id) <> ''
            AND applied_resource_binding_generation IS NOT NULL
        )
    );
