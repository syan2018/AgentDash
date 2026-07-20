-- Managed Runtime is the sole owner of applied source evidence.
--
-- Product bindings persist Product intent and the target-to-thread association only. Workspace
-- presentation provenance keeps the source coordinate inside its canonical JSON document instead
-- of duplicating Runtime projection revisions into relational columns.

DELETE FROM agent_run_product_runtime_recovery_saga;
DELETE FROM agent_run_product_runtime_binding;

DELETE FROM workspace_module_presentation_ack;
DELETE FROM workspace_module_presentation_outbox;
DELETE FROM workspace_module_presentation_change;
DELETE FROM workspace_module_presentation_intent;
DELETE FROM workspace_module_presentation_head;

ALTER TABLE workspace_module_presentation_intent
    DROP COLUMN source_ref,
    DROP COLUMN source_committed_revision,
    DROP COLUMN source_applied_surface_revision,
    DROP COLUMN source_activated_revision;
