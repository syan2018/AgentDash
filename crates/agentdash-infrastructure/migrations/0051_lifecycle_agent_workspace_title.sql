-- Add workspace-level title fields to lifecycle_agents.
-- Title is now a first-class AgentRun workspace property,
-- no longer read-through from SessionMeta.

ALTER TABLE lifecycle_agents
  ADD COLUMN workspace_title TEXT DEFAULT NULL,
  ADD COLUMN workspace_title_source TEXT DEFAULT NULL;

-- Backfill from runtime_sessions via execution anchors (most recent session per agent wins).
UPDATE lifecycle_agents la
SET workspace_title = sub.title,
    workspace_title_source = sub.title_source
FROM (
    SELECT DISTINCT ON (rsea.agent_id)
           rsea.agent_id,
           rs.title,
           rs.title_source
    FROM runtime_session_execution_anchors rsea
    JOIN runtime_sessions rs ON rs.id = rsea.runtime_session_id
    WHERE rs.title IS NOT NULL AND rs.title != ''
    ORDER BY rsea.agent_id, rsea.created_at DESC
) sub
WHERE la.id = sub.agent_id
  AND la.workspace_title IS NULL;

-- Drop old title columns from runtime_sessions.
ALTER TABLE runtime_sessions
  DROP COLUMN title,
  DROP COLUMN title_source;
