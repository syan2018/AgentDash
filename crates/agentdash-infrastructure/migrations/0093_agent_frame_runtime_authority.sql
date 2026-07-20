-- Runtime resource authority is derived from the Product binding's exact AgentFrame and current
-- Product relationships. Only the binding remains durable; duplicated surface projections and
-- their activation pin are removed.

DELETE FROM agent_run_product_runtime_recovery_saga;

ALTER TABLE agent_run_product_runtime_binding
    DROP CONSTRAINT agent_run_product_runtime_binding_activation_pins_match,
    DROP COLUMN applied_resource_snapshot_revision;

DROP TABLE agent_run_applied_resource_surface_current;
DROP TABLE agent_run_applied_resource_surface_snapshot;
