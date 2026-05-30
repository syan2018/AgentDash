-- 内联文件独立存储表
-- 统一存储原来嵌套在 Project/Story/LifecycleRun 实体中的文件内容
CREATE TABLE IF NOT EXISTS inline_fs_files (
    id              TEXT PRIMARY KEY,
    owner_kind      TEXT NOT NULL,       -- 'project' | 'story' | 'lifecycle_run' | 'project_agent_link'
    owner_id        TEXT NOT NULL,
    container_id    TEXT NOT NULL,        -- container 标识
    path            TEXT NOT NULL,        -- 归一化文件路径
    content         TEXT NOT NULL,
    updated_at      TEXT NOT NULL,

    UNIQUE(owner_kind, owner_id, container_id, path)
);

CREATE INDEX IF NOT EXISTS idx_inline_fs_files_owner
    ON inline_fs_files(owner_kind, owner_id, container_id);
