-- LifecycleAgent bootstrap_status: 取代 SessionMeta.bootstrap_state
-- "pending" = 等待首次 owner context bootstrap
-- "bootstrapped" = 已完成
-- "not_applicable" = 不需要 (companion child / reuse 场景)

ALTER TABLE lifecycle_agents
    ADD COLUMN IF NOT EXISTS bootstrap_status TEXT NOT NULL DEFAULT 'not_applicable';

-- 已有 agent 如果 status=active 且关联的 session 曾完成 bootstrap，标记为 bootstrapped
UPDATE lifecycle_agents SET bootstrap_status = 'bootstrapped' WHERE status = 'active';
