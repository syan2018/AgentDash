ALTER TABLE agent_run_terminal_projection
    DROP COLUMN source_committed_revision,
    DROP COLUMN source_applied_surface_revision,
    DROP COLUMN source_activated_revision;
