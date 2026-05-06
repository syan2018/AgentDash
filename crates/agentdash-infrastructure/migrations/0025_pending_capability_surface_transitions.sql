ALTER TABLE sessions
ADD COLUMN IF NOT EXISTS pending_capability_surface_transitions_json TEXT NOT NULL DEFAULT '[]';
