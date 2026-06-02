-- 支持 find_active_for_agent 直接查询，替代 list_by_run + 全量扫描的启发式选择
CREATE INDEX IF NOT EXISTS idx_agent_assignments_active_agent
    ON agent_assignments (agent_id, lease_status)
    WHERE lease_status = 'active';
