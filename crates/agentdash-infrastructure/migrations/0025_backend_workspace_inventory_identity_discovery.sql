ALTER TABLE backend_workspace_inventory
    DROP CONSTRAINT IF EXISTS backend_workspace_inventory_source_check;

ALTER TABLE backend_workspace_inventory
    ADD CONSTRAINT backend_workspace_inventory_source_check
    CHECK (source = ANY (ARRAY[
        'manual_register'::text,
        'identity_discovery'::text
    ]));
