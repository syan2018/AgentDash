ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS profile_id TEXT;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS device_id TEXT;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS device JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE backends
    ADD COLUMN IF NOT EXISTS last_claimed_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_backends_local_owner_profile_device
    ON backends (owner_user_id, profile_id, device_id)
    WHERE backend_type = 'local'
      AND owner_user_id IS NOT NULL
      AND profile_id IS NOT NULL
      AND device_id IS NOT NULL;
