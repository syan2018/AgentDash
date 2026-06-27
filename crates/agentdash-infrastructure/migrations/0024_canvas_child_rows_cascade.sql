DELETE FROM canvas_files AS files
WHERE NOT EXISTS (
    SELECT 1
    FROM canvases AS canvas
    WHERE canvas.id = files.canvas_id
);

DELETE FROM canvas_bindings AS bindings
WHERE NOT EXISTS (
    SELECT 1
    FROM canvases AS canvas
    WHERE canvas.id = bindings.canvas_id
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'canvas_files_canvas_id_fkey'
          AND conrelid = 'canvas_files'::regclass
    ) THEN
        ALTER TABLE ONLY canvas_files
            ADD CONSTRAINT canvas_files_canvas_id_fkey
            FOREIGN KEY (canvas_id)
            REFERENCES canvases(id)
            ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'canvas_bindings_canvas_id_fkey'
          AND conrelid = 'canvas_bindings'::regclass
    ) THEN
        ALTER TABLE ONLY canvas_bindings
            ADD CONSTRAINT canvas_bindings_canvas_id_fkey
            FOREIGN KEY (canvas_id)
            REFERENCES canvases(id)
            ON DELETE CASCADE;
    END IF;
END $$;
