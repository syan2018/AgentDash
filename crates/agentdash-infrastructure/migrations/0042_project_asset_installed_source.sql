ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS library_asset_id TEXT;
ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS source_ref TEXT;
ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS source_version TEXT;
ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS source_digest TEXT;
ALTER TABLE mcp_presets
ADD COLUMN IF NOT EXISTS installed_at TEXT;

ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS library_asset_id TEXT;
ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS source_ref TEXT;
ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS source_version TEXT;
ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS source_digest TEXT;
ALTER TABLE skill_assets
ADD COLUMN IF NOT EXISTS installed_at TEXT;

ALTER TABLE agent_procedures
ADD COLUMN IF NOT EXISTS library_asset_id TEXT;
ALTER TABLE agent_procedures
ADD COLUMN IF NOT EXISTS source_ref TEXT;
ALTER TABLE agent_procedures
ADD COLUMN IF NOT EXISTS source_version TEXT;
ALTER TABLE agent_procedures
ADD COLUMN IF NOT EXISTS source_digest TEXT;
ALTER TABLE agent_procedures
ADD COLUMN IF NOT EXISTS installed_at TEXT;

ALTER TABLE workflow_graphs
ADD COLUMN IF NOT EXISTS library_asset_id TEXT;
ALTER TABLE workflow_graphs
ADD COLUMN IF NOT EXISTS source_ref TEXT;
ALTER TABLE workflow_graphs
ADD COLUMN IF NOT EXISTS source_version TEXT;
ALTER TABLE workflow_graphs
ADD COLUMN IF NOT EXISTS source_digest TEXT;
ALTER TABLE workflow_graphs
ADD COLUMN IF NOT EXISTS installed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_mcp_presets_library_asset_id
    ON mcp_presets(library_asset_id);
CREATE INDEX IF NOT EXISTS idx_skill_assets_library_asset_id
    ON skill_assets(library_asset_id);
CREATE INDEX IF NOT EXISTS idx_agent_procedures_library_asset_id
    ON agent_procedures(library_asset_id);
CREATE INDEX IF NOT EXISTS idx_workflow_graphs_library_asset_id
    ON workflow_graphs(library_asset_id);
