-- Dash terminal failure history now preserves the concrete Agent error code, message, and
-- retryability as one typed owner fact. Existing development histories only retained a generic
-- string, so their missing evidence cannot be reconstructed without inventing Agent facts.
--
-- The project is not deployed. Reset the Dash-owned source/effect documents and let Product
-- associations observe the source as unavailable until a new AgentRun/source is created.
-- Product lifecycle, workflow, AgentFrame, and lineage documents remain untouched.

DELETE FROM dash_complete_effect;
DELETE FROM dash_agent_session;
