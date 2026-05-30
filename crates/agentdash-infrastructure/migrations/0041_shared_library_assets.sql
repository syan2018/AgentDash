CREATE TABLE IF NOT EXISTS library_assets (
    id TEXT PRIMARY KEY,
    asset_type TEXT NOT NULL,
    scope TEXT NOT NULL,
    owner_id TEXT,
    key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL,
    source TEXT NOT NULL,
    source_ref TEXT,
    payload_digest TEXT NOT NULL,
    deprecated BOOLEAN NOT NULL DEFAULT FALSE,
    payload JSONB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT library_assets_type_check CHECK (
        asset_type IN ('agent_template', 'mcp_server_template', 'workflow_template', 'skill_template')
    ),
    CONSTRAINT library_assets_scope_check CHECK (
        scope IN ('builtin', 'system', 'org', 'user')
    ),
    CONSTRAINT library_assets_source_check CHECK (
        source IN ('builtin', 'user_authored', 'remote_imported')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_library_assets_identity
    ON library_assets(asset_type, scope, COALESCE(owner_id, ''), key);

CREATE INDEX IF NOT EXISTS idx_library_assets_asset_type
    ON library_assets(asset_type);

CREATE INDEX IF NOT EXISTS idx_library_assets_scope_owner
    ON library_assets(scope, owner_id);

CREATE INDEX IF NOT EXISTS idx_library_assets_source_ref
    ON library_assets(source_ref);
