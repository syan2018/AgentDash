-- A LifecycleAgent owns its immutable AgentFrame history and its stable concrete-Agent
-- association. Keeping those local facts in the owner row makes deletion and recovery follow the
-- Product aggregate boundary instead of a graph of global projection tables.

ALTER TABLE lifecycle_agents
    ADD COLUMN frames JSONB NOT NULL DEFAULT '[]'::JSONB
        CHECK (jsonb_typeof(frames) = 'array'),
    ADD COLUMN runtime_binding JSONB
        CHECK (runtime_binding IS NULL OR jsonb_typeof(runtime_binding) = 'object');

UPDATE lifecycle_agents AS agent
SET frames = materialized.frames
FROM (
    SELECT
        frame.agent_id,
        jsonb_agg(
            jsonb_strip_nulls(
                jsonb_build_object(
                    'id', frame.id,
                    'agent_id', frame.agent_id,
                    'revision', frame.revision,
                    'surface', COALESCE(frame.surface, '{}'::JSONB),
                    'created_by_kind', frame.created_by_kind,
                    'created_by_id', frame.created_by_id,
                    'created_at', frame.created_at
                )
            )
            ORDER BY frame.revision, frame.created_at
        ) AS frames
    FROM agent_frames AS frame
    GROUP BY frame.agent_id
) AS materialized
WHERE agent.id = materialized.agent_id;

UPDATE lifecycle_agents AS agent
SET runtime_binding = binding.binding
FROM agent_run_product_runtime_binding AS binding
WHERE agent.id = binding.target_agent_id
  AND agent.run_id = binding.target_run_id;

CREATE UNIQUE INDEX lifecycle_agents_runtime_thread_id_unique
    ON lifecycle_agents ((runtime_binding ->> 'runtime_thread_id'))
    WHERE runtime_binding IS NOT NULL;

DROP TABLE agent_frame_transitions CASCADE;
DROP TABLE agent_frames CASCADE;
DROP TABLE agent_run_product_runtime_binding;
