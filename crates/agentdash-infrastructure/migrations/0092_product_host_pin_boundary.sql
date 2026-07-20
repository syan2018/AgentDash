-- Product activation owns the immutable Product binding and resource snapshot. Complete-Agent
-- binding identity, generation, compiled surface, and callback route remain Host-owned facts.

ALTER TABLE agent_run_product_runtime_binding
    DROP CONSTRAINT agent_run_product_runtime_binding_activation_pins_match,
    DROP COLUMN applied_resource_binding_id,
    DROP COLUMN applied_resource_binding_generation;

ALTER TABLE agent_run_product_runtime_binding
    ADD CONSTRAINT agent_run_product_runtime_binding_activation_pins_match
    CHECK (
        (
            binding #>> '{source_binding,activated_at_revision}' IS NULL
            AND applied_resource_snapshot_revision IS NULL
        )
        OR (
            binding #>> '{source_binding,activated_at_revision}' IS NOT NULL
            AND applied_resource_snapshot_revision IS NOT NULL
        )
    );
