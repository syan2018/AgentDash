-- MCP Preset——Project 级单个 MCP Server 配置模板。
-- 每个 Preset = 一个 MCP server 声明（http / sse / stdio），供 Agent 组装复用。
-- 对齐 Workflow 的 builtin/user 二元来源模型：source='builtin' 时 builtin_key 必填，
-- source='user' 时 builtin_key 必须为 NULL。

CREATE TABLE IF NOT EXISTS mcp_presets (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    -- server_decl 以 JSON 文本存储，结构对齐 domain::mcp_preset::McpServerDecl
    -- （`{ type: "http"|"sse"|"stdio", name, ... }`）。
    server_decl TEXT NOT NULL,
    source TEXT NOT NULL,
    builtin_key TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT mcp_presets_source_check CHECK (source IN ('builtin', 'user')),
    CONSTRAINT mcp_presets_builtin_key_consistency CHECK (
        (source = 'builtin' AND builtin_key IS NOT NULL)
        OR (source = 'user' AND builtin_key IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_presets_project_name
    ON mcp_presets(project_id, name);

CREATE INDEX IF NOT EXISTS idx_mcp_presets_project_id
    ON mcp_presets(project_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_presets_project_builtin_key
    ON mcp_presets(project_id, builtin_key)
    WHERE builtin_key IS NOT NULL;
