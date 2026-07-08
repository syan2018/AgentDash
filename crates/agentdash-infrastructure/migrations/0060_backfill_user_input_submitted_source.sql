UPDATE runtime_session_events
SET notification_json = jsonb_set(
    notification_json,
    '{event,payload,source}',
    '{
        "namespace": "core",
        "kind": "composer",
        "actor": "user",
        "displayLabelKey": "mailbox.source.core.composer"
    }'::jsonb,
    true
)
WHERE notification_json #>> '{event,type}' = 'user_input_submitted'
  AND notification_json #> '{event,payload,source}' IS NULL;
