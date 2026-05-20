-- inline_fs 支持 text / binary 内容
-- 旧 content TEXT 迁移为 text_content，图片等二进制资产写入 binary_content。

ALTER TABLE inline_fs_files
    ADD COLUMN IF NOT EXISTS content_kind TEXT;

ALTER TABLE inline_fs_files
    ADD COLUMN IF NOT EXISTS mime_type TEXT;

ALTER TABLE inline_fs_files
    ADD COLUMN IF NOT EXISTS text_content TEXT;

ALTER TABLE inline_fs_files
    ADD COLUMN IF NOT EXISTS binary_content BYTEA;

ALTER TABLE inline_fs_files
    ADD COLUMN IF NOT EXISTS size_bytes BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'inline_fs_files'
          AND column_name = 'content'
    ) THEN
        EXECUTE 'UPDATE inline_fs_files SET text_content = content WHERE text_content IS NULL';
        EXECUTE 'UPDATE inline_fs_files SET size_bytes = octet_length(content) WHERE size_bytes IS NULL';
    END IF;
END $$;

UPDATE inline_fs_files
SET content_kind = 'text'
WHERE content_kind IS NULL;

UPDATE inline_fs_files
SET text_content = ''
WHERE content_kind = 'text' AND text_content IS NULL;

UPDATE inline_fs_files
SET size_bytes = CASE
    WHEN content_kind = 'binary' AND binary_content IS NOT NULL THEN octet_length(binary_content)
    WHEN text_content IS NOT NULL THEN octet_length(text_content)
    ELSE 0
END
WHERE size_bytes IS NULL;

ALTER TABLE inline_fs_files
    ALTER COLUMN content_kind SET NOT NULL;

ALTER TABLE inline_fs_files
    ALTER COLUMN size_bytes SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_inline_fs_files_content_kind'
    ) THEN
        ALTER TABLE inline_fs_files
            ADD CONSTRAINT chk_inline_fs_files_content_kind
            CHECK (content_kind IN ('text', 'binary'));
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_inline_fs_files_content_payload'
    ) THEN
        ALTER TABLE inline_fs_files
            ADD CONSTRAINT chk_inline_fs_files_content_payload
            CHECK (
                (content_kind = 'text' AND text_content IS NOT NULL AND binary_content IS NULL)
                OR
                (content_kind = 'binary' AND binary_content IS NOT NULL AND text_content IS NULL AND mime_type IS NOT NULL)
            );
    END IF;
END $$;

ALTER TABLE inline_fs_files
    DROP COLUMN IF EXISTS content;
