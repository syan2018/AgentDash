-- 0012: 斩草除根清理兼容包袱（对应代码层 commits 中的 #2/#4/#5/#8/#9）
--
-- 本迁移是安全的"物理清理"：代码已经全部停止读写这些字段，执行前代码必须
-- 先上线。跑这个 migration 前请确认：
--   1. 所有服务都已部署到移除兼容包袱的版本；
--   2. 没有正在运行的老版本 session / lifecycle run 还在读取这些列。
--
-- 运行时：Postgres（sqlx::migrate 会为每个文件包一层事务，这里不再显式 BEGIN/COMMIT）。

-- #2 lifecycle_runs.current_step_key —— 字段已被 active_node_keys 完全取代
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS current_step_key;

-- #4 VFS mount metadata 里冗余的 owner_scope key —— 代码已经不再双写
--    （mount metadata 不落库、由服务运行时构造，此处仅作记录提示）
--
--    如果未来 mount metadata 有被 cached 到表中，在此追加清理 JSON 的 UPDATE。

-- #8 stories.context 里的 prd_doc / spec_refs / resource_list —— 代码已不再消费
--    stories.context 列类型为 TEXT，存 JSON 字符串；Postgres 下用 jsonb 做 key 去除。
UPDATE stories
SET context = ((context::jsonb) - 'prd_doc' - 'spec_refs' - 'resource_list')::text
WHERE (context::jsonb) ?| ARRAY['prd_doc', 'spec_refs', 'resource_list'];

-- #9 session hook metadata 里的 primary_workflow_key —— 仅存在于 snapshot/事件流
--    不在关系型列中，无需 DDL。历史事件负载里遗留的键可忽略，serde 会跳过。

-- #3 workflow_definitions.contract 里的 legacy constraints/checks —— 代码改为
--    「必须显式声明 hook_rules」。此处不做强制迁移：已有 workflow 若依赖旧
--    constraints/checks 推导 hook_rules，将需要人工更新 contract。可以用如下
--    查询找出候选：
--      SELECT id, key FROM workflow_definitions
--      WHERE (contract::jsonb) -> 'hook_rules' IS NULL
--        AND (contract::jsonb) #> '{completion,checks}' IS NOT NULL;
