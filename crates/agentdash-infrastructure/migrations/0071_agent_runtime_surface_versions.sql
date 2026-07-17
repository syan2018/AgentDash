ALTER TABLE agent_runtime_surface_snapshot
    DROP CONSTRAINT agent_runtime_surface_snapshot_pkey;

ALTER TABLE agent_runtime_surface_snapshot
    ADD PRIMARY KEY (binding_id, surface_revision, surface_digest);
