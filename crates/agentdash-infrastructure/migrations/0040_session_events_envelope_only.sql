ALTER TABLE session_events
    DROP COLUMN IF EXISTS session_update_type,
    DROP COLUMN IF EXISTS turn_id,
    DROP COLUMN IF EXISTS entry_index,
    DROP COLUMN IF EXISTS tool_call_id;
