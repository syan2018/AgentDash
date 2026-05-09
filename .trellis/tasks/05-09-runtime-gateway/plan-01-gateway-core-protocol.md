# 主线 1：Gateway Core Protocol

## Goal

建立 Runtime Gateway 的核心协议和应用层内核，让后续 Session Runtime Action 与 Setup Action 都能使用同一套 Invocation / Provider / Result / Audit 模型。该主线只处理抽象、协议和值对象，不迁移具体业务入口。

## Scope

- Runtime Gateway 服务接口。
- Runtime Action / Actor / Context / Target / Policy / Invocation / Result / Error 值对象。
- Runtime Provider SPI。
- Action key 命名与注册机制。
- Session Runtime Action 与 Setup Action 的协议分型。

## Dependencies

- 依赖现有 `CapabilityState`、`SessionFrame` / `TurnFrame`、`McpRelayProvider`、VFS provider 思路作为设计输入。
- 不依赖 Canvas / Workflow / Setup route 的具体迁移。

## Execution Plan

1. 盘点现有调用面：
   - Agent runtime tools。
   - direct MCP / relay MCP。
   - workspace detect / browse directory / MCP probe。
   - terminal / workspace prepare。
2. 定义核心类型：
   - `RuntimeActionKey`
   - `RuntimeActor`
   - `RuntimeContext`
   - `RuntimeTarget`
   - `RuntimePolicy`
   - `RuntimeInvocationRequest`
   - `RuntimeInvocationResult`
   - `RuntimeInvocationError`
   - `RuntimeTrace`
3. 定义 action 分型：
   - `SessionRuntimeAction`：必须带 `session_id`。
   - `SetupAction`：不带 `session_id`，但必须带平台配置上下文。
4. 定义 `RuntimeProvider` trait：
   - `supports(action_key, context) -> bool`
   - `invoke(request) -> result`
   - 可选 `list_actions(context)` / `describe_action(action_key)`。
5. 定义 `RuntimeGateway` 服务入口：
   - action registry。
   - provider router。
   - policy hook。
   - trace hook。
6. 补最小单元测试：
   - 未注册 action 拒绝。
   - Session action 缺 `session_id` 拒绝。
   - Setup action 带普通 runtime actor 拒绝。
   - provider 返回错误时保留 trace_id。

## Acceptance Criteria

- 核心类型能表达 Session Runtime Action 与 Setup Action 的差异。
- Provider SPI 不依赖 Canvas、Relay 裸 DTO 或前端 DTO。
- Gateway 内核不直接操作本机文件系统或数据库业务实体，只依赖抽象 provider / policy / trace。
- 错误模型能区分 invalid_request、capability_denied、provider_unavailable、provider_failed、timeout。
- 单元测试覆盖核心路由和校验。

## Risks

- 过早把 provider trait 做得太泛会失去类型约束。
- 如果直接复用 relay DTO，后续会污染 application/domain 边界。
- 如果 action key 只是自由字符串，能力过滤和审计会变弱。

## First PR Shape

- 只新增 Runtime Gateway 内核模块和值对象。
- 不迁移现有 route。
- 不接 Canvas / Agent / Workflow。
- 目标是让后续主线可以并行基于同一协议实现 provider。
