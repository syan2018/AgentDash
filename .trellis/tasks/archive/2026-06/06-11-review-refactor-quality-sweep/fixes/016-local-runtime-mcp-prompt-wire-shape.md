# FIX-016: local-runtime MCP prompt wire shape 对齐

## 模块

`local-runtime`

## 来源

- `research/local-runtime-followup-executable-plan.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/backend/domain-payload-typing.md`

## 更新

- 在 application relay connector 侧新增 `session_mcp_server_to_relay_prompt_value()`，把内部 `SessionMcpServer` 投影为本机 parser 当前接受的扁平 relay prompt JSON。
- `RelayAgentConnector::prompt()` 不再直接序列化内部 `SessionMcpServer`，避免向 local runtime 发送嵌套 `transport` 形态。
- 保留 relay protocol 的 `Vec<Value>` 公共形态，不在本批推进 full typed MCP prompt contract。
- 保留 local `parse_relay_mcp_servers()` fail-closed 行为，并补充 application helper 生成形态可被本机 parser 解析的测试。

## 涉及文件

- `crates/agentdash-application/src/relay_connector.rs`
- `crates/agentdash-local/src/handlers/relay_mcp_servers.rs`

## 验证

- `cargo test -p agentdash-application relay_prompt_payload_passes_full_mcp_and_projects_working_dir`：1 passed。
- `cargo test -p agentdash-local relay_mcp_servers`：10 passed。
- `cargo check -p agentdash-application`：通过。
- `cargo check -p agentdash-local`：通过；仅剩既有 `agentdash-executor` unused import warning。

## Commit

未提交；本轮按要求只完成代码与记录更新。
