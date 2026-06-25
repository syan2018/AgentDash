# 清理 crate split 后遗留边界回退执行计划

## 阶段 0：计划确认

- [x] 创建任务。
- [x] 写入 PRD、设计方案和实施计划。
- [ ] 用户确认是否按本方案进入实现阶段。

## 阶段 1：Workflow AgentCall 回归护栏

- [ ] 增加测试覆盖 production wiring 语义：`AgentNodeLauncher` / `LifecycleDispatchService::materialize_workflow_agent_node` 使用 production `AgentRunFrameConstructionPort` 时不会因 command 不支持失败。
- [ ] 增加测试覆盖 Workflow AgentCall materialization 结果：
  - LifecycleAgent 创建成功。
  - RuntimeSession 创建成功。
  - AgentFrame 创建成功并包含 lifecycle node surface。
  - RuntimeSessionExecutionAnchor 指向 launch frame。
  - current delivery binding 为 ready/running 语义正确。
  - orchestration reducer 记录 `NodeStarted`。
- [ ] 增加测试覆盖 AgentProcedure contract output ports / workflow label / node facts 被写入 frame surface。

## 阶段 2：Frame Construction Contract 修复

- [ ] 删除 `AgentNodeLauncher::WorkflowNodeFrameConstructionPort` 对 `DispatchLaunchAnchor` -> `ComposeLaunchSurface` 的命令改写。
- [ ] 删除 ports 中无 production owner 的 `ComposeLaunchSurface` / `FrameConstructionReason`。
- [ ] 删除 AgentRun local facade 中重复的 `ComposeLaunchSurface` / `FrameConstructionReason`；若 local construction facade 与 ports 重复，则同步收束到单一 contract。
- [ ] 新增窄口 `WorkflowAgentNodeFrameMaterializationPort`，专门服务 Workflow AgentCall frame materialization。
- [ ] 修改 `LifecycleDispatchService::materialize_workflow_agent_node`，传递完整 workflow-node construction facts。
- [ ] 保持 `AgentRunLaunchAnchorFrameConstructionAdapter` 只支持普通 `DispatchLaunchAnchor`，另建 Workflow node adapter 使用 `compose_lifecycle_node_to_frame_with_audit` 或同等 composer 写入 lifecycle node surface。
- [ ] 更新 API bootstrap wiring，为 workflow-node adapter 注入 composer 所需 deps。
- [ ] 更新相关 test fixtures，避免测试 adapter 只支持旧 command 导致覆盖不到 production 语义。

## 阶段 3：Read Model Contract 收束

- [ ] 盘点 AgentRun 当前消费的 Lifecycle read-model 类型：
  - `AgentRunView`
  - `LifecycleRunView`
  - `LifecycleSubjectAssociationView`
  - `RuntimeSessionRefView`
  - subject execution / runtime attempt 相关 view
- [ ] 将共享 DTO 移入 ports 或明确的 read-model contract module。
- [ ] 保留 Lifecycle crate 作为 projection implementation owner。
- [ ] 为 AgentRun 提供 `LifecycleReadModelQueryPort` 或 composition-injected query facade。
- [ ] 迁移 AgentRun presentation/workspace/conversation snapshot 调用点，改为消费 query port / shared DTO。
- [ ] 删除 `crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model.rs` 中的复制 projection implementation。
- [ ] 增加静态检查，确认 Lifecycle read model implementation 只有一个 owner。

## 阶段 4：Skill 回归检查

- [ ] 覆盖 `EnsureAndProject` builtin skill projection：builtin key 写入 lifecycle mount metadata 后，VFS 能读到对应 skill asset 文件。
- [ ] 确认本任务没有改回已修复的 skill bootstrap 语义。

## 阶段 5：验证命令

- [ ] `cargo fmt --check`
- [ ] `cargo metadata --no-deps --format-version 1`
- [ ] `cargo test -p agentdash-application-lifecycle --lib`
- [ ] `cargo test -p agentdash-application-agentrun --lib`
- [ ] `cargo check -p agentdash-application-lifecycle --message-format short`
- [ ] `cargo check -p agentdash-application-agentrun --message-format short`
- [ ] crate split forbidden-edge checks from `.trellis/tasks/06-24-release-crate-split-draft/dispatch-round-5.md`
- [ ] `git diff --check`

## 风险文件

- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_facade.rs`
- `crates/agentdash-application-lifecycle/src/workflow/orchestration/executor_launcher.rs`
- `crates/agentdash-application-agentrun/src/agent_run/frame/lifecycle_materialization.rs`
- `crates/agentdash-application-agentrun/src/agent_run/frame/construction/**`
- `crates/agentdash-application-agentrun/src/agent_run/frame/mod.rs`
- `crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model.rs`
- `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs`
- `crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs`
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs`
- `crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs`
- `crates/agentdash-api/src/bootstrap/repositories.rs`
- `crates/agentdash-application-ports/src/agent_frame_materialization.rs`

## 回滚点

- 阶段 2 完成后先跑 Workflow AgentCall targeted tests；若新增窄口 port 形状不合适，调整 port input，而不是恢复 `ComposeLaunchSurface` 或继续扩大 `FrameConstructionCommand`。
- 阶段 3 移动 DTO 后先跑 AgentRun read model consumers；若 crate graph 变复杂，保留 DTO 移动，暂缓 facade 注入，避免恢复复制实现。
- 任一阶段不得通过旧路径 compatibility shell 让 forbidden-edge check 变绿；失败时回到 contract 设计调整。
