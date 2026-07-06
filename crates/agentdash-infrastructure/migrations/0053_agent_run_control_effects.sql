-- AgentRun control-plane owns durable terminal follow-up effects.
DO $$
BEGIN
    IF to_regclass('public.agent_run_control_effects') IS NULL
       AND to_regclass('public.runtime_session_terminal_effects') IS NOT NULL THEN
        ALTER TABLE public.runtime_session_terminal_effects RENAME TO agent_run_control_effects;
    END IF;
END $$;

DO $$
BEGIN
    IF to_regclass('public.agent_run_control_effects') IS NOT NULL THEN
        ALTER TABLE public.agent_run_control_effects
            DROP CONSTRAINT IF EXISTS session_terminal_effects_session_id_fkey;
        ALTER TABLE public.agent_run_control_effects
            DROP CONSTRAINT IF EXISTS runtime_session_terminal_effects_session_id_fkey;

        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'agent_run_control_effects'
              AND column_name = 'session_id'
        ) AND NOT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'agent_run_control_effects'
              AND column_name = 'delivery_runtime_session_id'
        ) THEN
            ALTER TABLE public.agent_run_control_effects
                RENAME COLUMN session_id TO delivery_runtime_session_id;
        END IF;

        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'agent_run_control_effects'
              AND column_name = 'effect_type'
        ) AND NOT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'agent_run_control_effects'
              AND column_name = 'effect_kind'
        ) THEN
            ALTER TABLE public.agent_run_control_effects
                RENAME COLUMN effect_type TO effect_kind;
        END IF;

        ALTER TABLE public.agent_run_control_effects
            ADD COLUMN IF NOT EXISTS run_id uuid,
            ADD COLUMN IF NOT EXISTS agent_id uuid,
            ADD COLUMN IF NOT EXISTS frame_id uuid;
    END IF;
END $$;

DO $$
BEGIN
    IF to_regclass('public.agent_run_control_effects') IS NOT NULL THEN
        ALTER TABLE public.agent_run_control_effects
            DROP CONSTRAINT IF EXISTS runtime_session_terminal_effects_pkey;
        ALTER TABLE public.agent_run_control_effects
            DROP CONSTRAINT IF EXISTS session_terminal_effects_pkey;
        IF NOT EXISTS (
            SELECT 1
            FROM pg_constraint
            WHERE conname = 'agent_run_control_effects_pkey'
        ) THEN
            ALTER TABLE public.agent_run_control_effects
                ADD CONSTRAINT agent_run_control_effects_pkey PRIMARY KEY (id);
        END IF;
    END IF;
END $$;

ALTER INDEX IF EXISTS public.idx_runtime_session_terminal_effects_session_turn
    RENAME TO idx_agent_run_control_effects_delivery_turn;
ALTER INDEX IF EXISTS public.idx_runtime_session_terminal_effects_status_updated
    RENAME TO idx_agent_run_control_effects_status_updated;
ALTER INDEX IF EXISTS public.idx_runtime_session_terminal_effects_terminal_event
    RENAME TO idx_agent_run_control_effects_delivery_terminal_event;

CREATE INDEX IF NOT EXISTS idx_agent_run_control_effects_owner
    ON public.agent_run_control_effects USING btree (run_id, agent_id, frame_id);
CREATE INDEX IF NOT EXISTS idx_agent_run_control_effects_delivery_turn
    ON public.agent_run_control_effects USING btree (delivery_runtime_session_id, turn_id);
CREATE INDEX IF NOT EXISTS idx_agent_run_control_effects_delivery_terminal_event
    ON public.agent_run_control_effects USING btree (delivery_runtime_session_id, terminal_event_seq);
CREATE INDEX IF NOT EXISTS idx_agent_run_control_effects_status_updated
    ON public.agent_run_control_effects USING btree (status, updated_at_ms);
