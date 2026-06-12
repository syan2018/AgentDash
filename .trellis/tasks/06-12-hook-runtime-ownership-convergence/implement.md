# Hook runtime ownership 模型收敛执行计划

## Mainline

本任务按一条重构主线推进：先收敛 Hook runtime owner 与执行器会话绑定，再收敛 runtime transition 的 frame/capability/presentation 事实，最后让前端只消费当前投影。Canvas create / present / user-open 是覆盖这条主线的高风险验证场景，不作为独立旁路实现。

## Phase 1: Inventory

- [ ] 盘点所有 hook runtime 创建、读取、刷新、失效入口。
- [ ] 盘点所有从 RuntimeSession 反查 AgentRun / AgentFrame 的 adapter 路径。
- [ ] 标出仍以 raw session id 表达 hook owner 的类型、缓存、service 方法和测试夹具。
- [ ] 盘点 `canvas create` / `workspace_module_present` / runtime capability transition 的 before-state、target 解析和 hook runtime 获取顺序。
- [ ] 盘点前端 session feed 聚合规则中 `context_frame` 的 boundary contract 和已有测试断言。
- [ ] 盘点 Workspace Panel tab 初始化、`workspace_module_presented` 事件处理、`workspace_module_create` result、AgentRunWorkspaceView projection 的 Canvas presentation 数据流。
- [ ] 盘点项目 Canvas 列表、Project workspace module projection、Workspace Panel “+ Canvas” 入口之间的候选数据源和当前缺失的用户主动 attach/open 后端路径。

## Phase 2: Ownership Contract

- [ ] 定义 hook runtime binding 的目标类型和命名，明确 owner 是 `HookControlTarget`。
- [ ] 调整 `SessionRuntimeRegistry` 或新增 binding store，使 delivery session lookup 只表达 adapter binding。
- [ ] 统一 `resolve_runtime_hook_target` / `AgentFrameRuntimeTarget` 的使用边界。
- [ ] 将 target mismatch 处理收敛到 service 层的 rebind / invalidate / rebuild，不让 Canvas 或 workspace module 工具直接收到旧缓存 mismatch。

## Phase 3: Runtime Transitions

- [ ] 在 AgentFrame revision 推进、runtime capability transition、workspace module 动态授权中统一同步 hook runtime target。
- [ ] 检查 AgentRun message / steer / pending promote / cancel 的 command receipt 与 action projection 刷新路径。
- [ ] 检查 companion gate parent/child 回流的 hook target 解析。
- [ ] 调整 capability context frame 构造，让 Capability Keys 的 added/removed 以 `CapabilityStateDelta.tool_capabilities` 为准，Hook runtime capability update 只同步内部 cache。
- [ ] 补 Canvas create / present 回归，断言 VFS/Skill 增量正确且已有基础 capability keys 不被标为新增。
- [ ] 为 Canvas create 和 AgentRun start 暴露 canonical presentation payload，确保 `canvas://{mount_id}` 与 `WorkspaceModuleDescriptor.ui_entries[].presentation_uri` 同源。
- [ ] 增加用户主动打开 Canvas 的 application/API 入口，复用 `expose_existing_canvas_for_session`、hook target rebind 和 capability transition，不新增前端-only 状态旁路。

## Phase 4: Frontend Projection

- [ ] 确认 AgentRun Workspace 页面只从 `AgentRunWorkspaceView.actions` 派生输入区动作。
- [ ] 将 `refreshing` / `error` / stale projection 期间的输入区 command state 改为只读，不允许使用上一帧 actions 执行 enqueue、steer 或 send_next。
- [ ] 为 turn terminal 后 action projection 刷新和 Ctrl/Cmd+Enter 分流补 focused test。
- [ ] 检查 RuntimeSession trace/detail 页面是否仍只消费 trace/control 视角。
- [ ] 将 `context_frame` 从工具 burst 的 soft boundary 改为截断工具聚合的 hard boundary，同时保留连续 CTX 内部聚合。
- [ ] 更新 `useSessionFeed` 相关测试，覆盖 CTX 前后工具不跨帧合并、连续 CTX 仍聚合。
- [ ] 调整 Workspace Panel Canvas tab 初始化与打开逻辑：空 `canvas://` 只显示未绑定态，不作为已有 Canvas 的链接；真实 Canvas tab 只从 start projection / create presentation / present event 创建。
- [ ] 补 `workspaceModulePresentedTabTarget` 或 WorkspacePanel focused tests，覆盖真实 `canvas://{mount_id}` 打开、空 `canvas://` 不误认为已关联、create 后立即激活真实 Canvas tab。
- [ ] 将 “+ Canvas” 改为具体 Canvas 选择入口，候选来自 Project workspace module / Canvas projection；选择后调用用户主动 open API，再按返回 presentation 打开 tab。
- [ ] 保持 `allowMultiple` 的多 Canvas 语义：不同 `canvas://{mount_id}` 可并存，同一 URI 选择时激活已有 tab。

## Phase 5: Validation And Spec

- [ ] 补后端回归测试：frame switch、Canvas create/present/user-open/start projection、capability diff、steer、pending promote、companion gate。
- [ ] 运行 focused Rust tests、session feed / workspace presentation / Canvas selector tests 和前端 typecheck。
- [ ] 执行 Architecture Cutover Gate：审计旧 RuntimeSession-first owner/cache 路径是否已删除或降级为 debug/trace，确认 HookControlTarget / AgentFrame / AgentRunWorkspaceView 已成为唯一业务权威链路。
- [ ] 输出最终 gate 记录：列出 Hook runtime 入口、RuntimeSession 保留职责、AgentFrame transition 入口、Frontend authority 入口，以及所有残留路径的处理结论。
- [ ] 涉及 DTO / migration 时运行 contracts 和 migration guard。
- [ ] 更新 session / hooks / frontend state spec，记录目标模型和设计原因。

## Suggested Checks

```powershell
cargo test -p agentdash-application hook_runtime
cargo test -p agentdash-application workspace_module
cargo test -p agentdash-application agent_run
pnpm --filter app-web test -- useSessionFeed
pnpm --filter app-web test -- AgentRunWorkspacePage.workspace-module
pnpm --filter app-web run typecheck
pnpm run migration:guard
```
