ALTER TABLE IF EXISTS agent_run_mailbox_messages
    ADD COLUMN IF NOT EXISTS source_namespace text NOT NULL DEFAULT 'core',
    ADD COLUMN IF NOT EXISTS source_kind text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS source_ref text,
    ADD COLUMN IF NOT EXISTS source_correlation_ref text,
    ADD COLUMN IF NOT EXISTS source_actor text NOT NULL DEFAULT 'system',
    ADD COLUMN IF NOT EXISTS source_route text,
    ADD COLUMN IF NOT EXISTS source_display_label_key text NOT NULL DEFAULT 'mailbox.source.core.unknown',
    ADD COLUMN IF NOT EXISTS source_metadata text;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'agent_run_mailbox_messages'
          AND column_name = 'source'
    ) THEN
        UPDATE agent_run_mailbox_messages
        SET
            source_namespace = CASE source
                WHEN 'routine_executor' THEN 'routine'
                WHEN 'workflow_orchestrator' THEN 'workflow'
                WHEN 'companion_parent_resume' THEN 'companion'
                ELSE 'core'
            END,
            source_kind = CASE source
                WHEN 'routine_executor' THEN 'trigger'
                WHEN 'workflow_orchestrator' THEN 'orchestrator'
                WHEN 'companion_parent_resume' THEN 'parent_resume'
                ELSE source
            END,
            source_actor = CASE source
                WHEN 'composer' THEN 'user'
                WHEN 'draft_start' THEN 'user'
                WHEN 'canvas_action' THEN 'user'
                WHEN 'routine_executor' THEN 'routine'
                WHEN 'companion_parent_resume' THEN 'agent'
                ELSE 'system'
            END,
            source_route = CASE source
                WHEN 'companion_parent_resume' THEN 'parent'
                ELSE NULL
            END,
            source_display_label_key = 'mailbox.source.' ||
                CASE source
                    WHEN 'routine_executor' THEN 'routine'
                    WHEN 'workflow_orchestrator' THEN 'workflow'
                    WHEN 'companion_parent_resume' THEN 'companion'
                    ELSE 'core'
                END ||
                '.' ||
                CASE source
                    WHEN 'routine_executor' THEN 'trigger'
                    WHEN 'workflow_orchestrator' THEN 'orchestrator'
                    WHEN 'companion_parent_resume' THEN 'parent_resume'
                    ELSE source
                END
        WHERE source IS NOT NULL;
    END IF;
END $$;

ALTER TABLE IF EXISTS agent_run_mailbox_messages
    ALTER COLUMN source_namespace DROP DEFAULT,
    ALTER COLUMN source_kind DROP DEFAULT,
    ALTER COLUMN source_actor DROP DEFAULT,
    ALTER COLUMN source_display_label_key DROP DEFAULT;

ALTER TABLE IF EXISTS agent_run_mailbox_messages
    DROP CONSTRAINT IF EXISTS agent_run_mailbox_messages_source_check,
    DROP COLUMN IF EXISTS source;

CREATE INDEX IF NOT EXISTS idx_agent_run_mailbox_messages_source_identity
    ON agent_run_mailbox_messages USING btree (run_id, agent_id, source_namespace, source_kind);
