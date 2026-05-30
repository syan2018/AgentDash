CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS lifecycle_run_links (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_run_id
    ON lifecycle_run_links(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_subject
    ON lifecycle_run_links(subject_kind, subject_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_subject_role
    ON lifecycle_run_links(subject_kind, subject_id, role);

ALTER TABLE lifecycle_runs
    ALTER COLUMN session_id DROP NOT NULL;

INSERT INTO lifecycle_run_links (id, run_id, subject_kind, subject_id, role, created_at)
SELECT
    gen_random_uuid()::text,
    lr.id,
    'story',
    sb.owner_id,
    'subject',
    lr.created_at
FROM lifecycle_runs lr
JOIN session_bindings sb
    ON sb.session_id = lr.session_id
   AND sb.owner_type = 'story'
WHERE lr.session_id IS NOT NULL
ON CONFLICT DO NOTHING;
