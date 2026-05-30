ALTER TABLE library_assets DROP CONSTRAINT IF EXISTS library_assets_type_check;
ALTER TABLE library_assets ADD CONSTRAINT library_assets_type_check CHECK (
    asset_type IN ('agent_template', 'mcp_server_template', 'workflow_template', 'skill_template', 'extension_template')
);

ALTER TABLE library_assets DROP CONSTRAINT IF EXISTS library_assets_source_check;
ALTER TABLE library_assets ADD CONSTRAINT library_assets_source_check CHECK (
    source IN ('builtin', 'user_authored', 'remote_imported', 'plugin_embedded')
);

CREATE TABLE IF NOT EXISTS project_extension_installations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    extension_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    config JSONB NOT NULL DEFAULT '{}',
    manifest JSONB NOT NULL,
    installed_library_asset_id TEXT NOT NULL,
    installed_source_ref TEXT NOT NULL,
    installed_source_version TEXT NOT NULL,
    installed_source_digest TEXT NOT NULL,
    installed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT project_extension_installations_unique_key UNIQUE (project_id, extension_key)
);

CREATE INDEX IF NOT EXISTS idx_project_extension_installations_project
    ON project_extension_installations(project_id);

CREATE INDEX IF NOT EXISTS idx_project_extension_installations_source
    ON project_extension_installations(installed_library_asset_id);
