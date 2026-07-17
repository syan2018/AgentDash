UPDATE agent_frames
SET surface = surface - 'visible_canvas_mount_ids' - 'visible_workspace_module_refs'
WHERE surface IS NOT NULL;

ALTER TABLE agent_frames
    DROP COLUMN visible_canvas_mount_ids_json,
    DROP COLUMN visible_workspace_module_refs_json;
