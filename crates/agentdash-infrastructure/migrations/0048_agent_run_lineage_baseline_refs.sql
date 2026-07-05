ALTER TABLE agent_run_lineages
    ADD COLUMN IF NOT EXISTS parent_frame_id text,
    ADD COLUMN IF NOT EXISTS parent_frame_revision integer,
    ADD COLUMN IF NOT EXISTS child_frame_id text,
    ADD COLUMN IF NOT EXISTS child_frame_revision integer;
