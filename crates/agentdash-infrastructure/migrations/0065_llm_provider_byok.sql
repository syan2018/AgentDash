ALTER TABLE llm_providers
    ADD COLUMN IF NOT EXISTS credential_mode TEXT NOT NULL DEFAULT 'global_only',
    ADD COLUMN IF NOT EXISTS global_api_key_ciphertext TEXT NOT NULL DEFAULT '';

ALTER TABLE llm_providers
    DROP COLUMN IF EXISTS api_key;

CREATE TABLE IF NOT EXISTS llm_provider_user_credentials (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES llm_providers(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    api_key_ciphertext TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(provider_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_llm_provider_user_credentials_user
    ON llm_provider_user_credentials(user_id);

CREATE INDEX IF NOT EXISTS idx_llm_provider_user_credentials_provider
    ON llm_provider_user_credentials(provider_id);
