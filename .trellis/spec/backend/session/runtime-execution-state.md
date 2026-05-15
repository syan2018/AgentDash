# Session 运行态执行状态

## 核心规则

Session 启动后的"当前实际在跑什么"归属 `SessionRuntime.active_execution`。
`PreparedLaunchPrompt` 是入口装配产物，`ExecutionContext` 是连接器投影，
二者都不是运行态真相容器。

2026-05 重构后，运行态事实按以下边界拆分：

- `SessionRuntimeRegistry` 是进程内 `SessionRuntime` map 的唯一访问入口。
- `TurnSupervisor` 负责 turn claim / activate / cancel / cleanup / stalled scan。
- `has_live_executor_session(session_id)` 只表示 connector 层是否有 live executor
  session；不得再用 "live runtime" 混称 active turn 或 registry entry。
- `has_runtime_entry(session_id)` 表示 hub 进程内是否有 runtime entry。
- `has_active_turn(session_id)` 表示当前是否存在 active turn。

## 内嵌 Connector

PiAgent 等 in-process connector 不处理原始 `McpServer` 声明，也不区分
direct / relay MCP。Application 层负责把 runtime tools、direct MCP tools、
relay MCP tools 统一构建成 `assembled_tools: Vec<DynAgentTool>`，connector
只接收并调用这些工具。

`ExecutionContext.session.mcp_servers` 只作为 connector-facing 的完整运行输入存在；
内嵌 connector 不消费该字段，不能在 agent 模块内重新做 MCP 发现或建联。

## Relay Connector

Relay connector 是远端执行器的 transport bridge。对于 relay 的本地/第三方
agent，cloud 侧直接把完整 `mcp_servers` 结构随 prompt payload 透传给远端，
不区分 direct / relay，也不加额外标注。

这些 MCP 连接由远端第三方 agent 自己处理，跟云端内嵌 agent 的
`assembled_tools` 设计无关。`RelayAgentConnector` 只能做原样透传，不能维护
私有 per-session MCP 缓存，也不能自创第二套 relay MCP 分类状态。

## 热更新

Workflow phase / lifecycle hot update 必须从 `SessionRuntime.active_execution`
读取当前 `CapabilityState.mcp_servers`，重建完整工具集后通过 live connector 替换。
`CompositeConnector` 必须把 `update_session_tools` 转发给持有 live session 的
子 connector，不能走 trait 默认 no-op。

## 内部 Follow-up

Hub 内部构造的 follow-up prompt（例如 hook auto-resume、companion parent
resume）必须经过 `PromptRequestAugmenter` 或等价的 assembler/envelope 路径，
以补齐 owner、VFS、MCP、CapabilityState、context bundle 等运行时字段。
禁止在特化路径中手写半裸 `PreparedLaunchPrompt` 并手工拷贝部分状态。

## Terminal Effects

`turn_terminal` event 必须先持久化，并由 `SessionMeta.last_execution_status` 投影
记录终态。终态后的业务副作用不允许由 `SessionTurnProcessor` 直接分发；必须先写入
terminal effect outbox，再由 dispatcher 执行并标记 `pending` / `running` /
`succeeded` / `failed`。

当前 outbox effect 类型：

- `hook_effects`：`SessionTerminal` hook 产出的 `HookEffect` 列表。
- `session_terminal_callback`：平台级 terminal callback。
- `hook_auto_resume`：hook `BeforeStop == continue` 驱动的 auto-resume。

effect 失败不能回滚 terminal event，也不能破坏 active turn cleanup。

## Persistence Store Boundaries

`SessionPersistence` 仍可作为迁移期组合接口存在，但能力边界必须拆开：

- `SessionMetaStore`：session meta CRUD 与投影字段合并写回。
- `SessionEventStore`：append/read/list session events。
- `SessionTerminalEffectStore`：terminal effect outbox 写入、状态迁移和查询。
- `SessionRuntimeCommandStore`：pending runtime command upsert / pending 查询 /
  applied / failed 状态迁移。

新增 runtime/effect/pending 调用点必须依赖对应 store 边界，不得继续把
`SessionPersistence` 当作无差别大仓储扩展。

## Pending Runtime Commands

pending runtime context / capability transition 不再存放在
`SessionMeta.pending_capability_state_transitions`。目标态事实源是 runtime command
store，按 `pending` / `applied` / `failed` 状态审计。

下轮 prompt 只从 command store 查询 pending commands，应用后标记为 `applied`。
`SessionMeta` 不再承担 command queue 职责；repository 主线不得继续读写 legacy
`pending_capability_state_transitions_json` 字段。
