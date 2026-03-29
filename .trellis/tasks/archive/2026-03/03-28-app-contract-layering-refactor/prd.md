# Application / Connector-Contract 分层重构

## Goal

彻底解决后端架构中 Application 层、Connector-Contract 层、Executor 层和 API 层之间的分层不清晰和过度耦合问题。
通过一次性重构（单 PR），实现干净的六边形架构分层，消除重复类型和手动映射。

## 背景

架构审查发现以下核心问题：
1. connector-contract 被具体协议污染（依赖 vibe-kanban executors 和 ACP SDK）
2. 3 套重复的 ExecutorConfig 类型（contract / application / relay）
3. API 层承载了大量 Application 层逻辑（session_plan、execution_hooks、address_space_access 等）
4. HookSessionRuntime（有状态运行时）放在无状态合同层
5. Application 层对执行器/协议层横向依赖
6. ExecutorHub 本质是 session 编排，应属于 Application 层而非独立的 Executor 层
7. Agent 生命周期 SPI 锁在 Pi Agent 专有 crate 中，其他层难以复用
8. TaskExecutionGateway 设计过度 — 实际是对 AppState 字段的透传封装

---

## 设计决策

### D1: ACP 协议绑定策略 → 保持 ACP + Adapter

- connector-contract **继续使用 ACP `SessionNotification`** 作为 ExecutionStream 的事件类型
- **移除 `executors` (vibe-kanban) 依赖** — `to_vibe_kanban_config()` 迁出
- 非 ACP 连接器通过 adapter 层转换为 ACP 事件

### D2: ExecutorConfig 统一 → Domain 层

- 在 `agentdash-domain` 中定义唯一的 `ExecutorConfig` 类型
- 删除 application/runtime.rs 中的重复定义
- 删除 runtime_bridge.rs 中的双向映射函数
- relay 保留 `ExecutorConfigRelay` 作为跨进程序列化 DTO，提供 From 转换

### D3: API 层瘦身 → 全部迁入 Application 层

以下模块从 `agentdash-api` 迁入 `agentdash-application`：
- `session_plan.rs` — 会话计划构建
- `session_context.rs` — 会话上下文
- `execution_hooks/` — AppExecutionHookProvider
- `address_space_access/` — RelayAddressSpaceService, RuntimeToolProvider 等
- `task_agent_context.rs` — ContextContributorRegistry
- `workspace_resolution.rs` — 工作空间解析
- `runtime_bridge.rs` — 剩余必要转换

API 层保留：
- routes/（路由 + handler）
- dto/（请求/响应 DTO）
- app_state.rs（DI 组装）
- auth.rs, stream.rs, rpc.rs
- relay/（WebSocket handler）
- plugins.rs, bootstrap/

### D4: ExecutorHub 迁入 Application 层

- `ExecutorHub` 从 executor 迁入 application（session 生命周期编排是应用层逻辑）
- `HookSessionRuntime` 随 Hub 一起迁入 application
- `HookRuntimeDelegate` 随 Hub 一起迁入 application（或移入 connectors 层，因为它是 Pi Agent 的 delegate 适配器）
- 原 `agentdash-executor` crate 精简为纯 Connector 实现层（可考虑改名为 `agentdash-connectors`）

### D5: 删除 TaskExecutionGateway，改为直接 Service

- 删除 `TaskExecutionGateway<C>` trait 及其 API 层实现 `AppStateTaskExecutionGateway`
- `start_task` / `continue_task` / `cancel_task` 变为 Application 层 Service 的直接方法
- Application 层 Service 持有 RepositorySet + SessionHub + WorkspaceResolver，直接编排

### D6: Agent 生命周期 SPI 抽取到 Connector-Contract

从 `agentdash-agent` 抽取通用生命周期类型到 `agentdash-connector-contract`：
- `AgentRuntimeDelegate` trait
- `AgentMessage` (User / Assistant / ToolCall / ToolResult)
- `AgentContext` (system_prompt + messages + tools)
- `StopDecision`, `ToolCallDecision`
- `TransformContextInput/Output`, `BeforeToolCallInput`, `AfterToolCallInput`, `AfterTurnInput`, `BeforeStopInput`

agentdash-agent 保留：
- `agent_loop.rs`（Rig SDK agent loop 实现）
- `tools/`（工具注册、schema、builtins）
- `event_stream.rs`（事件流实现）

### D7: 执行策略 → 一次性 PR

单 PR 完成所有变更，保证架构一致性。

---

## 预期目标拓扑

```
agentdash-domain
  ├── 实体：Project, Story, Task, Agent, Workspace, SessionBinding, ...
  ├── Repository traits (Ports)
  ├── ExecutorConfig（唯一定义）
  └── 值对象：MountCapability, AddressSpace, Mount, ...

agentdash-connector-contract（Agent 执行统一 SPI，零实现）
  ├── connector.rs  — AgentConnector trait, ExecutionContext, ExecutionStream
  ├── tool.rs       — AgentTool, AgentToolResult
  ├── hooks.rs      — ExecutionHookProvider trait + Hook DTOs（仅 trait + DTO，无运行时状态）
  ├── lifecycle.rs  — AgentRuntimeDelegate, AgentMessage, StopDecision, ToolCallDecision
  └── 依赖：domain + agent-client-protocol + serde（不再有 executors/vibe-kanban）

agentdash-agent（Pi Agent SDK，独立可用）
  ├── agent_loop.rs（基于 Rig 的 Agent 循环实现）
  ├── tools/（工具注册 + schema + builtins）
  ├── event_stream.rs
  └── 依赖：connector-contract + rig-core（不直接依赖 domain 业务实体）

agentdash-application（核心应用层：编排 + 服务）
  ├── session/
  │   ├── hub.rs（原 ExecutorHub：session 管理 + 事件流）
  │   ├── hook_runtime.rs（HookSessionRuntime：per-session Hook 运行时状态）
  │   └── hook_delegate.rs（HookRuntimeDelegate：lifecycle 委托桥接）
  ├── task/
  │   ├── execution_service.rs（原 start_task/continue_task/cancel_task，直接编排）
  │   ├── config.rs, artifact.rs, meta.rs, ...
  │   └── state_reconciler.rs, restart_tracker.rs, lock.rs
  ├── context/（上下文构建）
  ├── hooks/（AppExecutionHookProvider，从 API 层迁入）
  ├── session_plan.rs（从 API 层迁入）
  ├── session_context.rs（从 API 层迁入）
  ├── address_space/（含从 API 层迁入的 relay_service, runtime_provider 等）
  ├── workspace/（含从 API 层迁入的 workspace_resolution）
  ├── task_agent_context.rs（从 API 层迁入）
  ├── workflow/, project/, story/
  ├── repository_set.rs
  └── 依赖：domain + connector-contract（不直接依赖 connectors/agent）

agentdash-executor（精简为纯 Connector 实现层）
  ├── connectors/
  │   ├── pi_agent.rs（PiAgentConnector：依赖 agentdash-agent + rig-core）
  │   ├── vibe_kanban.rs（VibeKanbanConnector：依赖 executors crate）
  │   ├── remote_acp.rs（RemoteACPConnector）
  │   └── composite.rs（CompositeConnector：路由分发）
  ├── adapters/
  │   ├── normalized_to_acp.rs（vibe-kanban 事件 → ACP SessionNotification）
  │   └── vibe_kanban_config.rs（to_vibe_kanban_config() 从 contract 迁入此处）
  └── 依赖：connector-contract + agent + executors(vibe-kanban) + agent-client-protocol

agentdash-api（纯 HTTP 入口 + DI 组装）
  ├── routes/（路由 handler，委托给 application 层服务）
  ├── dto/（请求/响应 DTO 转换）
  ├── app_state.rs（DI 组装，注入 connector 实例到 application 层）
  ├── auth.rs, stream.rs, rpc.rs
  ├── relay/（WebSocket handler）
  └── plugins.rs, bootstrap/
```

---

## Requirements

### R1: Connector-Contract 依赖精简
- [ ] 移除 `executors` (vibe-kanban) 依赖
- [ ] 移除 `json-patch` 依赖
- [ ] `AgentDashExecutorConfig` 迁入 domain 并统一为 `ExecutorConfig`
- [ ] `to_vibe_kanban_config()` 迁入 executor 层的 adapter 模块
- [ ] contract 层最终依赖：domain + agent-client-protocol + serde 系 + async-trait

### R2: Agent 生命周期 SPI 抽取
- [ ] `AgentRuntimeDelegate` trait 从 agentdash-agent 迁入 connector-contract
- [ ] `AgentMessage`, `AgentContext`, `StopDecision`, `ToolCallDecision` 迁入 connector-contract
- [ ] `TransformContextInput/Output`, `BeforeToolCallInput`, `AfterToolCallInput` 等迁入
- [ ] agentdash-agent 改为引用 connector-contract 中的这些类型（使用 re-export 保持兼容性）

### R3: ExecutorConfig 类型统一
- [ ] domain 层定义唯一的 `ExecutorConfig`
- [ ] 删除 application/runtime.rs 中的重复定义
- [ ] 删除 runtime_bridge.rs 中的双向映射函数
- [ ] 所有内部引用统一为 `agentdash_domain::ExecutorConfig`
- [ ] relay 保留 `ExecutorConfigRelay` 作为序列化 DTO

### R4: ExecutorHub 迁入 Application 层
- [ ] `hub.rs` 迁入 application/session/
- [ ] `HookSessionRuntime` 从 connector-contract 迁入 application/session/
- [ ] `HookRuntimeDelegate` 迁入 application/session/ 或 executor/connectors/pi_agent/
- [ ] `hook_events.rs` 迁入 application/session/
- [ ] 原 executor crate 精简，只保留 connectors/ 和 adapters/

### R5: 删除 TaskExecutionGateway
- [ ] 删除 `TaskExecutionGateway<C>` trait
- [ ] 删除 `AppStateTaskExecutionGateway`
- [ ] `start_task` / `continue_task` / `cancel_task` 变为 application 层 Service 方法
- [ ] 原 gateway 中的 repo 操作改为直接使用 RepositorySet
- [ ] 原 gateway 中的 Hub 操作改为直接使用 SessionHub

### R6: API 层瘦身
- [ ] session_plan 迁入 application
- [ ] session_context 迁入 application
- [ ] execution_hooks (AppExecutionHookProvider) 迁入 application
- [ ] address_space_access 迁入 application
- [ ] task_agent_context + ContextContributorRegistry 迁入 application
- [ ] workspace_resolution 迁入 application
- [ ] runtime_bridge 剩余必要转换迁入 application
- [ ] API 层路由 handler 改为调用 application 层服务

### R7: Application 层依赖清理
- [ ] 不再依赖 `agentdash-executor`（Hub 已迁入）
- [ ] 不直接依赖 `agentdash-agent`（通过 connector-contract 的 lifecycle SPI）
- [ ] 评估是否可以移除对 `agentdash-mcp` 和 `agentdash-relay` 的直接依赖
- [ ] 最终依赖目标：domain + connector-contract + agent-client-protocol + 标准库

---

## Acceptance Criteria

- [ ] `cargo check --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] `cargo test --workspace` 全部通过
- [ ] connector-contract 不依赖 `executors`（vibe-kanban）
- [ ] connector-contract 包含完整的 Agent 生命周期 SPI（lifecycle.rs）
- [ ] agentdash-api/src/ 中不再包含 session_plan、execution_hooks、address_space_access 等业务模块
- [ ] application 层不依赖 agentdash-executor 或 agentdash-agent
- [ ] ExecutorHub 位于 application 层
- [ ] 无 TaskExecutionGateway trait
- [ ] 无重复 ExecutorConfig 定义，无逐字段拷贝映射函数
- [ ] 前端功能不受影响（API 路径和响应格式不变）

---

## 内部实施阶段（单 PR 内的工作顺序）

### Phase A: 基础类型统一
1. ExecutorConfig 迁入 domain
2. Agent 生命周期类型从 agentdash-agent 抽取到 connector-contract

### Phase B: Contract 层精简
1. 移除 `executors` 依赖
2. `to_vibe_kanban_config()` 迁入 executor adapter
3. `HookSessionRuntime` 实现迁出 connector-contract（留 trait + DTO）

### Phase C: ExecutorHub 迁移
1. Hub 从 executor 迁入 application/session/
2. HookSessionRuntime 迁入 application/session/
3. HookRuntimeDelegate 归位
4. Executor crate 精简为纯 connectors 层

### Phase D: API → Application 迁移
1. 逐模块迁移（session_plan, execution_hooks, address_space_access, ...）
2. 删除 TaskExecutionGateway，建立 Service 直接编排
3. API 层 handler 改为调用 application 层

### Phase E: 依赖清理 + 验证
1. 清理 application 层的 Cargo.toml
2. 确保编译 + clippy + 测试全过
3. 确认 API 路径和响应格式无变化

---

## 风险

- **改动量大**：涉及 ~10 个 crate 的接口变更，需仔细验证
- **Hub 体量**：ExecutorHub 2146 行迁入 application 后需要内部模块化拆分
- **Pi Agent 生命周期类型迁移**：需确保 agentdash-agent 的外部接口保持兼容（re-export）
- **Application 对 relay/mcp 的残余依赖**：部分转换逻辑可能暂时无法完全消除

## 不在范围内

- Domain 层 StreamEvent 和 ChangeLog 的归属问题（留作后续独立 PR）
- 前端任何修改
- Relay 协议格式变更
- Executor crate 改名（可后续讨论）
