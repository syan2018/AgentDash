-- A Product session is addressable only while its committed Runtime binding resolves to the
-- concrete Agent source that owns the conversation. Earlier pre-release Dash authority resets
-- removed those source documents while leaving their Product owners addressable, so every read
-- and live subscription for those owners crossed an impossible binding and failed.
--
-- The missing conversation documents contain the only authoritative replay history and cannot
-- be reconstructed from the remaining Product shell. Removing those shells restores the owner
-- invariant and keeps the session list limited to conversations that can actually be opened.

DELETE FROM lifecycle_runs AS run
WHERE EXISTS (
    SELECT 1
    FROM lifecycle_agents AS agent
    WHERE agent.run_id = run.id
      AND agent.runtime_binding IS NOT NULL
      AND agent.runtime_binding #>> '{agent,source}' LIKE 'dash:%'
      AND NOT EXISTS (
          SELECT 1
          FROM dash_complete_source AS source
          WHERE source.source_coordinate = agent.runtime_binding #>> '{agent,source}'
      )
);
