-- Task execution ownership is projected from lifecycle subject associations.
DROP INDEX IF EXISTS idx_tasks_session_id;
ALTER TABLE tasks DROP COLUMN IF EXISTS session_id;
