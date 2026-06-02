-- companion 控制面已完全迁移到 LifecycleGate + AgentLineage；
-- CompanionSessionContext 和 companion_context_json 不再被任何代码路径使用。
ALTER TABLE sessions DROP COLUMN IF EXISTS companion_context_json;
