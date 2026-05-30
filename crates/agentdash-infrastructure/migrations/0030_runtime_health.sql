CREATE TABLE IF NOT EXISTS runtime_health (
    backend_id TEXT PRIMARY KEY REFERENCES backends(id) ON DELETE CASCADE,
    profile_id TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    version TEXT,
    capabilities JSONB NOT NULL DEFAULT '{}'::jsonb,
    accessible_roots JSONB NOT NULL DEFAULT '[]'::jsonb,
    device JSONB NOT NULL DEFAULT '{}'::jsonb,
    connected_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ,
    disconnected_at TIMESTAMPTZ,
    disconnect_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT runtime_health_status_check CHECK (
        status IN ('online', 'offline', 'starting', 'degraded', 'stopping', 'error')
    )
);

CREATE INDEX IF NOT EXISTS idx_runtime_health_status
    ON runtime_health(status);

CREATE INDEX IF NOT EXISTS idx_runtime_health_last_seen_at
    ON runtime_health(last_seen_at);
