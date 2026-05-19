-- 将旧 Agent + ProjectAgentLink 关联模型收敛为 ProjectAgent 项目实例模型。
--
-- ProjectAgent.id 继承旧 agents.id，避免 Routine / Session 现有 agent_id 绑定失效。
-- 如果旧数据真的存在同一个全局 Agent 被多个 Project 关联，说明数据仍在表达旧模型，
-- 迁移必须 fail-fast，由开发者先手动拆分实例。

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM project_agent_links
        GROUP BY agent_id
        HAVING COUNT(*) > 1
    ) THEN
        RAISE EXCEPTION '无法迁移 ProjectAgent：存在同一个 Agent 被多个 ProjectAgentLink 复用，请先拆分为项目实例';
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS project_agents (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    installed_library_asset_id TEXT,
    installed_source_ref TEXT,
    installed_source_version TEXT,
    installed_source_digest TEXT,
    installed_at TEXT,
    default_lifecycle_key TEXT,
    is_default_for_story BOOLEAN NOT NULL DEFAULT FALSE,
    is_default_for_task BOOLEAN NOT NULL DEFAULT FALSE,
    knowledge_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    project_container_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, name)
);

INSERT INTO project_agents (
    id,
    project_id,
    name,
    agent_type,
    config,
    installed_library_asset_id,
    installed_source_ref,
    installed_source_version,
    installed_source_digest,
    installed_at,
    default_lifecycle_key,
    is_default_for_story,
    is_default_for_task,
    knowledge_enabled,
    project_container_ids,
    created_at,
    updated_at
)
SELECT
    a.id,
    pal.project_id,
    a.name,
    a.agent_type,
    (a.base_config::jsonb || COALESCE(pal.config_override::jsonb, '{}'::jsonb))::text,
    pal.installed_library_asset_id,
    pal.installed_source_ref,
    pal.installed_source_version,
    pal.installed_source_digest,
    pal.installed_at,
    pal.default_lifecycle_key,
    pal.is_default_for_story,
    pal.is_default_for_task,
    COALESCE(pal.knowledge_enabled, FALSE),
    COALESCE(pal.project_container_ids, '[]'),
    LEAST(a.created_at, pal.created_at),
    GREATEST(a.updated_at, pal.updated_at)
FROM project_agent_links pal
JOIN agents a ON a.id = pal.agent_id
ON CONFLICT (id) DO UPDATE SET
    project_id = EXCLUDED.project_id,
    name = EXCLUDED.name,
    agent_type = EXCLUDED.agent_type,
    config = EXCLUDED.config,
    installed_library_asset_id = EXCLUDED.installed_library_asset_id,
    installed_source_ref = EXCLUDED.installed_source_ref,
    installed_source_version = EXCLUDED.installed_source_version,
    installed_source_digest = EXCLUDED.installed_source_digest,
    installed_at = EXCLUDED.installed_at,
    default_lifecycle_key = EXCLUDED.default_lifecycle_key,
    is_default_for_story = EXCLUDED.is_default_for_story,
    is_default_for_task = EXCLUDED.is_default_for_task,
    knowledge_enabled = EXCLUDED.knowledge_enabled,
    project_container_ids = EXCLUDED.project_container_ids,
    updated_at = EXCLUDED.updated_at;

UPDATE inline_fs_files f
SET owner_kind = 'project_agent',
    owner_id = pal.agent_id
FROM project_agent_links pal
WHERE f.owner_kind = 'project_agent_link'
  AND f.owner_id = pal.id;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'routines'
          AND column_name = 'agent_id'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'routines'
          AND column_name = 'project_agent_id'
    ) THEN
        ALTER TABLE routines RENAME COLUMN agent_id TO project_agent_id;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_project_agents_project
    ON project_agents(project_id);

DROP TABLE IF EXISTS project_agent_links;
DROP TABLE IF EXISTS agents;
