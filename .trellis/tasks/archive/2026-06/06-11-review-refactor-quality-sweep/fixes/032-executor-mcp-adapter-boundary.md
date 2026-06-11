# Fix 032: executor MCP adapter boundary

## 模块

- executor-connector-bridges

## 问题

`executor-connector-bridges` narrow review 发现 MCP direct 与 relay adapter 重复维护描述归一、schema sanitize、参数 object 校验、discovered entry 投影；MCP runtime naming 与 capability mapper 也放在 `direct.rs` 下，职责归属偏窄。

## 更新

- 新增 `mcp/common.rs`，集中 `McpToolSurface`、`normalize_description`、`normalize_args_object` 和 `build_discovered_entry`。
- 新增 `mcp/naming.rs`，集中 `namespaced_tool_name`、`agent_facing_mcp_server_name`、`capability_key_for_mcp_server_name` 与命名测试。
- direct / relay adapter 复用 common 与 naming，只保留各自 transport 调用差异。
- `mcp/mod.rs` 继续 re-export `namespaced_tool_name`，外部调用路径不变。

## 涉及文件

- `crates/agentdash-executor/src/mcp/common.rs`
- `crates/agentdash-executor/src/mcp/naming.rs`
- `crates/agentdash-executor/src/mcp/direct.rs`
- `crates/agentdash-executor/src/mcp/relay.rs`
- `crates/agentdash-executor/src/mcp/mod.rs`

## 验证

- `cargo fmt -p agentdash-executor`
- `cargo test -p agentdash-executor mcp::`
- `git diff --check`

## Commit

- `bc949430`：`refactor(executor): 收敛 MCP adapter 共用边界`
