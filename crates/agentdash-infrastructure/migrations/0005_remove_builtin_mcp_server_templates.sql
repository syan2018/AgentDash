-- 当前内置 Shared Library manifest 不再声明 MCP Server Template。
-- 清理历史 builtin 行，避免 repository 在启动校验时读取已退出 manifest 的旧 payload shape。
DELETE FROM library_assets
WHERE scope = 'builtin'
  AND source = 'builtin'
  AND asset_type = 'mcp_server_template';
