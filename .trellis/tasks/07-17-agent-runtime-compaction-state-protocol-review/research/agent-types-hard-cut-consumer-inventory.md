# agent-types hard cut consumer inventory

## 结论

`agentdash-agent-types` 不能通过改名、re-export 或同构序列化桥接继续存在。当前直接
consumer 分为两类：

- W7 必须先切走的 product caller：这些模块仍在把 Product/Application 工具装配成
  Native `AgentTool`，或从 presentation journal 重建 Core transcript。最终 caller
  应只消费 Runtime Contract、Business Surface、Tool Broker 与 Runtime snapshot/change。
- W8 必须删除或迁空的 legacy boundary：这些 crate/module 仍承载旧 connector、
  RuntimeSession、Backbone-to-Core 或 Agent SPI 职责。

真正执行 Core loop 的 Native、Infrastructure 与 LLM Provider 已直接依赖
`agentdash-agent-core`，不在剩余 consumer 中。

## Consumer matrix

| Current consumer | Current symbols / call path | Class | Final owner and hard-cut action |
| --- | --- | --- | --- |
| `agentdash-agent-protocol` | `ContentPart`, `AgentMessage`, `MessageRef`, `ProjectedTranscript`, `ToolCallInfo`；`user_input_blocks_to_content_parts` 与 `project_transcript` | W8 legacy boundary | Backbone/Codex wire vocabulary 保留在 owned protocol owner；Core input/transcript anti-corruption projection 迁入 Native adapter，Infrastructure 复用 Native-owned projector；随后删除 legacy protocol crate |
| `agentdash-api` | `AgentTool`, `AgentToolResult`, `ContentPart`, `ToolUpdateCallback`；`bootstrap::agent_runtime_surface` 把 product tools 包装为 Runtime Tool Broker callback | W7 product caller | production composition 只装配 Business Surface、Runtime Tool Broker 与 `AgentHostCallbacks`；删除 API 对 Core tool object 的构造与检查 |
| `agentdash-application` | wait/activity、companion、task、VFS 等 `AgentTool` 实现与 `RuntimeToolProvider` catalog | W7 product caller | Product 层只产出 typed tool contribution/requirement；执行归 Runtime Tool Broker，Core tool callback 只在 Native adapter materialize |
| `agentdash-application-agentrun` | `context_sources` 读取 `AgentTool::definition/protocol_projector`；`context_projection` 从 journal 调 `project_transcript` | W7 product caller | Surface compiler 读取平台 tool contribution；context/read 读取 Runtime snapshot/context contract，不从 presentation journal 重建 Core transcript |
| `agentdash-application-lifecycle` | workflow advance-node `AgentTool` 与 `RuntimeToolProvider` | W7 product caller | workflow 只声明平台 tool contribution；执行和 presentation 由 Tool Broker/Runtime change owner |
| `agentdash-application-ports` | `McpToolDiscovery` 返回 `DynAgentTool`；已无调用者的 `RuntimeSessionMailboxRuntimePort` 返回 Core turn delegate | W7 product caller + W8 cleanup | MCP discovery 返回平台 descriptor/call route，不返回 Core object；删除 legacy RuntimeSession live port |
| `agentdash-application-runtime-gateway` | `tool_adapter`, `session_actions`, `mcp_access` 接受/返回 Core tool types | W8 legacy boundary | extension gateway 只保留 extension/runtime action 职责；session/tool/MCP execution 改由 Runtime command + Tool Broker owner 后删除旧模块 |
| `agentdash-application-vfs` | filesystem/mount tools 实现 Core `AgentTool` 与 `ToolProtocolProjector` | W7 product caller | VFS 产出平台 tool contribution并由 Tool Broker调用；不实现 Core tool trait |
| `agentdash-spi` | 整包 re-export agent types；`connector` 的 `ExecutionContext`, `RuntimeToolProvider`, assembled tools 与 delegates | W8 legacy boundary | 删除 Agent re-export/facade；平台 SPI 只保留非 Agent ports并迁名，旧 connector/tool assembly 在 W7 caller 切换后删除 |

## Completed component cleanup

Activation component `e1abec31` 已完成两项无争议清理：

- `agentdash-contracts` 删除无调用者的
  `From<MessageRef> for SessionMessageRefDto` 与 direct dependency；
- `agentdash-executor` 删除无源码使用的 direct dependency，其经旧 SPI/MCP path 的
  connector 职责仍按 W8 legacy deletion 处理。

因此 metadata 中当前 direct consumers 与上表一致，恰为 9 个。

## Atomic activation dependency

以下修改必须作为同一 S5 cutover unit 合并，不能先把 Product/Application 改为直接依赖
Core：

1. W7 将 API/Application/AgentRun/Lifecycle/VFS 的旧 `RuntimeToolProvider` caller
   切到 Business Surface + Runtime Tool Broker + Host callback；
2. W7 将 context projection/read 切到 Runtime Contract snapshot/change；
3. W8 删除 application-ports MCP Core object、runtime-gateway legacy tool/session
   modules，以及 SPI Agent re-export/connector assembly；
4. Dash/Native owner 将 Core anti-corruption projection 收口到 Native adapter并删除所有
   `serde_json` 同构 transcode；
5. consumer 归零后删除 `agentdash-agent-types` workspace member、目录与 lockfile entry。

`agentdash-agent-service-api::AgentToolResult` 表达 Host callback 的 applied/rejected
结果，不等同于 Core 的多内容 tool loop result，因此不能用同名替换规避上述 caller
cutover。
