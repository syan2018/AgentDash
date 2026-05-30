ALTER TABLE llm_provider_user_credentials
    ADD COLUMN IF NOT EXISTS verification_status TEXT NOT NULL DEFAULT 'unverified',
    ADD COLUMN IF NOT EXISTS verification_message TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS verified_at TEXT;
