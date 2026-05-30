CREATE TABLE IF NOT EXISTS project_backend_access (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    backend_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    access_mode TEXT NOT NULL DEFAULT 'use_inventory',
    priority INTEGER NOT NULL DEFAULT 0,
    root_policy TEXT NOT NULL DEFAULT '{"kind":"backend_inventory"}',
    capability_policy TEXT NOT NULL DEFAULT '{}',
    note TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, backend_id)
);

CREATE INDEX IF NOT EXISTS idx_project_backend_access_project
    ON project_backend_access(project_id);
CREATE INDEX IF NOT EXISTS idx_project_backend_access_backend
    ON project_backend_access(backend_id);
CREATE INDEX IF NOT EXISTS idx_project_backend_access_status
    ON project_backend_access(status);

CREATE TABLE IF NOT EXISTS backend_workspace_inventory (
    id TEXT PRIMARY KEY,
    backend_id TEXT NOT NULL,
    root_ref TEXT NOT NULL,
    identity_kind TEXT NOT NULL,
    identity_payload TEXT NOT NULL DEFAULT '{}',
    detected_facts TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'available',
    source TEXT NOT NULL DEFAULT 'manual_refresh',
    last_seen_at TEXT NOT NULL,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(backend_id, root_ref)
);

CREATE INDEX IF NOT EXISTS idx_backend_workspace_inventory_backend
    ON backend_workspace_inventory(backend_id);
CREATE INDEX IF NOT EXISTS idx_backend_workspace_inventory_status
    ON backend_workspace_inventory(status);
