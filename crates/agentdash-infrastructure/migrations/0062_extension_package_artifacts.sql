CREATE TABLE IF NOT EXISTS extension_package_artifacts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    extension_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    package_version TEXT NOT NULL,
    asset_version TEXT NOT NULL,
    source_version TEXT NOT NULL,
    storage_ref TEXT NOT NULL,
    archive_digest TEXT NOT NULL,
    manifest_digest TEXT NOT NULL,
    manifest JSONB NOT NULL,
    byte_size BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT extension_package_artifacts_digest_format CHECK (archive_digest LIKE 'sha256:%'),
    CONSTRAINT extension_package_artifacts_manifest_digest_format CHECK (manifest_digest LIKE 'sha256:%'),
    CONSTRAINT extension_package_artifacts_unique_project_digest UNIQUE (project_id, archive_digest)
);

CREATE INDEX IF NOT EXISTS idx_extension_package_artifacts_project
    ON extension_package_artifacts(project_id);

CREATE INDEX IF NOT EXISTS idx_extension_package_artifacts_extension
    ON extension_package_artifacts(project_id, extension_id);

ALTER TABLE project_extension_installations
    ALTER COLUMN installed_library_asset_id DROP NOT NULL,
    ALTER COLUMN installed_source_ref DROP NOT NULL,
    ALTER COLUMN installed_source_version DROP NOT NULL,
    ALTER COLUMN installed_source_digest DROP NOT NULL,
    ALTER COLUMN installed_at DROP NOT NULL;

ALTER TABLE project_extension_installations
    ADD COLUMN IF NOT EXISTS package_artifact_id TEXT,
    ADD COLUMN IF NOT EXISTS package_name TEXT,
    ADD COLUMN IF NOT EXISTS package_version TEXT,
    ADD COLUMN IF NOT EXISTS package_asset_version TEXT,
    ADD COLUMN IF NOT EXISTS package_source_version TEXT,
    ADD COLUMN IF NOT EXISTS artifact_storage_ref TEXT,
    ADD COLUMN IF NOT EXISTS artifact_archive_digest TEXT,
    ADD COLUMN IF NOT EXISTS artifact_manifest_digest TEXT;

CREATE INDEX IF NOT EXISTS idx_project_extension_installations_artifact
    ON project_extension_installations(package_artifact_id);
