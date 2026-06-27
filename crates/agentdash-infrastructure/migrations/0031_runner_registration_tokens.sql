CREATE TABLE IF NOT EXISTS runner_registration_tokens (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_secret_hash TEXT NOT NULL,
    token_prefix TEXT NOT NULL,
    created_by_user_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    last_used_at TIMESTAMPTZ NULL,
    last_claimed_backend_id TEXT NULL,
    default_capability_slot TEXT NOT NULL DEFAULT 'default',
    machine_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runner_registration_tokens_project
    ON runner_registration_tokens (project_id);

CREATE INDEX IF NOT EXISTS idx_runner_registration_tokens_active_project
    ON runner_registration_tokens (project_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_runner_registration_tokens_expires_active
    ON runner_registration_tokens (expires_at)
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_runner_registration_tokens_last_used
    ON runner_registration_tokens (last_used_at);

CREATE INDEX IF NOT EXISTS idx_runner_registration_tokens_last_claimed_backend
    ON runner_registration_tokens (last_claimed_backend_id);
