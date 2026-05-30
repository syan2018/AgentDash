ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS remote_source_url TEXT;

ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS remote_imported_at TEXT;

ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS remote_digest TEXT;

ALTER TABLE skill_assets
DROP CONSTRAINT IF EXISTS skill_assets_source_check;

ALTER TABLE skill_assets
ADD CONSTRAINT skill_assets_source_check
CHECK (source IN ('builtin_seed', 'user', 'github'));

ALTER TABLE skill_assets
DROP CONSTRAINT IF EXISTS skill_assets_builtin_key_consistency;

ALTER TABLE skill_assets
ADD CONSTRAINT skill_assets_builtin_key_consistency
CHECK (
    (source = 'builtin_seed' AND builtin_key IS NOT NULL)
    OR (source <> 'builtin_seed' AND builtin_key IS NULL)
);
