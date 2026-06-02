-- Builtin LibraryAsset source_ref 收敛为结构化身份，便于 seed/startup 版本治理与审计。
-- Project installed source 只同步 source_ref；source_version/source_digest 仍保留安装时快照，
-- 用于继续判断 Project 副本是否落后于 Shared Library 来源。

UPDATE library_assets
SET source_ref = 'builtin:' || asset_type || ':' || key
WHERE source = 'builtin'
  AND (source_ref IS NULL OR source_ref = key);

UPDATE mcp_presets p
SET source_ref = la.source_ref
FROM library_assets la
WHERE p.library_asset_id = la.id
  AND la.source = 'builtin'
  AND p.source_ref IS DISTINCT FROM la.source_ref;

UPDATE skill_assets s
SET source_ref = la.source_ref
FROM library_assets la
WHERE s.library_asset_id = la.id
  AND la.source = 'builtin'
  AND s.source_ref IS DISTINCT FROM la.source_ref;

UPDATE workflow_definitions w
SET source_ref = la.source_ref
FROM library_assets la
WHERE w.library_asset_id = la.id
  AND la.source = 'builtin'
  AND w.source_ref IS DISTINCT FROM la.source_ref;

UPDATE lifecycle_definitions l
SET source_ref = la.source_ref
FROM library_assets la
WHERE l.library_asset_id = la.id
  AND la.source = 'builtin'
  AND l.source_ref IS DISTINCT FROM la.source_ref;

UPDATE project_agents a
SET installed_source_ref = la.source_ref
FROM library_assets la
WHERE a.installed_library_asset_id = la.id
  AND la.source = 'builtin'
  AND a.installed_source_ref IS DISTINCT FROM la.source_ref;

UPDATE project_extension_installations e
SET installed_source_ref = la.source_ref
FROM library_assets la
WHERE e.installed_library_asset_id = la.id
  AND la.source = 'builtin'
  AND e.installed_source_ref IS DISTINCT FROM la.source_ref;

UPDATE project_vfs_mounts m
SET installed_source = (
    m.installed_source::jsonb || jsonb_build_object('source_ref', la.source_ref)
)::text
FROM library_assets la
WHERE m.installed_source IS NOT NULL
  AND m.installed_source::jsonb ->> 'library_asset_id' = la.id
  AND la.source = 'builtin'
  AND m.installed_source::jsonb ->> 'source_ref' IS DISTINCT FROM la.source_ref;
