# FIX-006: local-runtime Relay MCP 配置 fail-closed

## 模块

`local-runtime`

## 来源

- `reviews/004-local-runtime.md`
- `research/local-runtime-executable-plan.md`
- worker: `019eb2b5-0d3f-7d71-ab5e-e3c810bd1df3`

## 更新

- `parse_relay_mcp_servers` 改为返回 `Result<Vec<SessionMcpServer>, RelayMcpServerParseError>`。
- 对非对象、缺少或非法 `name` / `type`、未知 transport、缺少 `url` / `command`、非法 `headers` / `env` / `args` 全部 fail-closed。
- `handle_prompt` 在 workspace prepare 和 `LaunchCommand` 组装前解析 MCP 配置，失败时返回 `INVALID_MESSAGE`。
- 保持 relay DTO 与 cloud/local wire contract 不变。

## 涉及文件

- `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`
- `crates/agentdash-local/src/handlers/prompt.rs`

## 验证

- `cargo test -p agentdash-local relay_mcp_servers`：8 passed。
- `cargo test -p agentdash-local prompt`：1 passed。
- `cargo check -p agentdash-local`：通过；仅剩既有 `agentdash-executor` unused import warning。
- 汇合验证：`cargo test -p agentdash-local`：86 passed。

## Commit

待提交。
