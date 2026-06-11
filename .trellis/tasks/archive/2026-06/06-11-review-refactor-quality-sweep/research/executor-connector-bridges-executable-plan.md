# Executor Connector Bridges Executable Plan

## 模块边界

本轮只读扫描 `agentdash-executor` 的 MCP direct/relay、Pi Agent connector/provider bridge 和相关 SPI/runtime types。没有发现必须列为严格架构项的跨 10+ 文件事实源重定问题；主要是 executor 模块内重复和命名归位。

## 证据

- `McpToolAdapter::from_tool` 与 `RelayMcpToolAdapter::from_info` 重复默认描述、`sanitize_tool_schema`、JSON object 参数校验、`AgentToolResult` 投影、capability filter。
- `capability_key_for_mcp_server_name`、`namespaced_tool_name`、`agent_facing_mcp_server_name` 放在 `mcp/direct.rs` 下，但职责是 runtime tool naming 与 platform MCP capability mapping，relay 也复用，命名边界偏弱。
- `pi_agent/stream_mapper.rs` 硬编码 `fs_read` / `fs_grep` / `fs_glob` native item 投影，同一 native item 语义在 `agentdash-agent-types/src/protocol.rs` 维护。
- OpenAI Responses、OpenAI Chat Completions、Anthropic bridge 分别转换消息和工具流；`openai_content.rs` 已抽出部分 OpenAI helper，但 Anthropic 仍独立处理 `ContentPart`、tool result、reasoning/tool delta state。
- 静态 bridge 回退会生成 `static-default`；provider model catalog 会合成 default fallback model；OpenAI-compatible `wire_api` 按 base_url 隐式默认 responses/completions。预研期可考虑显式化配置，降低隐式分叉。

## 可执行批次

### Batch A: MCP adapter 共用核心

- 写入：`crates/agentdash-executor/src/mcp/direct.rs`、`relay.rs`，新增或扩展 `mcp/common.rs`。
- 内容：抽出 `normalize_description`、`normalize_args_object`、`McpToolSurface`、`build_discovered_entry`；direct/relay 只保留 transport 调用差异。
- 风险：低；重点保持行为。
- 验证：`cargo test -p agentdash-executor mcp::direct::tests mcp::relay::tests`。

### Batch B: MCP 命名与 capability mapper 归位

- 写入：`crates/agentdash-executor/src/mcp/direct.rs`、`relay.rs`、`mod.rs`，可新增 `mcp/naming.rs`。
- 内容：把 `namespaced_tool_name` / `capability_key_for_mcp_server_name` / `agent_facing_mcp_server_name` 从 `direct` 移到 runtime surface/naming 模块。
- 风险：低到中；导入路径调整影响测试。
- 验证：`cargo test -p agentdash-executor mcp::`。

### Batch C: Pi Agent tool item 投影收敛

- 写入：`stream_mapper.rs`、`agentdash-agent-types/src/protocol.rs`，必要时 `agentdash-agent-protocol` thread item。
- 内容：把 `fs_read/fs_grep/fs_glob` 参数提取和 native item 构造移到 agent-types/protocol helper；stream mapper 只按 tool name 调 helper。
- 风险：中；涉及事件 projection。
- 验证：`cargo test -p agentdash-executor connectors::pi_agent::connector_tests`；`cargo check -p agentdash-agent-types -p agentdash-executor`。

### Batch D: Provider bridge 内容转换分层

- 写入：`openai_content.rs`、`openai_completions_bridge.rs`、`openai_responses_common.rs`、`anthropic_bridge.rs`，可新增 `content_codec.rs`。
- 内容：把 `ContentPart`、tool result、compaction summary、tool schema 的公共中间表达抽出；各 provider 只负责最终 wire shape 和 SSE state machine。
- 风险：中；模型 API payload 容错面较大。
- 验证：`cargo test -p agentdash-executor connectors::pi_agent::bridges`。

## 架构项

无。MCP relay wire DTO 与 SPI DTO 有相似形态，但当前有明确 relay 协议边界与 API mapper；本轮不升级为跨层协议重定。
