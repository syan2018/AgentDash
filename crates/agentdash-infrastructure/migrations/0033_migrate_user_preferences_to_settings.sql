DO $$
BEGIN
    IF to_regclass('public.user_preferences') IS NOT NULL THEN
        INSERT INTO settings (scope_kind, scope_id, key, value, updated_at)
        SELECT
            'user',
            users.user_id,
            'agent.mailbox.hide_system_steer_messages',
            ((prefs.value::jsonb ->> 'hide_system_steer_messages')::boolean)::text,
            now()
        FROM user_preferences AS prefs
        CROSS JOIN users
        WHERE prefs.key = 'prefs'
          AND prefs.value::jsonb ? 'hide_system_steer_messages'
        ON CONFLICT (scope_kind, scope_id, key)
        DO UPDATE SET
            value = EXCLUDED.value,
            updated_at = EXCLUDED.updated_at;
    END IF;
END $$;

DROP TABLE IF EXISTS user_preferences;
