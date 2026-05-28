UPDATE session_events
SET notification_json = jsonb_set(
    notification_json::jsonb,
    '{event,payload,startedAtMs}',
    to_jsonb(occurred_at_ms),
    true
)::text
WHERE session_update_type = 'item_started'
  AND notification_json::jsonb #> '{event,payload,startedAtMs}' IS NULL;

UPDATE session_events
SET notification_json = jsonb_set(
    notification_json::jsonb,
    '{event,payload,completedAtMs}',
    to_jsonb(occurred_at_ms),
    true
)::text
WHERE session_update_type = 'item_completed'
  AND notification_json::jsonb #> '{event,payload,completedAtMs}' IS NULL;

WITH usage_events AS (
    SELECT
        session_id,
        event_seq,
        notification_json::jsonb AS notification
    FROM session_events
    WHERE session_update_type = 'token_usage_updated'
      AND notification_json::jsonb #> '{event,payload,tokenUsage,context}' IS NULL
)
UPDATE session_events AS target
SET notification_json = jsonb_set(
    usage_events.notification,
    '{event,payload,tokenUsage,context}',
    jsonb_build_object(
        'providerContextTokens',
        GREATEST(
            COALESCE((usage_events.notification #>> '{event,payload,tokenUsage,last,totalTokens}')::bigint, 0),
            0
        ),
        'pendingEstimateTokens',
        0,
        'currentContextTokens',
        GREATEST(
            COALESCE((usage_events.notification #>> '{event,payload,tokenUsage,last,totalTokens}')::bigint, 0),
            0
        ),
        'cumulativeTotalTokens',
        GREATEST(
            COALESCE((usage_events.notification #>> '{event,payload,tokenUsage,total,totalTokens}')::bigint, 0),
            0
        ),
        'modelContextWindow',
        usage_events.notification #> '{event,payload,tokenUsage,modelContextWindow}',
        'effectiveContextWindow',
        usage_events.notification #> '{event,payload,tokenUsage,modelContextWindow}',
        'reserveTokens',
        0,
        'source',
        'provider'
    ),
    true
)::text
FROM usage_events
WHERE target.session_id = usage_events.session_id
  AND target.event_seq = usage_events.event_seq;
