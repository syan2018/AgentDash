-- AgentFrame 的 workspace module 可见性裁切持久化列（Child 3 闭合 Child 1 预留字段）。
-- 与 visible_canvas_mount_ids_json 同构：可空 text，存 JSON 数组字符串（FrameRow 读为 Option<String>）。
-- 空/NULL 表示裁切默认 All；非空 allowlist 经 capability 通道在 agent 侧生效。
ALTER TABLE agent_frames ADD COLUMN IF NOT EXISTS visible_workspace_module_refs_json text;
