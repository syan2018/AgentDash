ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS machine_id TEXT;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS machine_label TEXT;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS legacy_machine_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'private';

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS share_scope_kind TEXT NOT NULL DEFAULT 'user';

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS share_scope_id TEXT;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS capability_slot TEXT NOT NULL DEFAULT 'default';

UPDATE backends
   SET machine_id = COALESCE(machine_id, device_id),
       machine_label = COALESCE(machine_label, name),
       share_scope_id = COALESCE(share_scope_id, owner_user_id),
       legacy_machine_ids = CASE
           WHEN device_id IS NOT NULL
                AND (machine_id IS NULL OR machine_id <> device_id)
           THEN jsonb_build_array(device_id)
           ELSE COALESCE(legacy_machine_ids, '[]'::jsonb)
       END
 WHERE backend_type = 'local';

DROP INDEX IF EXISTS idx_backends_local_owner_profile_device;

CREATE UNIQUE INDEX IF NOT EXISTS idx_backends_local_machine_scope_slot
    ON backends (machine_id, share_scope_kind, COALESCE(share_scope_id, ''), capability_slot)
    WHERE backend_type = 'local'
      AND machine_id IS NOT NULL
      AND share_scope_kind IS NOT NULL
      AND capability_slot IS NOT NULL;
