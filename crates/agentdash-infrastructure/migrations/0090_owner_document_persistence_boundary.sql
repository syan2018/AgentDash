-- Each subsystem persists one owner document. Runtime, Complete-Agent Host, and callback
-- revision JSONB values are the durable fact authorities; Product-owned delivery progress lives
-- with the Product binding that consumes those facts.

ALTER TABLE agent_run_product_runtime_binding
    ADD COLUMN change_delivery_state JSONB NOT NULL DEFAULT '{}'::JSONB
        CHECK (jsonb_typeof(change_delivery_state) = 'object');

DROP TABLE agent_runtime_product_change_delivery;

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
DROP TABLE agent_runtime_binding CASCADE;
DROP TABLE agent_runtime_lifecycle_target;

DROP TABLE agent_runtime_surface_snapshot;
DROP TABLE agent_runtime_outbox;
DROP TABLE agent_runtime_change;
DROP TABLE agent_runtime_pending_command;
DROP TABLE agent_runtime_idempotency;
DROP TABLE agent_runtime_operation;
DROP TABLE agent_runtime_thread_binding CASCADE;
DROP TABLE agent_runtime_projection;
DROP TABLE agent_runtime_source_change;
DROP TABLE agent_runtime_source_identity;
DROP TABLE agent_runtime_source_projection;
