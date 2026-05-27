-- ExtensionTemplate 现在以 package metadata 作为 package artifact 与 runtime projection 的共同身份。
-- 旧 seed / installation manifest 需要先收敛到当前 schema，之后启动 seed 才能安全校验和刷新 digest。

UPDATE library_assets
SET payload = jsonb_set(
    payload || jsonb_build_object(
        'asset_version',
        COALESCE(NULLIF(payload ->> 'asset_version', ''), version)
    ),
    '{package}',
    COALESCE(payload -> 'package', '{}'::jsonb) || jsonb_build_object(
        'name',
        COALESCE(
            NULLIF(payload #>> '{package,name}', ''),
            NULLIF(payload ->> 'extension_id', ''),
            key
        ),
        'version',
        COALESCE(
            NULLIF(payload #>> '{package,version}', ''),
            NULLIF(payload ->> 'asset_version', ''),
            version
        )
    ),
    TRUE
)
WHERE asset_type = 'extension_template'
  AND (
      NOT (payload ? 'package')
      OR NULLIF(payload #>> '{package,name}', '') IS NULL
      OR NULLIF(payload #>> '{package,version}', '') IS NULL
      OR NULLIF(payload ->> 'asset_version', '') IS NULL
  );

UPDATE project_extension_installations
SET manifest = jsonb_set(
    manifest || jsonb_build_object(
        'asset_version',
        COALESCE(
            NULLIF(manifest ->> 'asset_version', ''),
            NULLIF(package_asset_version, ''),
            NULLIF(installed_source_version, ''),
            '0.1.0'
        )
    ),
    '{package}',
    COALESCE(manifest -> 'package', '{}'::jsonb) || jsonb_build_object(
        'name',
        COALESCE(
            NULLIF(manifest #>> '{package,name}', ''),
            NULLIF(package_name, ''),
            NULLIF(manifest ->> 'extension_id', ''),
            extension_key
        ),
        'version',
        COALESCE(
            NULLIF(manifest #>> '{package,version}', ''),
            NULLIF(package_version, ''),
            NULLIF(manifest ->> 'asset_version', ''),
            NULLIF(installed_source_version, ''),
            '0.1.0'
        )
    ),
    TRUE
)
WHERE NOT (manifest ? 'package')
   OR NULLIF(manifest #>> '{package,name}', '') IS NULL
   OR NULLIF(manifest #>> '{package,version}', '') IS NULL
   OR NULLIF(manifest ->> 'asset_version', '') IS NULL;

UPDATE extension_package_artifacts
SET manifest = jsonb_set(
    manifest || jsonb_build_object(
        'asset_version',
        COALESCE(NULLIF(manifest ->> 'asset_version', ''), asset_version)
    ),
    '{package}',
    COALESCE(manifest -> 'package', '{}'::jsonb) || jsonb_build_object(
        'name',
        COALESCE(
            NULLIF(manifest #>> '{package,name}', ''),
            package_name,
            NULLIF(manifest ->> 'extension_id', ''),
            extension_id
        ),
        'version',
        COALESCE(
            NULLIF(manifest #>> '{package,version}', ''),
            package_version,
            NULLIF(manifest ->> 'asset_version', ''),
            asset_version
        )
    ),
    TRUE
)
WHERE NOT (manifest ? 'package')
   OR NULLIF(manifest #>> '{package,name}', '') IS NULL
   OR NULLIF(manifest #>> '{package,version}', '') IS NULL
   OR NULLIF(manifest ->> 'asset_version', '') IS NULL;
