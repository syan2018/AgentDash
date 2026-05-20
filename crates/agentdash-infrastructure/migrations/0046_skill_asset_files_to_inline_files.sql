DO $$
BEGIN
    IF to_regclass('public.skill_asset_files') IS NOT NULL THEN
        INSERT INTO inline_fs_files (
            id,
            owner_kind,
            owner_id,
            container_id,
            path,
            content_kind,
            mime_type,
            text_content,
            binary_content,
            size_bytes,
            updated_at
        )
        SELECT
            id,
            'skill_asset',
            skill_asset_id,
            'files',
            path,
            'text',
            NULL,
            content,
            NULL,
            octet_length(content::bytea),
            updated_at
        FROM skill_asset_files
        ON CONFLICT (owner_kind, owner_id, container_id, path) DO UPDATE SET
            content_kind = EXCLUDED.content_kind,
            mime_type = EXCLUDED.mime_type,
            text_content = EXCLUDED.text_content,
            binary_content = EXCLUDED.binary_content,
            size_bytes = EXCLUDED.size_bytes,
            updated_at = EXCLUDED.updated_at;

        DROP TABLE skill_asset_files;
    END IF;
END $$;
