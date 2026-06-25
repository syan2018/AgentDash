# 清理 crate split 后遗留边界回退

## Goal

清理 Round 5 crate split 中为解除编译阻塞遗留的边界回退，使 Workflow AgentCall frame construction 和 Lifecycle read model 回到明确的所有者模型。

本任务的价值是把“能编译”的临时桥接改成可维护的应用边界：Workflow AgentCall 启动必须有一条生产可用的 frame construction 路径，AgentRun 与 Lifecycle 之间的 read model 共享必须通过明确 contract 表达，而不是复制实现。

## Confirmed Facts

- `crates/agentdash-application-lifecycle/src/workflow/orchestration/agent_node_launcher.rs` 中的 `WorkflowNodeFrameConstructionPort` 会把 `DispatchLaunchAnchor` 改写成 `ComposeLaunchSurface`。
- `crates/agentdash-api/src/bootstrap/repositories.rs` 当前注入的 production `agent_frame_construction` 是 `AgentRunLaunchAnchorFrameConstructionAdapter`。
- `crates/agentdash-application-agentrun/src/agent_run/frame/lifecycle_materialization.rs` 中的 `AgentRunLaunchAnchorFrameConstructionAdapter` 只接受 `DispatchLaunchAnchor`，收到 `ComposeLaunchSurface` 会返回 construction rejected。
- `LifecycleDispatchService::materialize_workflow_agent_node` 在调用 frame construction 成功后才 upsert `RuntimeSessionExecutionAnchor`，而 workflow node surface compose 需要 orchestration/run/node 上下文。
- `crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model.rs` 与 `crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs` 当前保留了同类 Lifecycle run / subject execution projection 逻辑。
- AgentRun presentation/workspace 查询已经引用 AgentRun crate 内部的 `lifecycle_read_model`，形成第二个 Lifecycle read-model owner。
- SkillAsset builtin bootstrap / projection 问题已由前序修复覆盖，本任务只保留回归检查，避免本轮方案扩大为 skill 清理任务。

## Requirements

- Workflow AgentCall materialization 使用一条 production 可执行的 frame construction adapter，能够创建含 lifecycle node surface 的 AgentFrame，并保持 LifecycleAgent、RuntimeSession、ExecutionAnchor、NodeStarted 状态一致。
- 删除 Round 5 遗留的重复 launch construction 路径：`DispatchLaunchAnchor` 不能再被改写为 `ComposeLaunchSurface`；若 `ComposeLaunchSurface` / `FrameConstructionReason` 没有独立 production owner，应从 ports 与 AgentRun facade 中移除。
- Frame construction command contract 只保留真实生产语义：dispatch launch anchor、workflow node materialization、accepted launch commit；不保留未接入 production adapter 的中间 compose command。
- Workflow node surface compose 的上下文来源稳定：run、agent、runtime session、orchestration id、node path、attempt、plan node、AgentProcedure contract、workflow label 均来自同一个可验证的 materialization flow。
- Production bootstrap 注入的 adapter 支持 Workflow AgentCall 需要的 command，或 command flow 调整为当前 adapter 语义可表达的路径。
- Lifecycle read model 只有一个实现所有者；AgentRun 通过 shared contract、port 或明确的 query facade 消费 read model，不复制 Lifecycle projection 逻辑。
- Read-model DTO 的位置表达跨 crate contract，不把 Lifecycle implementation 细节倒灌进 AgentRun，也不让 AgentRun 拥有 Lifecycle domain projection 规则。
- 增加回归测试覆盖 Workflow AgentCall materialization：AgentFrame 中存在 workflow node lifecycle mount / contract projection，ExecutionAnchor 与 runtime node state 对齐，production adapter command 不被拒绝。
- 增加静态检查或单元测试覆盖 read model 单所有者，防止 `lifecycle_read_model.rs` / `run_view_builder.rs` 再次出现并行实现。
- 保留 SkillAsset bootstrap 已修复语义的回归检查：`EnsureAndProject` 仍能确保 builtin skill 可读，但本任务不重新设计 SkillAsset mutation。

## Acceptance Criteria

- [ ] Workflow AgentCall 启动路径不再出现 `DispatchLaunchAnchor` 被改写成 production adapter 不支持 command 的情况。
- [ ] `ComposeLaunchSurface` / `FrameConstructionReason::LifecycleAgentProcedure` 这类无 production owner 的重复路径被删除，或被证明仍有唯一合法 owner；默认目标是删除。
- [ ] `AgentRunLaunchAnchorFrameConstructionAdapter` 或其替代 production adapter 覆盖 Workflow AgentCall 所需 frame construction，但不通过 `DispatchLaunchAnchor -> ComposeLaunchSurface` 改写实现。
- [ ] Workflow AgentCall materialization 在一次测试中证明 agent、runtime session、launch frame、execution anchor、current delivery binding、`NodeStarted` 事件一致。
- [ ] Workflow node AgentFrame surface 测试证明 lifecycle mount、orchestration node facts、AgentProcedure contract output ports / label 投影进入 frame。
- [ ] AgentRun 与 Lifecycle 之间的 read model projection 只保留一个实现位置；另一个 crate 只消费 contract / facade。
- [ ] `rg "pub\\(crate\\) mod lifecycle_read_model|pub mod run_view_builder"` 之类的静态检查能说明 read model owner 不再重复。
- [ ] `cargo test -p agentdash-application-lifecycle --lib` 和 `cargo test -p agentdash-application-agentrun --lib` 通过，或失败项被确认与本任务无关且有明确记录。
- [ ] crate split forbidden-edge 静态检查继续通过，不通过增加旧路径兼容层完成修复。
- [ ] SkillAsset builtin projection 回归检查仍通过，证明本任务没有回退前序 skill 修复。

## Out of Scope

- SkillAsset builtin bootstrap 的重新实现；该问题已由前序修复覆盖。
- 全量重做 `FrameConstructionService`、runtime surface update service 或 AgentRun frame/surface command boundary；本任务只清理 Round 5 遗留的错误桥接和重复 read model。
- 为历史 API 或旧 module path 保留兼容层；项目未上线，边界可直接改到正确形态。
- 数据库迁移；本任务预期只调整应用层 command/read-model 边界和测试。

## Open Questions

- 方案默认选择“先修 production AgentCall 硬伤，再收束 read model owner”。如果实现中发现 read model contract 需要跨 crate 新包，应在设计评审时确认包名与 ownership。
