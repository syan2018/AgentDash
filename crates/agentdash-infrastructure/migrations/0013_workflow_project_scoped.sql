-- 将 workflow/lifecycle 定义归属到 project，移除 status 和 workflow_assignments 表。
-- project_id 默认 '00000000-0000-0000-0000-000000000000' 以兼容遗留无归属数据。

ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS project_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';
ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS project_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

-- 将遗留 assignment 的 project_id 回填到 lifecycle_definitions
UPDATE lifecycle_definitions SET project_id = (
    SELECT wa.project_id FROM workflow_assignments wa WHERE wa.lifecycle_id = lifecycle_definitions.id LIMIT 1
) WHERE EXISTS (
    SELECT 1 FROM workflow_assignments wa WHERE wa.lifecycle_id = lifecycle_definitions.id
);

-- key 唯一约束从全局改为 project 内唯一
-- SQLite 不支持 DROP CONSTRAINT，但 Postgres 支持。
-- 由于项目使用的是 Postgres (PgPool)，直接用 Postgres 语法。
ALTER TABLE workflow_definitions DROP CONSTRAINT IF EXISTS workflow_definitions_key_key;
CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_definitions_project_key ON workflow_definitions(project_id, key);

ALTER TABLE lifecycle_definitions DROP CONSTRAINT IF EXISTS lifecycle_definitions_key_key;
CREATE UNIQUE INDEX IF NOT EXISTS idx_lifecycle_definitions_project_key ON lifecycle_definitions(project_id, key);

-- status 列保留但不再写入，避免破坏 SELECT *。新代码不读取该列。
-- 如需彻底清理: ALTER TABLE workflow_definitions DROP COLUMN status;

-- 删除 workflow_assignments 表
DROP TABLE IF EXISTS workflow_assignments;
