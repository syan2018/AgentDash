WITH latest_frames AS (
    SELECT DISTINCT ON (agent_id)
        agent_id,
        id AS frame_id
    FROM agent_frames
    ORDER BY agent_id, revision DESC, created_at DESC, id DESC
)
UPDATE lifecycle_agents AS agent
SET current_frame_id = latest_frames.frame_id
FROM latest_frames
WHERE agent.id = latest_frames.agent_id
  AND agent.current_frame_id IS NULL;

WITH latest_anchors AS (
    SELECT DISTINCT ON (anchor.run_id, anchor.agent_id)
        anchor.run_id,
        anchor.agent_id,
        anchor.runtime_session_id,
        anchor.launch_frame_id,
        anchor.orchestration_id,
        anchor.node_path,
        anchor.node_attempt,
        anchor.updated_at,
        anchor.created_at,
        session.last_delivery_status
    FROM runtime_session_execution_anchors AS anchor
    JOIN sessions AS session
      ON session.id = anchor.runtime_session_id
    ORDER BY
        anchor.run_id,
        anchor.agent_id,
        anchor.updated_at DESC,
        anchor.created_at DESC,
        anchor.runtime_session_id DESC
)
UPDATE lifecycle_agents AS agent
SET
    current_delivery_runtime_session_id = latest_anchors.runtime_session_id,
    current_delivery_launch_frame_id = latest_anchors.launch_frame_id,
    current_delivery_orchestration_id = latest_anchors.orchestration_id,
    current_delivery_node_path = latest_anchors.node_path,
    current_delivery_node_attempt = latest_anchors.node_attempt,
    current_delivery_status = CASE latest_anchors.last_delivery_status
        WHEN 'running' THEN 'running'
        WHEN 'lost' THEN 'lost'
        WHEN 'idle' THEN 'ready'
        ELSE 'terminal'
    END,
    current_delivery_observed_at = COALESCE(latest_anchors.updated_at, latest_anchors.created_at, now())
FROM latest_anchors
WHERE agent.run_id = latest_anchors.run_id
  AND agent.id = latest_anchors.agent_id
  AND agent.current_delivery_runtime_session_id IS NULL;
