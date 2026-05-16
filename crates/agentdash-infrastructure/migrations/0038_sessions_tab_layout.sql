ALTER TABLE sessions
ADD COLUMN IF NOT EXISTS tab_layout_json TEXT;
