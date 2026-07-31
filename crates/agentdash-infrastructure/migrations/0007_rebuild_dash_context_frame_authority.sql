UPDATE lifecycle_agents AS agent
SET runtime_binding = NULL
WHERE agent.runtime_binding IS NOT NULL
  AND EXISTS (
      SELECT 1
      FROM dash_complete_source AS source
      WHERE source.source_coordinate = agent.runtime_binding #>> '{agent,source}'
  );

UPDATE agent_run_mailbox_messages AS message
SET delivery_source_coordinate = NULL,
    delivery_binding_generation = NULL
WHERE message.status IN (
        'accepted',
        'queued',
        'ready_to_consume',
        'consuming',
        'paused',
        'blocked'
    )
  AND EXISTS (
      SELECT 1
      FROM dash_complete_source AS source
      WHERE source.source_coordinate = message.delivery_source_coordinate
  );

DELETE FROM dash_complete_effect;
DELETE FROM dash_complete_source;
