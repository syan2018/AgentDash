ALTER TABLE canvases
    ADD COLUMN IF NOT EXISTS owner_user_id text,
    ADD COLUMN IF NOT EXISTS scope text DEFAULT 'project'::text,
    ADD COLUMN IF NOT EXISTS published_from_canvas_id text,
    ADD COLUMN IF NOT EXISTS shared_canvas_id text,
    ADD COLUMN IF NOT EXISTS cloned_from_canvas_id text,
    ADD COLUMN IF NOT EXISTS published_at timestamp with time zone,
    ADD COLUMN IF NOT EXISTS published_by_user_id text;

UPDATE canvases
SET scope = 'project'
WHERE scope IS NULL
   OR scope NOT IN ('personal', 'project');

ALTER TABLE canvases
    ALTER COLUMN scope SET DEFAULT 'project',
    ALTER COLUMN scope SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'canvases_scope_check'
    ) THEN
        ALTER TABLE canvases
            ADD CONSTRAINT canvases_scope_check
            CHECK (scope IN ('personal', 'project'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS canvases_project_scope_idx
    ON canvases (project_id, scope);

CREATE INDEX IF NOT EXISTS canvases_project_owner_scope_idx
    ON canvases (project_id, owner_user_id, scope);

CREATE INDEX IF NOT EXISTS canvases_published_from_canvas_id_idx
    ON canvases (published_from_canvas_id);

CREATE INDEX IF NOT EXISTS canvases_shared_canvas_id_idx
    ON canvases (shared_canvas_id);

CREATE INDEX IF NOT EXISTS canvases_cloned_from_canvas_id_idx
    ON canvases (cloned_from_canvas_id);

CREATE UNIQUE INDEX IF NOT EXISTS canvases_project_publication_source_uidx
    ON canvases (published_from_canvas_id)
    WHERE published_from_canvas_id IS NOT NULL
      AND scope = 'project';
