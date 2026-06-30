ALTER TABLE permission_grants
    ADD COLUMN IF NOT EXISTS requested_vfs_access jsonb NOT NULL DEFAULT '[]'::jsonb;
