ALTER TABLE runtime_health
    DROP COLUMN IF EXISTS workspace_roots;

UPDATE project_backend_access
SET access_mode = 'explicit_grant'
WHERE access_mode = 'use_inventory';

ALTER TABLE project_backend_access
    ALTER COLUMN access_mode SET DEFAULT 'explicit_grant';

UPDATE project_backend_access
SET root_policy = '{"kind":"workspace_registry"}'
WHERE root_policy = '{"kind":"backend_inventory"}';

ALTER TABLE project_backend_access
    ALTER COLUMN root_policy SET DEFAULT '{"kind":"workspace_registry"}';

UPDATE backend_workspace_inventory
SET source = 'manual_register'
WHERE source IN (
    'runtime_register',
    'manual_refresh',
    'scheduled_refresh',
    'capability_expansion_ack'
);

ALTER TABLE backend_workspace_inventory
    ALTER COLUMN source SET DEFAULT 'manual_register';
