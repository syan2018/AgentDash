-- Add workspace-level title fields to lifecycle_agents.
-- Title is now a first-class AgentRun workspace property,
-- no longer read-through from SessionMeta.

ALTER TABLE lifecycle_agents
  ADD COLUMN workspace_title TEXT DEFAULT NULL,
  ADD COLUMN workspace_title_source TEXT DEFAULT NULL;
