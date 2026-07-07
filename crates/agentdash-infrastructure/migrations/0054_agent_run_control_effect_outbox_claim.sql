-- AgentRun control effects are a durable outbox keyed by terminal evidence.
ALTER TABLE public.agent_run_control_effects
    ADD COLUMN IF NOT EXISTS dedup_key text,
    ADD COLUMN IF NOT EXISTS claim_token text,
    ADD COLUMN IF NOT EXISTS claim_owner text,
    ADD COLUMN IF NOT EXISTS claim_expires_at_ms bigint;

UPDATE public.agent_run_control_effects
SET dedup_key = concat_ws(
    ':',
    'runtime_terminal',
    coalesce(delivery_runtime_session_id, ''),
    turn_id,
    terminal_event_seq::text,
    effect_kind,
    id
)
WHERE dedup_key IS NULL;

ALTER TABLE public.agent_run_control_effects
    ALTER COLUMN dedup_key SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_run_control_effects_dedup_key
    ON public.agent_run_control_effects USING btree (dedup_key);

CREATE INDEX IF NOT EXISTS idx_agent_run_control_effects_claim
    ON public.agent_run_control_effects USING btree (status, claim_expires_at_ms, updated_at_ms);
