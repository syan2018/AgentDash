-- Host Integration 内嵌资产的 source_ref 使用 integration:{name}:{asset_type}:{key}。
-- 旧 plugin 前缀会让启动期 seed 误判为来源冲突。
UPDATE library_assets
SET source_ref = regexp_replace(source_ref, '^plugin:', 'integration:')
WHERE source = 'integration_embedded'
  AND source_ref LIKE 'plugin:%';
