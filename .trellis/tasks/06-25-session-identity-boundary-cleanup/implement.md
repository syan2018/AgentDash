# Implementation Plan

## Phase 0: Audit Baseline

- [ ] 建立 session identity audit 清单，按模块分类当前 `session_id` / `runtime_session_id` 使用点：
  - allowed session zone
  - adapter-only usage
  - forbidden business usage
- [ ] 为禁止区建立 grep/check 脚本或文档化命令，避免任务中途反复回归。
- [ ] 更新 backend spec，先记录 session identity isolation 原则。

Recommended audit commands:

```powershell
rg "runtime_session_id|delivery_runtime_session_id|find_by_session|RuntimeContext::Session|RuntimeActor::AgentSession|SessionMetaUpdate" crates/agentdash-workspace-module crates/agentdash-application crates/agentdash-application-agentrun crates/agentdash-application-lifecycle crates/agentdash-api crates/agentdash-contracts crates/agentdash-domain -g "*.rs" -n
rg "session_id" crates/agentdash-workspace-module packages/app-web/src/generated packages/app-web/src/services packages/app-web/src/types -n
```

## Phase 1: AgentRun Delivery Boundary

- [ ] 定义 AgentRun delivery context / target 类型，用于替代外围业务中的 runtime session id。
- [ ] 在 application/session adapter 中集中实现：
  - `ExecutionContext -> AgentRunDeliveryContext`
  - `AgentRunDeliveryContext -> runtime session delivery address`
  - `runtime session -> AgentRunDeliveryContext` 只允许 adapter 内部使用
- [ ] 将 `RuntimeSessionExecutionAnchorRepository::find_by_session` 使用点收束到 adapter 内，禁止 workspace-module/canvas/workflow business 直接调用。

## Phase 2: Workspace Module Adapter Extraction

- [ ] 将 `WorkspaceModuleRuntimeToolProvider` 从 `agentdash-workspace-module` 外移到 adapter 边界。
- [ ] `agentdash-workspace-module` 删除对以下 crate/type 的依赖：
  - `agentdash-spi::ExecutionContext`
  - `agentdash-application-runtime-gateway`
  - `agentdash-application-vfs::VfsService`
- [ ] Workspace Module tool business 改为接收 `AgentRunDeliveryContext` 和纯业务 ports。
- [ ] `workspace_module_invoke` 改为产出 invocation intent，由 adapter 调 RuntimeGateway / extension channel。
- [ ] `workspace_module_present` 改为产出 presentation intent，由 adapter 构造 runtime/session event 或 future AgentRun workspace event。
- [ ] `canvas.inspect_render_state` / `canvas.get_interaction_state` 改为通过 AgentRun delivery context 查询 Canvas runtime state。

## Phase 3: Canvas / AgentRun Mailbox Contract Cleanup

- [ ] Canvas runtime snapshot 去除 `session_id` 字段和 Session 文案，改为 AgentRun bridge / workspace surface 状态。
- [ ] Canvas API/SDK/generated contract 删除 `session_id` 输入。
- [ ] AgentRun mailbox domain contract 删除 `runtime_session_id` 业务字段，替换为 AgentRun delivery identity 或 adapter trace ref。
- [ ] Canvas submit-to-Agent、interaction state、render observation 全链路以 AgentRun 关联为事实源。

## Phase 4: Peripheral Business Cleanup

- [ ] Workflow/Lifecycle：runtime session id 不再作为 node/provenance 的业务字段；如需 live delivery，使用 AgentRun/workflow node delivery target。
- [ ] Companion/Hook：parent/child 关联不再以 session id 表达，改为 dispatch id、AgentRun/frame/node/request id；session notification 只留在 adapter。
- [ ] Permission：permission grant provenance 不再以 runtime session id 作为业务来源；改为 run/frame/action/request provenance。
- [ ] Task context builder：Canvas visibility / runtime surface 不通过 runtime session 反查，改为 AgentRun/runtime surface query。
- [ ] VFS surface：业务 source 不使用 `SessionRuntime`；改为 AgentRun/workspace/canvas source。
- [ ] Terminal：公开 route/contract 不以 session id 作为业务 target；session id 只在 stream adapter 内部路由。

## Phase 5: Migration and Contract Regeneration

- [ ] 为外围业务表字段修改增加 PostgreSQL migration。
- [ ] 更新 Rust contracts 并 regenerate TypeScript contracts。
- [ ] 更新前端调用点，移除 Canvas/Workspace/VFS/Terminal 外围 API 的 session id 输入。

## Phase 6: Verification

- [ ] `cargo check -p agentdash-workspace-module --tests`
- [ ] `cargo check -p agentdash-application -p agentdash-application-agentrun -p agentdash-application-lifecycle -p agentdash-api --tests`
- [ ] 相关 Rust 单测，尤其是 workspace module、Canvas runtime state、AgentRun mailbox、workflow/companion/hook/permission。
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web run typecheck`
- [ ] 受影响前端测试：Canvas runtime preview、workspace panel、extension canvas panel、canvas service。
- [ ] 禁止区 grep gate 无违规残留。

## Review Gates

- [ ] 开始实现前确认 scope：第一轮是否覆盖所有外围模块，还是先完成 Workspace Module / Canvas / AgentRun mailbox 主链路。
- [ ] 每个阶段结束后检查是否只是“改名”而没有消除 session 作为关联键。
- [ ] 任何保留的 session id 使用点必须能归入 allowed session zone，并在代码结构上位于 session/runtime adapter 内。

## Rollback Points

- Workspace Module adapter extraction 和 peripheral cleanup 分开提交，避免 crate dependency 改动与全系统 contract 改动混在一起。
- 数据库 migration 与 generated contract regeneration 单独检查，便于定位 schema/DTO break。
- 若 RuntimeGateway Session Action 短期无法去除，保留在 adapter 内部，不阻塞业务层去 session 化。
