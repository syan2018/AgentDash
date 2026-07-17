-- Runtime outbox entries retain the immutable Host binding coordinate used when the command was
-- accepted. ThreadRebind advances the thread's current binding/generation without rewriting that
-- historical dispatch fence.

ALTER TABLE agent_runtime_outbox
    ADD COLUMN binding_id text,
    ADD COLUMN binding_epoch bigint;

UPDATE agent_runtime_outbox
SET binding_id = payload ->> 'binding_id',
    binding_epoch = (payload ->> 'binding_epoch')::bigint;

ALTER TABLE agent_runtime_outbox
    ALTER COLUMN binding_id SET NOT NULL,
    ALTER COLUMN binding_epoch SET NOT NULL,
    ADD CONSTRAINT agent_runtime_outbox_binding_epoch_check
        CHECK (binding_epoch > 0),
    DROP CONSTRAINT agent_runtime_outbox_thread_generation_fkey,
    ADD CONSTRAINT agent_runtime_outbox_binding_generation_fkey
        FOREIGN KEY (binding_id, driver_generation)
        REFERENCES agent_runtime_binding(id, driver_generation)
        ON DELETE RESTRICT;
