UPDATE library_assets
SET deprecated = TRUE,
    updated_at = NOW()
WHERE asset_type = 'skill_template'
  AND scope = 'builtin'
  AND source = 'builtin'
  AND key = ANY (ARRAY[
    'canvas-system',
    'workspace-module-system',
    'companion-system',
    'routine-memory',
    'memory-manager'
  ]);

UPDATE skill_assets
SET source = 'builtin_seed',
    builtin_key = key,
    library_asset_id = NULL,
    source_ref = NULL,
    source_version = NULL,
    source_digest = NULL,
    installed_at = NULL,
    updated_at = NOW()
WHERE source = 'user'
  AND key = ANY (ARRAY[
    'canvas-system',
    'workspace-module-system',
    'companion-system',
    'routine-memory',
    'memory-manager'
  ])
  AND source_ref = 'builtin:skill_template:' || key;
