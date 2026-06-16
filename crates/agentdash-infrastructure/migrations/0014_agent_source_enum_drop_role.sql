-- 收束 Agent 来源：lifecycle_agents.agent_kind → source（标准化枚举 slug），并删除冗余的 agent_role。
-- agent_role 经全量勘察确认无分支逻辑消费（主从真值源是 AgentLineage 控制树），存量基本恒为 'primary'，删除安全。
-- 幂等：可在已迁移库上重复执行。

DO $$
BEGIN
    -- 1. agent_kind → source（仅当尚未改名时）。
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'lifecycle_agents' AND column_name = 'agent_kind'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'lifecycle_agents' AND column_name = 'source'
    ) THEN
        ALTER TABLE lifecycle_agents RENAME COLUMN agent_kind TO source;
    END IF;

    -- 2. 存量 slug 规范化为 AgentSource 枚举变体。
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'lifecycle_agents' AND column_name = 'source'
    ) THEN
        -- AgentSource 诚实集合：project_agent / routine / subagent / workflow_agent / unknown。
        -- 历史别名归一；已废弃 / 测试遗留 slug（migration_agent / task_agent / workflow_activity / test ...）落 unknown。
        UPDATE lifecycle_agents SET source = CASE source
            WHEN 'project_agent' THEN 'project_agent'
            WHEN 'routine' THEN 'routine'
            WHEN 'routine_agent' THEN 'routine'
            WHEN 'subagent' THEN 'subagent'
            WHEN 'child_agent' THEN 'subagent'
            WHEN 'workflow_agent' THEN 'workflow_agent'
            ELSE 'unknown'
        END;

        ALTER TABLE lifecycle_agents ALTER COLUMN source DROP DEFAULT;
    END IF;

    -- 3. 删除冗余的 agent_role 列。
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'lifecycle_agents' AND column_name = 'agent_role'
    ) THEN
        ALTER TABLE lifecycle_agents DROP COLUMN agent_role;
    END IF;
END $$;
