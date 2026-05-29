CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS project_filespaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    installed_source TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(project_id, key)
);

CREATE INDEX IF NOT EXISTS idx_project_filespaces_project
    ON project_filespaces(project_id);

CREATE TABLE IF NOT EXISTS project_vfs_mount_bindings (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    mount_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    source TEXT NOT NULL,
    capabilities TEXT NOT NULL,
    default_write BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(project_id, mount_id)
);

CREATE INDEX IF NOT EXISTS idx_project_vfs_mount_bindings_project
    ON project_vfs_mount_bindings(project_id);

DO $$
DECLARE
    project_row RECORD;
    container JSONB;
    provider JSONB;
    v_source JSONB;
    file JSONB;
    v_fs_id TEXT;
    v_mount_id TEXT;
    v_display_name TEXT;
    v_now_text TEXT;
BEGIN
    FOR project_row IN SELECT id, config FROM projects LOOP
        IF COALESCE((project_row.config::jsonb ? 'context_containers'), FALSE) THEN
            FOR container IN
                SELECT value FROM jsonb_array_elements(
                    COALESCE(project_row.config::jsonb -> 'context_containers', '[]'::jsonb)
                )
            LOOP
                v_mount_id := NULLIF(BTRIM(container ->> 'mount_id'), '');
                IF v_mount_id IS NULL THEN
                    CONTINUE;
                END IF;

                v_display_name := COALESCE(NULLIF(BTRIM(container ->> 'display_name'), ''), v_mount_id);
                provider := COALESCE(container -> 'provider', '{}'::jsonb);
                v_now_text := now()::text;

                IF provider ->> 'kind' = 'inline_files' THEN
                    v_fs_id := gen_random_uuid()::text;

                    INSERT INTO project_filespaces (
                        id, project_id, key, display_name, description, installed_source, created_at, updated_at
                    )
                    VALUES (
                        v_fs_id, project_row.id, v_mount_id, v_display_name, NULL, NULL, v_now_text, v_now_text
                    )
                    ON CONFLICT (project_id, key) DO NOTHING;

                    SELECT pf.id INTO v_fs_id
                    FROM project_filespaces pf
                    WHERE pf.project_id = project_row.id AND pf.key = v_mount_id;

                    v_source := jsonb_build_object('kind', 'filespace', 'filespace_id', v_fs_id);

                    UPDATE inline_fs_files
                    SET owner_kind = 'project_filespace',
                        owner_id = v_fs_id,
                        container_id = 'files'
                    WHERE owner_kind = 'project'
                      AND owner_id = project_row.id
                      AND container_id = v_mount_id;

                    FOR file IN
                        SELECT value FROM jsonb_array_elements(COALESCE(provider -> 'files', '[]'::jsonb))
                    LOOP
                        IF NULLIF(BTRIM(file ->> 'path'), '') IS NULL THEN
                            CONTINUE;
                        END IF;
                        INSERT INTO inline_fs_files (
                            id, owner_kind, owner_id, container_id, path,
                            content_kind, mime_type, text_content, binary_content, size_bytes, updated_at
                        )
                        VALUES (
                            gen_random_uuid()::text,
                            'project_filespace',
                            v_fs_id,
                            'files',
                            BTRIM(file ->> 'path'),
                            'text',
                            NULL,
                            COALESCE(file ->> 'content', ''),
                            NULL,
                            OCTET_LENGTH(COALESCE(file ->> 'content', '')::text),
                            v_now_text
                        )
                        ON CONFLICT (owner_kind, owner_id, container_id, path) DO NOTHING;
                    END LOOP;
                ELSIF provider ->> 'kind' = 'external_service' THEN
                    v_source := jsonb_build_object(
                        'kind', 'external_service',
                        'service_id', COALESCE(provider ->> 'service_id', ''),
                        'root_ref', COALESCE(provider ->> 'root_ref', '')
                    );
                ELSE
                    CONTINUE;
                END IF;

                INSERT INTO project_vfs_mount_bindings (
                    id, project_id, mount_id, display_name, source,
                    capabilities, default_write, created_at, updated_at
                )
                VALUES (
                    gen_random_uuid()::text,
                    project_row.id,
                    v_mount_id,
                    v_display_name,
                    v_source::text,
                    COALESCE(container -> 'capabilities', '[]'::jsonb)::text,
                    COALESCE((container ->> 'default_write')::boolean, FALSE),
                    v_now_text,
                    v_now_text
                )
                ON CONFLICT (project_id, mount_id) DO NOTHING;
            END LOOP;

            UPDATE projects
            SET config = ((config::jsonb - 'context_containers')::text),
                updated_at = now()::text
            WHERE id = project_row.id;
        END IF;
    END LOOP;
END $$;

ALTER TABLE project_agents
    DROP COLUMN IF EXISTS project_container_ids;

ALTER TABLE library_assets
    DROP CONSTRAINT IF EXISTS library_assets_type_check;

ALTER TABLE library_assets
    ADD CONSTRAINT library_assets_type_check CHECK (
        asset_type IN (
            'agent_template',
            'mcp_server_template',
            'workflow_template',
            'skill_template',
            'filespace_template',
            'extension_template'
        )
    );
