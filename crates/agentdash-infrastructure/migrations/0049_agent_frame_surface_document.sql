ALTER TABLE agent_frames
    ADD COLUMN IF NOT EXISTS surface text;

UPDATE agent_frames
SET surface = jsonb_strip_nulls(
    jsonb_build_object(
        'capability_state',
            CASE
                WHEN NULLIF(BTRIM(effective_capability_json), '') IS NULL THEN NULL
                ELSE effective_capability_json::jsonb
            END,
        'context_slice',
            CASE
                WHEN NULLIF(BTRIM(context_slice_json), '') IS NULL THEN NULL
                ELSE context_slice_json::jsonb
            END,
        'vfs_surface',
            CASE
                WHEN NULLIF(BTRIM(vfs_surface_json), '') IS NULL THEN NULL
                ELSE vfs_surface_json::jsonb
            END,
        'mcp_surface',
            CASE
                WHEN NULLIF(BTRIM(mcp_surface_json), '') IS NULL THEN NULL
                ELSE mcp_surface_json::jsonb
            END,
        'execution_profile',
            CASE
                WHEN NULLIF(BTRIM(execution_profile_json), '') IS NULL THEN NULL
                ELSE execution_profile_json::jsonb
            END,
        'visible_canvas_mount_ids',
            CASE
                WHEN NULLIF(BTRIM(visible_canvas_mount_ids_json), '') IS NULL THEN NULL
                ELSE visible_canvas_mount_ids_json::jsonb
            END,
        'visible_workspace_module_refs',
            CASE
                WHEN NULLIF(BTRIM(visible_workspace_module_refs_json), '') IS NULL THEN NULL
                ELSE visible_workspace_module_refs_json::jsonb
            END
    )
)::text
WHERE surface IS NULL;

DROP INDEX IF EXISTS idx_agent_frames_agent_id;
DROP INDEX IF EXISTS idx_agent_frame_transitions_run_phase;
DROP INDEX IF EXISTS idx_agent_run_mailbox_states_delivery_runtime_ref;
