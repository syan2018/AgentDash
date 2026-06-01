-- 0012: 收敛早期投影字段到当前 runtime evidence shape。
--
-- lifecycle_runs.current_step_key 已被 ActivityLifecycleRunState 派生的
-- active_node_keys 取代。
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS current_step_key;

-- VFS mount metadata 由 runtime surface projection 生成，关系 schema 无需保存
-- owner-scope 派生字段。

-- stories.context 只保留 story 级上下文事实；文档引用由 project/story context
-- source 显式声明。
UPDATE stories
SET context = ((context::jsonb) - 'prd_doc' - 'spec_refs' - 'resource_list')::text
WHERE (context::jsonb) ?| ARRAY['prd_doc', 'spec_refs', 'resource_list'];

-- AgentProcedure contracts 显式声明 hook_rules，hook metadata 由 frame/runtime
-- snapshot 投影提供。
