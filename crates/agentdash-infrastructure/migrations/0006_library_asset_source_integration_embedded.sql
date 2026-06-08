-- Host Integration 内嵌资产的持久化 source 使用 integration_embedded。
-- 数据库约束与既有行同步到当前领域枚举，保证 seed 写入和 repository 映射使用同一事实。
ALTER TABLE library_assets
    DROP CONSTRAINT IF EXISTS library_assets_source_check;

UPDATE library_assets
SET source = 'integration_embedded'
WHERE source = 'plugin_embedded';

ALTER TABLE library_assets
    ADD CONSTRAINT library_assets_source_check
    CHECK (source = ANY (ARRAY[
        'builtin'::text,
        'user_authored'::text,
        'remote_imported'::text,
        'integration_embedded'::text
    ]));
