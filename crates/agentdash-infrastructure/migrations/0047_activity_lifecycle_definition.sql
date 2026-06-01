ALTER TABLE workflow_graphs
    ADD COLUMN IF NOT EXISTS entry_activity_key TEXT NOT NULL DEFAULT '';

ALTER TABLE workflow_graphs
    ADD COLUMN IF NOT EXISTS activities TEXT NOT NULL DEFAULT '[]';

ALTER TABLE workflow_graphs
    ADD COLUMN IF NOT EXISTS transitions TEXT NOT NULL DEFAULT '[]';
