DO $$
DECLARE
    col record;
BEGIN
    FOR col IN
        SELECT
            table_schema,
            table_name,
            column_name,
            column_default
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND data_type IN ('text', 'character varying', 'character')
          AND column_name LIKE '%\_at' ESCAPE '\'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ALTER COLUMN %I DROP DEFAULT',
            col.table_schema,
            col.table_name,
            col.column_name
        );

        EXECUTE format(
            'ALTER TABLE %I.%I ALTER COLUMN %I TYPE TIMESTAMPTZ USING NULLIF(%I::text, '''')::TIMESTAMPTZ',
            col.table_schema,
            col.table_name,
            col.column_name,
            col.column_name
        );

        IF col.column_default IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.%I ALTER COLUMN %I SET DEFAULT CURRENT_TIMESTAMP',
                col.table_schema,
                col.table_name,
                col.column_name
            );
        END IF;
    END LOOP;
END $$;
