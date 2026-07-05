DO $$
BEGIN
    IF to_regclass('public.sessions') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NULL
    THEN
        ALTER TABLE public.sessions RENAME TO runtime_sessions;
    END IF;

    IF to_regclass('public.session_events') IS NOT NULL
        AND to_regclass('public.runtime_session_events') IS NULL
    THEN
        ALTER TABLE public.session_events RENAME TO runtime_session_events;
    END IF;

    IF to_regclass('public.session_compactions') IS NOT NULL
        AND to_regclass('public.runtime_session_compactions') IS NULL
    THEN
        ALTER TABLE public.session_compactions RENAME TO runtime_session_compactions;
    END IF;

    IF to_regclass('public.session_projection_heads') IS NOT NULL
        AND to_regclass('public.runtime_session_projection_heads') IS NULL
    THEN
        ALTER TABLE public.session_projection_heads RENAME TO runtime_session_projection_heads;
    END IF;

    IF to_regclass('public.session_projection_segments') IS NOT NULL
        AND to_regclass('public.runtime_session_projection_segments') IS NULL
    THEN
        ALTER TABLE public.session_projection_segments RENAME TO runtime_session_projection_segments;
    END IF;

    IF to_regclass('public.session_terminal_effects') IS NOT NULL
        AND to_regclass('public.runtime_session_terminal_effects') IS NULL
    THEN
        ALTER TABLE public.session_terminal_effects RENAME TO runtime_session_terminal_effects;
    END IF;

    IF to_regclass('public.session_lineage') IS NOT NULL
        AND to_regclass('public.runtime_session_lineage') IS NULL
    THEN
        ALTER TABLE public.session_lineage RENAME TO runtime_session_lineage;
    END IF;

    IF to_regclass('public.session_runtime_commands') IS NOT NULL
        AND to_regclass('public.runtime_session_delivery_commands') IS NULL
    THEN
        ALTER TABLE public.session_runtime_commands RENAME TO runtime_session_delivery_commands;
    END IF;
END $$;

DO $$
DECLARE
    rename_item record;
BEGIN
    FOR rename_item IN
        SELECT *
        FROM (VALUES
            ('runtime_sessions', 'sessions_pkey', 'runtime_sessions_pkey'),
            ('runtime_session_events', 'session_events_pkey', 'runtime_session_events_pkey'),
            ('runtime_session_compactions', 'session_compactions_pkey', 'runtime_session_compactions_pkey'),
            ('runtime_session_projection_heads', 'session_projection_heads_pkey', 'runtime_session_projection_heads_pkey'),
            ('runtime_session_projection_segments', 'session_projection_segments_pkey', 'runtime_session_projection_segments_pkey'),
            ('runtime_session_projection_segments', 'session_projection_segments_session_kind_version_order_key', 'runtime_session_projection_segments_kind_version_order_key'),
            ('runtime_session_terminal_effects', 'session_terminal_effects_pkey', 'runtime_session_terminal_effects_pkey'),
            ('runtime_session_lineage', 'session_lineage_pkey', 'runtime_session_lineage_pkey'),
            ('runtime_session_lineage', 'session_lineage_check', 'runtime_session_lineage_check'),
            ('runtime_session_delivery_commands', 'session_runtime_commands_pkey', 'runtime_session_delivery_commands_pkey')
        ) AS mappings(table_name, old_name, new_name)
    LOOP
        IF to_regclass('public.' || rename_item.table_name) IS NOT NULL
            AND EXISTS (
                SELECT 1
                FROM pg_constraint
                WHERE conrelid = to_regclass('public.' || rename_item.table_name)
                  AND conname = rename_item.old_name
            )
            AND NOT EXISTS (
                SELECT 1
                FROM pg_constraint
                WHERE conrelid = to_regclass('public.' || rename_item.table_name)
                  AND conname = rename_item.new_name
            )
        THEN
            EXECUTE format(
                'ALTER TABLE public.%I RENAME CONSTRAINT %I TO %I',
                rename_item.table_name,
                rename_item.old_name,
                rename_item.new_name
            );
        END IF;
    END LOOP;
END $$;

DO $$
DECLARE
    rename_item record;
BEGIN
    FOR rename_item IN
        SELECT *
        FROM (VALUES
            ('idx_session_compactions_lifecycle_item', 'idx_runtime_session_compactions_lifecycle_item'),
            ('idx_session_compactions_session_kind_status', 'idx_runtime_session_compactions_session_kind_status'),
            ('idx_session_compactions_source_range', 'idx_runtime_session_compactions_source_range'),
            ('idx_session_lineage_fork_point', 'idx_runtime_session_lineage_fork_point'),
            ('idx_session_lineage_parent_status_kind', 'idx_runtime_session_lineage_parent_status_kind'),
            ('idx_session_projection_heads_active_compaction', 'idx_runtime_session_projection_heads_active_compaction'),
            ('idx_session_projection_segments_projection', 'idx_runtime_session_projection_segments_projection'),
            ('idx_session_projection_segments_source_range', 'idx_runtime_session_projection_segments_source_range'),
            ('idx_session_runtime_commands_frame_transition', 'idx_runtime_session_delivery_commands_frame_transition'),
            ('idx_session_runtime_commands_session_status', 'idx_runtime_session_delivery_commands_session_status'),
            ('idx_session_runtime_commands_status_updated', 'idx_runtime_session_delivery_commands_status_updated'),
            ('idx_session_terminal_effects_session_turn', 'idx_runtime_session_terminal_effects_session_turn'),
            ('idx_session_terminal_effects_status_updated', 'idx_runtime_session_terminal_effects_status_updated'),
            ('idx_session_terminal_effects_terminal_event', 'idx_runtime_session_terminal_effects_terminal_event')
        ) AS mappings(old_name, new_name)
    LOOP
        IF to_regclass('public.' || rename_item.old_name) IS NOT NULL
            AND to_regclass('public.' || rename_item.new_name) IS NULL
        THEN
            EXECUTE format(
                'ALTER INDEX public.%I RENAME TO %I',
                rename_item.old_name,
                rename_item.new_name
            );
        END IF;
    END LOOP;
END $$;

DO $$
BEGIN
    IF to_regclass('public.runtime_session_execution_anchors') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_execution_anchors
            DROP CONSTRAINT IF EXISTS runtime_session_execution_anchors_runtime_session_id_fkey;
        ALTER TABLE ONLY runtime_session_execution_anchors
            ADD CONSTRAINT runtime_session_execution_anchors_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE RESTRICT;
    END IF;

    IF to_regclass('public.agent_run_command_receipts') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY agent_run_command_receipts
            DROP CONSTRAINT IF EXISTS agent_run_delivery_command_receipts_runtime_session_id_fkey;
        ALTER TABLE ONLY agent_run_command_receipts
            DROP CONSTRAINT IF EXISTS agent_run_command_receipts_runtime_session_id_fkey;
        ALTER TABLE ONLY agent_run_command_receipts
            ADD CONSTRAINT agent_run_command_receipts_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.agent_run_mailbox_messages') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY agent_run_mailbox_messages
            DROP CONSTRAINT IF EXISTS agent_run_mailbox_messages_runtime_session_id_fkey;
        ALTER TABLE ONLY agent_run_mailbox_messages
            ADD CONSTRAINT agent_run_mailbox_messages_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.agent_run_mailbox_states') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY agent_run_mailbox_states
            DROP CONSTRAINT IF EXISTS agent_run_mailbox_states_runtime_session_id_fkey;
        ALTER TABLE ONLY agent_run_mailbox_states
            ADD CONSTRAINT agent_run_mailbox_states_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.agent_run_delivery_bindings') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY agent_run_delivery_bindings
            DROP CONSTRAINT IF EXISTS agent_run_delivery_bindings_runtime_session_id_fkey;
        ALTER TABLE ONLY agent_run_delivery_bindings
            ADD CONSTRAINT agent_run_delivery_bindings_runtime_session_id_fkey
            FOREIGN KEY (runtime_session_id) REFERENCES runtime_sessions(id) ON DELETE RESTRICT;
    END IF;

    IF to_regclass('public.runtime_session_events') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_events
            DROP CONSTRAINT IF EXISTS session_events_session_id_fkey;
        ALTER TABLE ONLY runtime_session_events
            DROP CONSTRAINT IF EXISTS runtime_session_events_session_id_fkey;
        ALTER TABLE ONLY runtime_session_events
            ADD CONSTRAINT runtime_session_events_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_compactions') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_compactions
            DROP CONSTRAINT IF EXISTS session_compactions_session_id_fkey;
        ALTER TABLE ONLY runtime_session_compactions
            DROP CONSTRAINT IF EXISTS runtime_session_compactions_session_id_fkey;
        ALTER TABLE ONLY runtime_session_compactions
            ADD CONSTRAINT runtime_session_compactions_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_lineage') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS session_lineage_child_session_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS runtime_session_lineage_child_session_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            ADD CONSTRAINT runtime_session_lineage_child_session_id_fkey
            FOREIGN KEY (child_session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;

        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS session_lineage_parent_session_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS runtime_session_lineage_parent_session_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            ADD CONSTRAINT runtime_session_lineage_parent_session_id_fkey
            FOREIGN KEY (parent_session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_lineage') IS NOT NULL
        AND to_regclass('public.runtime_session_compactions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS session_lineage_fork_point_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            DROP CONSTRAINT IF EXISTS runtime_session_lineage_fork_point_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_lineage
            ADD CONSTRAINT runtime_session_lineage_fork_point_compaction_id_fkey
            FOREIGN KEY (fork_point_compaction_id) REFERENCES runtime_session_compactions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.runtime_session_projection_heads') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_projection_heads
            DROP CONSTRAINT IF EXISTS session_projection_heads_session_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_heads
            DROP CONSTRAINT IF EXISTS runtime_session_projection_heads_session_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_heads
            ADD CONSTRAINT runtime_session_projection_heads_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_projection_heads') IS NOT NULL
        AND to_regclass('public.runtime_session_compactions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_projection_heads
            DROP CONSTRAINT IF EXISTS session_projection_heads_active_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_heads
            DROP CONSTRAINT IF EXISTS runtime_session_projection_heads_active_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_heads
            ADD CONSTRAINT runtime_session_projection_heads_active_compaction_id_fkey
            FOREIGN KEY (active_compaction_id) REFERENCES runtime_session_compactions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.runtime_session_projection_segments') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_projection_segments
            DROP CONSTRAINT IF EXISTS session_projection_segments_session_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_segments
            DROP CONSTRAINT IF EXISTS runtime_session_projection_segments_session_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_segments
            ADD CONSTRAINT runtime_session_projection_segments_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_projection_segments') IS NOT NULL
        AND to_regclass('public.runtime_session_compactions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_projection_segments
            DROP CONSTRAINT IF EXISTS session_projection_segments_generated_by_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_segments
            DROP CONSTRAINT IF EXISTS runtime_session_projection_segments_compaction_id_fkey;
        ALTER TABLE ONLY runtime_session_projection_segments
            ADD CONSTRAINT runtime_session_projection_segments_compaction_id_fkey
            FOREIGN KEY (generated_by_compaction_id) REFERENCES runtime_session_compactions(id) ON DELETE SET NULL;
    END IF;

    IF to_regclass('public.runtime_session_terminal_effects') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_terminal_effects
            DROP CONSTRAINT IF EXISTS session_terminal_effects_session_id_fkey;
        ALTER TABLE ONLY runtime_session_terminal_effects
            DROP CONSTRAINT IF EXISTS runtime_session_terminal_effects_session_id_fkey;
        ALTER TABLE ONLY runtime_session_terminal_effects
            ADD CONSTRAINT runtime_session_terminal_effects_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_delivery_commands') IS NOT NULL
        AND to_regclass('public.runtime_sessions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_delivery_commands
            DROP CONSTRAINT IF EXISTS session_runtime_commands_session_id_fkey;
        ALTER TABLE ONLY runtime_session_delivery_commands
            DROP CONSTRAINT IF EXISTS runtime_session_delivery_commands_session_id_fkey;
        ALTER TABLE ONLY runtime_session_delivery_commands
            ADD CONSTRAINT runtime_session_delivery_commands_session_id_fkey
            FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE;
    END IF;

    IF to_regclass('public.runtime_session_delivery_commands') IS NOT NULL
        AND to_regclass('public.agent_frame_transitions') IS NOT NULL
    THEN
        ALTER TABLE ONLY runtime_session_delivery_commands
            DROP CONSTRAINT IF EXISTS fk_session_runtime_commands_frame_transition;
        ALTER TABLE ONLY runtime_session_delivery_commands
            DROP CONSTRAINT IF EXISTS fk_runtime_session_delivery_commands_frame_transition;
        ALTER TABLE ONLY runtime_session_delivery_commands
            ADD CONSTRAINT fk_runtime_session_delivery_commands_frame_transition
            FOREIGN KEY (frame_transition_id) REFERENCES agent_frame_transitions(id) ON DELETE CASCADE;
    END IF;
END $$;
