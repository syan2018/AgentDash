-- visible_canvas_mount_ids 从 sessions 表迁移到 agent_frames 表
ALTER TABLE agent_frames
ADD COLUMN IF NOT EXISTS visible_canvas_mount_ids_json TEXT;

-- 将 sessions 表中的现有数据迁移到关联的 agent_frames
UPDATE agent_frames
SET visible_canvas_mount_ids_json = s.visible_canvas_mount_ids_json
FROM sessions s
WHERE agent_frames.runtime_session_refs_json IS NOT NULL
  AND s.visible_canvas_mount_ids_json IS NOT NULL
  AND s.visible_canvas_mount_ids_json != '[]'
  AND agent_frames.runtime_session_refs_json::jsonb @> jsonb_build_array(
      jsonb_build_object('kind', 'runtime_session', 'session_id', s.id)
  );

-- sessions 表的 visible_canvas_mount_ids_json 列保留但不再使用
-- 后续可通过独立 migration 删除
