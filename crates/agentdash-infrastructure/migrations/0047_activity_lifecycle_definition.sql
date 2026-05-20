ALTER TABLE lifecycle_definitions
    ADD COLUMN IF NOT EXISTS entry_activity_key TEXT NOT NULL DEFAULT '';

ALTER TABLE lifecycle_definitions
    ADD COLUMN IF NOT EXISTS activities TEXT NOT NULL DEFAULT '[]';

ALTER TABLE lifecycle_definitions
    ADD COLUMN IF NOT EXISTS transitions TEXT NOT NULL DEFAULT '[]';
