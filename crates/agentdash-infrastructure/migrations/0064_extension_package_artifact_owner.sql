ALTER TABLE extension_package_artifacts
    ADD COLUMN IF NOT EXISTS owner_kind TEXT,
    ADD COLUMN IF NOT EXISTS owner_id TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'extension_package_artifacts'
          AND column_name = 'project_id'
    ) THEN
        UPDATE extension_package_artifacts
        SET owner_kind = COALESCE(owner_kind, 'project'),
            owner_id = COALESCE(owner_id, project_id)
        WHERE owner_kind IS NULL
           OR owner_id IS NULL;
    END IF;
END $$;

ALTER TABLE extension_package_artifacts
    ALTER COLUMN owner_kind SET NOT NULL,
    ALTER COLUMN owner_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'extension_package_artifacts_owner_kind_check'
    ) THEN
        ALTER TABLE extension_package_artifacts
            ADD CONSTRAINT extension_package_artifacts_owner_kind_check
            CHECK (owner_kind IN ('project', 'library_asset'));
    END IF;
END $$;

ALTER TABLE extension_package_artifacts
    DROP CONSTRAINT IF EXISTS extension_package_artifacts_unique_project_digest;

DROP INDEX IF EXISTS idx_extension_package_artifacts_project;
DROP INDEX IF EXISTS idx_extension_package_artifacts_extension;

CREATE UNIQUE INDEX IF NOT EXISTS idx_extension_package_artifacts_owner_digest
    ON extension_package_artifacts(owner_kind, owner_id, archive_digest);

CREATE INDEX IF NOT EXISTS idx_extension_package_artifacts_owner
    ON extension_package_artifacts(owner_kind, owner_id);

CREATE INDEX IF NOT EXISTS idx_extension_package_artifacts_owner_extension
    ON extension_package_artifacts(owner_kind, owner_id, extension_id);

ALTER TABLE extension_package_artifacts
    DROP COLUMN IF EXISTS project_id;
