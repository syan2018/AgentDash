-- 0020: Task 合入 Story aggregate — stories 表新增 tasks JSONB 列
--
-- 对应主线任务 04-27-slim-runtime-layer-session-owner · M1-a
-- 对齐 spec: .trellis/spec/backend/story-task-runtime.md §2.1 / §2.4 / §5
--
-- 变更：
--   1. stories 表新增 tasks JSONB 列（默认 '[]'）
--   2. 将 tasks 表的存量数据按 story_id 聚合写入 stories.tasks
--   3. 用 jsonb_array_length 校准 stories.task_count 冗余列
--
-- 幂等：所有 DDL 使用 IF NOT EXISTS；数据迁移步骤仅在 tasks 表仍有数据且 stories.tasks 为空
-- 时写入（避免重跑覆盖已迁移数据）。
--
-- 回滚参考：
--   - 旧 tasks 表保留（deprecated，不再接受写入），允许在紧急情况下从表数据重建 JSONB 列。
--   - 读路径回退需同时回滚代码层 M1-a 的仓储实现。
--
-- 运行时：Postgres（sqlx::migrate 自动在事务中运行本文件）。

-- ── Step 1: DDL · stories.tasks JSONB 列 ────────────────────────────

ALTER TABLE stories
    ADD COLUMN IF NOT EXISTS tasks JSONB NOT NULL DEFAULT '[]'::jsonb;

-- ── Step 2: 数据迁移 · tasks 表 → stories.tasks ─────────────────────
--
-- 构造的 JSONB 对象字段必须与 Rust `Task` struct 的 serde 序列化形式保持一致：
--   - snake_case 字段名
--   - workspace_id 允许 null
--   - agent_binding / artifacts 在表中已是 JSON 字符串（TEXT），反解析后再嵌入
--   - status 为枚举字符串（小写 snake_case）
--   - created_at / updated_at 为 RFC3339 字符串
--
-- 注意：仅当 stories.tasks 当前为空数组（默认值）时才迁移，避免重跑覆盖。

DO $$
DECLARE
    story_row RECORD;
    task_row RECORD;
    tasks_array JSONB;
    task_obj JSONB;
    agent_binding_json JSONB;
    artifacts_json JSONB;
BEGIN
    FOR story_row IN
        SELECT id FROM stories
        WHERE tasks = '[]'::jsonb OR tasks IS NULL
    LOOP
        tasks_array := '[]'::jsonb;

        FOR task_row IN
            SELECT id, project_id, story_id, workspace_id, title, description,
                   status,
                   agent_binding, artifacts, created_at, updated_at
            FROM tasks
            WHERE story_id = story_row.id
            ORDER BY created_at ASC
        LOOP
            -- agent_binding / artifacts 在旧表中是 JSON 字符串，需要先解析为 JSONB
            BEGIN
                agent_binding_json := (task_row.agent_binding)::jsonb;
            EXCEPTION WHEN OTHERS THEN
                agent_binding_json := '{}'::jsonb;
                RAISE WARNING '迁移 task % 时 agent_binding 解析失败，使用空对象兜底', task_row.id;
            END;

            BEGIN
                artifacts_json := (task_row.artifacts)::jsonb;
            EXCEPTION WHEN OTHERS THEN
                artifacts_json := '[]'::jsonb;
                RAISE WARNING '迁移 task % 时 artifacts 解析失败，使用空数组兜底', task_row.id;
            END;

            task_obj := jsonb_build_object(
                'id', task_row.id,
                'project_id', task_row.project_id,
                'story_id', task_row.story_id,
                'workspace_id', task_row.workspace_id,
                'title', task_row.title,
                'description', task_row.description,
                'status', task_row.status,
                'agent_binding', agent_binding_json,
                'artifacts', artifacts_json,
                'created_at', task_row.created_at,
                'updated_at', task_row.updated_at
            );

            tasks_array := tasks_array || task_obj;
        END LOOP;

        IF jsonb_array_length(tasks_array) > 0 THEN
            UPDATE stories
            SET tasks = tasks_array
            WHERE id = story_row.id;

            RAISE NOTICE 'migrated story % with % tasks',
                story_row.id, jsonb_array_length(tasks_array);
        END IF;
    END LOOP;
END $$;

-- ── Step 3: 校准 task_count 冗余列 ──────────────────────────────────

UPDATE stories
SET task_count = jsonb_array_length(tasks)
WHERE task_count <> jsonb_array_length(tasks);

-- ── Step 4: 注释标记旧 tasks 表 deprecated ──────────────────────────
--
-- 不 DROP 旧表，保留作只读回滚参照。表注释可被运维工具发现。

COMMENT ON TABLE tasks IS
    'DEPRECATED（自 migration 0020 起）: Task 合入 Story aggregate（stories.tasks JSONB）；本表仅作只读回滚参照，不再接受写入。';
