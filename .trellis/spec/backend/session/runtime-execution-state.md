# Session 运行态执行状态

## 核心规则

Session 启动后的"当前实际在跑什么"归属 `SessionRuntime.active_execution`。
`PromptSessionRequest` 是入口装配产物，`ExecutionContext` 是连接器投影，
二者都不是运行态真相容器。

## 内嵌 Connector

PiAgent 等 in-process connector 不处理原始 `McpServer` 声明，也不区分
direct / relay MCP。Application 层负责把 runtime tools、direct MCP tools、
relay MCP tools 统一构建成 `assembled_tools: Vec<DynAgentTool>`，connector
只接收并调用这些工具。

`ExecutionContext.mcp_servers` 只作为 connector-facing 的完整运行输入存在；
内嵌 connector 不消费该字段，不能在 agent 模块内重新做 MCP 发现或建联。
`relay_mcp_server_names` 仍归属 `SessionRuntime.active_execution`，不进入
connector 投影。

## Relay Connector

Relay connector 是远端执行器的 transport bridge。对于 relay 的本地/第三方
agent，cloud 侧直接把完整 `mcp_servers` 结构随 prompt payload 透传给远端，
不区分 direct / relay，也不加额外标注。

这些 MCP 连接由远端第三方 agent 自己处理，跟云端内嵌 agent 的
`assembled_tools` 设计无关。`RelayAgentConnector` 只能做原样透传，不能维护
私有 per-session MCP 缓存，也不能根据 `relay_mcp_server_names` 做分类。

## 热更新

Workflow phase / lifecycle hot update 必须从 `SessionRuntime.active_execution`
读取当前 relay MCP 分类，重建完整工具集后通过 live connector 替换。
`CompositeConnector` 必须把 `update_session_tools` 转发给持有 live session 的
子 connector，不能走 trait 默认 no-op。

## 内部 Follow-up

Hub 内部构造的 follow-up prompt（例如 hook auto-resume、companion parent
resume）必须经过 `PromptRequestAugmenter` 或等价的 assembler/envelope 路径，
以补齐 owner、VFS、MCP、flow capabilities、context bundle 等运行时字段。
禁止在特化路径中手写半裸 `PromptSessionRequest` 并手工拷贝部分状态。
