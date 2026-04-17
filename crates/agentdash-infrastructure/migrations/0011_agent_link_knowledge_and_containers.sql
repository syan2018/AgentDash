-- 替换原 knowledge_containers (Vec<ContextContainerDefinition>) 为精简模型：
--   knowledge_enabled: bool  — 是否启用 Agent 跨 session 知识库
--   project_container_ids: JSON string[]  — 允许此 Agent 访问的项目级容器白名单

-- 1) 移除旧列（如果存在）
ALTER TABLE project_agent_links DROP COLUMN IF EXISTS knowledge_containers;

-- 2) 新增 knowledge_enabled（默认 false — 无知识库是常态）
ALTER TABLE project_agent_links
    ADD COLUMN IF NOT EXISTS knowledge_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- 3) 新增 project_container_ids（默认空数组 — 不继承任何项目容器）
ALTER TABLE project_agent_links
    ADD COLUMN IF NOT EXISTS project_container_ids TEXT NOT NULL DEFAULT '[]';
