-- Runtime sessions carry project_id for trace grouping; business ownership lives in lifecycle associations.
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS project_id TEXT;
