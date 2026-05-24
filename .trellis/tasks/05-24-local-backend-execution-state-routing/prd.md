# 本机后端执行状态与分配治理

## Goal

让云端能够可靠管理本机后端的执行占用状态，并在启动 session turn 时支持明确指定后端或自动选择空闲后端。该能力为后续多本机并行、共享执行后端、按 Project/Workspace 选择运行位置打基础。

## 背景与用户价值

当前系统已经具备 local runtime ensure、relay 注册、runtime health、workspace inventory / binding、relay executor discovery 与 session prompt 路由。但这些状态分散在不同链路中：

- `runtime_health` 记录连接健康与能力快照。
- `backend_workspace_inventory` / `workspace_bindings` 记录本机目录事实。
- `BackendRegistry` 只保存当前 WebSocket 连接与 executor snapshot。
- relay session sink 只按 `session_id` 路由通知，不记录该 session 归属哪个 backend。
- session 启动时只能从 VFS mount 间接推断 backend，不能把“执行目标后端”作为显式启动输入。

用户希望后续可以在执行 session 时指定后端，也希望自动分配时优先选择空闲的可用后端。为此需要补齐执行状态事实源、分配策略、session/backend 归属与状态释放链路。

## Confirmed Facts

- 云端是业务数据与事件事实源，本机只管理本机进程、工具执行和物理文件访问。
- `runtime_health` 已有 PostgreSQL 表和 repository，状态值表达 backend 连接健康，不表达执行占用。
- `BackendRegistry::resolve_backend()` 当前只按 online + executor available 选择；多个候选时报错。
- `RelayAgentConnector` 当前通过 VFS default mount 或唯一 backend mount 推断 preferred backend。
- `UserPromptInput` 当前没有 backend selection 字段。
- relay cancel 当前不知道 session 归属 backend，只能遍历在线 backend 广播 cancel。
- workspace binding 解析当前允许回退到离线 default binding，适合作为目录事实解析，不适合作为空闲执行分配的最终事实。
- 项目处于预研期，不需要保留旧 API / 字段兼容，但需要通过 migration 表达数据库变更。

## Requirements

### R1: 执行占用事实源

新增云端权威的 backend execution state / lease 事实源，记录每个正在执行或刚结束的 remote turn 与 backend 的归属关系。该事实源必须至少覆盖：

- `backend_id`
- `session_id`
- `turn_id`
- `executor_id`
- `workspace_id` / `root_ref`（存在 workspace 时）
- selection mode：`explicit` / `auto_idle` / `workspace_binding`
- lease state：`claimed` / `running` / `released` / `lost` / `failed`
- claim、activate、release、last_seen 时间
- terminal kind / release reason

### R2: Backend 选择策略

新增 application 层 allocator，用同一个入口解析 backend 选择：

- 显式指定 backend：必须校验当前用户可见/可用、backend 在线、executor 可用、workspace 绑定约束成立。
- 自动分配空闲 backend：从在线、授权、executor 可用、满足 workspace 约束的候选中选择 active lease 数最少的 backend；第一版不引入 capacity / weight。
- workspace binding 模式：保留 workspace 目录解析职责，但最终执行 backend 必须经过 allocator 校验。

### R3: Session 启动输入与计划

session launch 链路必须能携带 backend selection intent，并在 `SessionConstructionPlan` / `LaunchPlan` / connector context 中表达最终选中的 backend。执行后端不能只从 VFS mount 间接推断。

### R4: Relay session/backend 归属

relay connector 与 `BackendRegistry` 必须记录 `session_id -> backend_id / lease_id` 的运行态归属，用于：

- 精确 cancel
- backend disconnect 时标记受影响 lease
- terminal 到达时释放对应 lease
- 后续 UI/API 查询运行中的 backend session

### R5: 状态释放闭环

以下路径都必须释放或标记 lease：

- prompt 成功进入 running
- terminal completed / failed / cancelled
- prompt 启动失败
- cancel 成功或失败
- backend disconnect
- stalled / recovery 扫描发现 orphan lease

### R6: 查询与前端投影

后端需要暴露可供前端使用的 runtime/backend execution summary，至少能展示：

- backend online / health
- active session count
- active sessions 基本信息
- executor availability
- 是否可作为自动分配候选

前端不得通过 runtime health 或 executor list 自行推断空闲状态。

### R7: 测试与文档

实现必须补齐 repository、allocator、relay session route、terminal release、disconnect release、explicit backend selection、auto idle selection 的测试，并将新的跨层契约沉淀到 spec。

## Acceptance Criteria

- [ ] 启动 session turn 时可以通过 API / launch command 明确指定 `backend_id`。
- [ ] 未指定 backend 且策略为自动分配时，系统会选择满足约束且 active lease 最少的在线 backend。
- [ ] 多个 backend 提供同一 executor 时，不再因为候选数量大于 1 直接失败。
- [ ] cancel relay session 时只向 session 实际归属 backend 下发 cancel。
- [ ] terminal、启动失败、取消、后端断连都会释放或标记对应 lease。
- [ ] runtime/backend 查询接口返回 active session count 与可分配状态，前端直接消费该状态。
- [ ] workspace binding 解析与 execution placement 在代码和文档中职责分离。
- [ ] 新增 PostgreSQL migration、repository readiness、SQL 映射和测试。
- [ ] 相关 backend / cross-layer spec 更新，解释新的事实源与数据流。

## Out Of Scope

- 不实现复杂队列、公平调度、优先级抢占或容量预留。
- 不支持一个 session turn 同时跨多个执行 backend。
- 不改造本机会话 SQLite 为云端事实源；本机 SQLite 仍只是 local runtime 的会话缓存。
- 不为旧字段、旧 API 或旧行为提供兼容 fallback。

## Planning Decisions

- 第一版自动分配策略只按 active lease count 排序，并使用稳定 tie-breaker；capacity / weight 留到后续共享高性能后端场景再引入。
- `BackendSelectionRequest` / `ExecutionPlacementPlan` 已收口在 application 层，并接入 session launch / `ExecutionContext.session.backend_execution`。relay connector 只消费已 claim placement。
