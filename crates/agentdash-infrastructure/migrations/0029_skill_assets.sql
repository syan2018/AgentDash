CREATE TABLE IF NOT EXISTS skill_assets (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL,
    source TEXT NOT NULL,
    builtin_key TEXT,
    disable_model_invocation BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT skill_assets_source_check CHECK (source IN ('builtin_seed', 'user')),
    CONSTRAINT skill_assets_builtin_key_consistency CHECK (
        (source = 'builtin_seed' AND builtin_key IS NOT NULL)
        OR (source = 'user' AND builtin_key IS NULL)
    )
);

CREATE TABLE IF NOT EXISTS skill_asset_files (
    id TEXT PRIMARY KEY,
    skill_asset_id TEXT NOT NULL REFERENCES skill_assets(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    content TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT skill_asset_files_kind_check CHECK (kind IN ('skill', 'reference', 'script', 'asset'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_assets_project_key
    ON skill_assets(project_id, key);

CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_assets_project_builtin_key
    ON skill_assets(project_id, builtin_key)
    WHERE builtin_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_skill_assets_project_id
    ON skill_assets(project_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_asset_files_asset_path
    ON skill_asset_files(skill_asset_id, path);
