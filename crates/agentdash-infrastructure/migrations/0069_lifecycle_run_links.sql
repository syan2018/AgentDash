-- LifecycleRunLink: 显式关联层，替代 session 反查路径
CREATE TABLE IF NOT EXISTS lifecycle_run_links (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_run_id ON lifecycle_run_links(run_id);
CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_subject ON lifecycle_run_links(subject_kind, subject_id);
CREATE INDEX IF NOT EXISTS idx_lifecycle_run_links_subject_role ON lifecycle_run_links(subject_kind, subject_id, role);

-- session_id 改为 nullable：业务归属通过 lifecycle_run_links 表达
-- SQLite 不支持 ALTER COLUMN SET NULL，但该列在 init migration 中已定义为 TEXT（无 NOT NULL 约束）
-- 因此无需 DDL 变更，仅此注释标记语义变化。

-- 历史数据回填：为每条 existing run 通过 session_binding 创建 link
INSERT OR IGNORE INTO lifecycle_run_links (id, run_id, subject_kind, subject_id, role, created_at)
SELECT
    lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)),2) || '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6))),
    lr.id,
    'story',
    sb.owner_id,
    'subject',
    lr.created_at
FROM lifecycle_runs lr
JOIN session_bindings sb ON sb.session_id = lr.session_id AND sb.owner_type = 'story'
WHERE lr.session_id IS NOT NULL;
