-- sessions 表死列清理：bootstrap_state 已迁移到 lifecycle_agents.bootstrap_status，
-- visible_canvas_mount_ids_json 已迁移到 agent_frames.visible_canvas_mount_ids_json。
ALTER TABLE sessions DROP COLUMN IF EXISTS bootstrap_state;
ALTER TABLE sessions DROP COLUMN IF EXISTS visible_canvas_mount_ids_json;
