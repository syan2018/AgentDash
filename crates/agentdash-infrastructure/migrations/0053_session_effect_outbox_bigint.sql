ALTER TABLE session_terminal_effects
    ALTER COLUMN terminal_event_seq TYPE BIGINT USING terminal_event_seq::bigint,
    ALTER COLUMN attempt_count TYPE BIGINT USING attempt_count::bigint,
    ALTER COLUMN created_at_ms TYPE BIGINT USING created_at_ms::bigint,
    ALTER COLUMN updated_at_ms TYPE BIGINT USING updated_at_ms::bigint;

ALTER TABLE session_runtime_commands
    ALTER COLUMN created_at_ms TYPE BIGINT USING created_at_ms::bigint,
    ALTER COLUMN updated_at_ms TYPE BIGINT USING updated_at_ms::bigint,
    ALTER COLUMN applied_at_ms TYPE BIGINT USING applied_at_ms::bigint,
    ALTER COLUMN failed_at_ms TYPE BIGINT USING failed_at_ms::bigint;
