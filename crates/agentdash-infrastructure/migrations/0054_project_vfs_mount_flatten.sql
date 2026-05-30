-- Flatten ProjectFilespace + ProjectVfsMountBinding into a single ProjectVfsMount entity.
-- Hard cut migration; no compatibility kept.

CREATE TABLE IF NOT EXISTS project_vfs_mounts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    mount_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    capabilities TEXT NOT NULL DEFAULT '[]',
    default_write BOOLEAN NOT NULL DEFAULT FALSE,
    installed_source TEXT,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, mount_id)
);

CREATE INDEX IF NOT EXISTS idx_project_vfs_mounts_project
    ON project_vfs_mounts(project_id);

-- 1) Inline 来源：合并 filespace + binding(filespace) 行
-- mount.id 直接借用 binding.id，让 inline_fs_files.owner_id 指向 binding.id 的行
-- 在 step 3 中只需要一次 UPDATE 就能完成 owner_id 改写。
INSERT INTO project_vfs_mounts (
    id, project_id, mount_id, display_name, description,
    capabilities, default_write, installed_source, content,
    created_at, updated_at
)
SELECT b.id,
       b.project_id,
       b.mount_id,
       b.display_name,
       f.description,
       b.capabilities,
       b.default_write,
       f.installed_source,
       jsonb_build_object('kind', 'inline')::text,
       LEAST(b.created_at, f.created_at),
       GREATEST(b.updated_at, f.updated_at)
FROM project_vfs_mount_bindings b
JOIN project_filespaces f
  ON ((b.source::jsonb)->>'kind' = 'filespace'
      AND ((b.source::jsonb)->>'filespace_id') = f.id)
ON CONFLICT (project_id, mount_id) DO NOTHING;

-- 2) External 来源：直接搬 binding 行
INSERT INTO project_vfs_mounts (
    id, project_id, mount_id, display_name, description,
    capabilities, default_write, installed_source, content,
    created_at, updated_at
)
SELECT b.id,
       b.project_id,
       b.mount_id,
       b.display_name,
       NULL,
       b.capabilities,
       b.default_write,
       NULL,
       jsonb_build_object(
         'kind', 'external_service',
         'service_id', COALESCE((b.source::jsonb)->>'service_id', ''),
         'root_ref',   COALESCE((b.source::jsonb)->>'root_ref', '')
       )::text,
       b.created_at,
       b.updated_at
FROM project_vfs_mount_bindings b
WHERE (b.source::jsonb)->>'kind' = 'external_service'
ON CONFLICT (project_id, mount_id) DO NOTHING;

-- 3) inline_fs_files owner 改写：旧 owner_id 是 filespace.id，新 owner_id 是 mount.id (= binding.id)
UPDATE inline_fs_files i
   SET owner_kind = 'project_vfs_mount',
       owner_id   = b.id
  FROM project_vfs_mount_bindings b
 WHERE i.owner_kind = 'project_filespace'
   AND (b.source::jsonb)->>'kind' = 'filespace'
   AND ((b.source::jsonb)->>'filespace_id') = i.owner_id;

-- 4) DROP 旧表
DROP TABLE IF EXISTS project_vfs_mount_bindings;
DROP TABLE IF EXISTS project_filespaces;

-- 5) Marketplace：直接清掉 filespace_template 行（确认无存量）
DELETE FROM library_assets WHERE asset_type = 'filespace_template';

-- 6) library_assets 类型枚举更新
ALTER TABLE library_assets
    DROP CONSTRAINT IF EXISTS library_assets_type_check;

ALTER TABLE library_assets
    ADD CONSTRAINT library_assets_type_check CHECK (
        asset_type IN (
            'agent_template',
            'mcp_server_template',
            'workflow_template',
            'skill_template',
            'vfs_mount_template',
            'extension_template'
        )
    );
