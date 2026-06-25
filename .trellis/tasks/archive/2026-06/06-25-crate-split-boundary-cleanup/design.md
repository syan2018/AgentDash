# 清理 crate split 后遗留边界回退设计

## 设计结论

本任务推荐分两步处理：

1. 先修 Workflow AgentCall frame construction 的生产路径，使 Lifecycle dispatch 触发的 command 与 AgentRun production adapter 能力一致。
2. 再收束 Lifecycle read model 的所有权，使 AgentRun 消费一个稳定 read-model contract，而不是维护 Lifecycle projection 的复制版。

这两个问题来自同一类 Round 5 临时措施：为了先解除 crate split 编译阻塞，把跨 crate 关系改成局部绕行。修复方向不是补更多 fallback，而是把缺失的 contract 明确化。

## Workflow AgentCall Frame Construction

### 当前问题

`AgentNodeLauncher` 构造了一个局部 `WorkflowNodeFrameConstructionPort`，在端口内部把 `DispatchLaunchAnchor` 转成 `ComposeLaunchSurface`。这个转换试图让 workflow node 走新的 compose 路径，但 production 注入的 `AgentRunLaunchAnchorFrameConstructionAdapter` 仍只支持 `DispatchLaunchAnchor`。

同时，`LifecycleDispatchService::materialize_workflow_agent_node` 当前顺序是：

```text
create LifecycleAgent
create RuntimeSession
construct launch frame
upsert RuntimeSessionExecutionAnchor
bind current delivery
return runtime refs
```

如果 compose path 依赖 `RuntimeSessionExecutionAnchor::find_by_session` 读取 orchestration context，那么在 anchor upsert 之前 compose 会缺上下文。这个顺序说明 command 改写本身不是稳定设计。

### 推荐方案

把 Workflow AgentCall 的 frame construction 改成显式 materialization flow：

```text
LifecycleDispatchService::materialize_workflow_agent_node
  -> create LifecycleAgent
  -> create RuntimeSession
  -> build WorkflowAgentNodeFrameConstructionInput
  -> AgentRun workflow-node frame construction adapter
  -> create AgentFrame with lifecycle node surface
  -> upsert RuntimeSessionExecutionAnchor with launch_frame_id
  -> bind current delivery
  -> reducer records NodeStarted
```

关键点：

- Lifecycle dispatch 负责控制面事实：agent、runtime session、anchor、delivery binding。
- AgentRun frame construction adapter 负责 surface：base frame、workflow node lifecycle mount、AgentProcedure contract、workflow label、executor config inheritance。
- Adapter input 使用显式 fields 传递 orchestration facts，避免在 anchor 写入前反查 session。
- `ComposeLaunchSurface` 不作为长期边界保留。当前代码没有 production adapter owner，只有 `DispatchLaunchAnchor -> ComposeLaunchSurface` 的局部改写和测试枚举命中；默认处理是删除该 command 及 `FrameConstructionReason`。

### Contract 形状

优先选择新增窄口 workflow-node materialization port，而不是继续扩大 `FrameConstructionCommand`：

```rust
pub trait WorkflowAgentNodeFrameMaterializationPort: Send + Sync {
    async fn materialize_workflow_agent_node_frame(
        &self,
        input: WorkflowAgentNodeFrameMaterializationInput,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError>;
}
```

优点：

- Workflow node construction 与 plain launch anchor construction 不再塞进同一个 enum 分支。
- Lifecycle crate 仍只依赖 ports，不依赖 AgentRun implementation。
- Production adapter 能根据完整 input 创建 workflow node frame。
- Anchor 写入仍由 Lifecycle dispatch 完成，避免 compose 反查未写入 anchor。

备选方案是在 `FrameConstructionCommand` 中新增 `WorkflowNodeLaunch`，但这只是把另一个 construction 语义塞回同一个大 enum。除非实现时发现 bootstrap wiring 代价过大，否则 MVP 推荐窄口 port。

### Adapter 行为

`AgentRunLaunchAnchorFrameConstructionAdapter` 保持只处理 launch anchor；Workflow node 另建 adapter，避免名字和职责继续错位：

```text
DispatchLaunchAnchor -> new_launch_anchor frame
WorkflowAgentNodeFrameMaterializationPort -> compose_lifecycle_node_to_frame_with_audit + build frame
```

Workflow adapter 需要 `RepositorySet`、`PlatformConfig`、`LifecycleSurfaceProjectionPort` 等上下文时，应在 API bootstrap 注入完整 deps，而不是通过 `ComposeLaunchSurface` 让 launch-anchor adapter 反向猜测。

### 删除清单

- 删除 `WorkflowNodeFrameConstructionPort` 中的 `DispatchLaunchAnchor -> ComposeLaunchSurface` 改写。
- 删除 ports 中无 production owner 的 `ComposeLaunchSurface` 和 `FrameConstructionReason`。
- 删除 AgentRun facade 中重复定义的 `ComposeLaunchSurface` / `FrameConstructionReason`，若整个 local construction facade 仅服务测试或已被 ports 替代，则进一步收束到 ports 类型或删除未使用 construct 分支。
- 清理只为 `ComposeLaunchSurface` 存在的测试，改成测试 workflow-node materialization port。

## Lifecycle Read Model Ownership

### 当前问题

AgentRun crate 新增 `agent_run/lifecycle_read_model.rs`，Lifecycle crate 仍保留 `lifecycle/run_view_builder.rs`。两者承担同类 projection：Lifecycle run view、subject execution view、runtime attempts、agent view、association view。

这会让 read-model 语义拥有两个 owner。后续任何一个 view 字段、排序、association 选择、attempt 过滤规则更新，都可能只改一边。

### 推荐方案

将 Lifecycle read model 收束为单 owner，推荐顺序如下：

1. 把 DTO 和 pure conversion helpers 移入 `agentdash-application-ports` 或新的 lightweight read-model contract module。
2. Lifecycle crate 保留 projection implementation，暴露 `LifecycleReadModelQueryPort` 或 public query facade。
3. AgentRun presentation/workspace 查询通过 port/facade 获取 `LifecycleRunView` / `LifecycleSubjectAssociationView` / `RuntimeSessionRefView`。
4. 删除 AgentRun 内部复制版 `lifecycle_read_model.rs`。

这样做的原因：

- Lifecycle domain projection 规则天然属于 Lifecycle owner。
- AgentRun 需要消费 view，但不应拥有 subject association / orchestration / runtime node history 的解释规则。
- DTO 放到 ports 可以避免 AgentRun 反向依赖 Lifecycle implementation，同时保持 compile boundary 清晰。

### Contract 形状

建议最小 contract：

```rust
pub trait LifecycleReadModelQueryPort: Send + Sync {
    async fn lifecycle_run_view(&self, run_id: Uuid) -> Result<LifecycleRunView, LifecycleReadModelError>;
    async fn subject_execution_view(&self, subject: SubjectRef) -> Result<SubjectExecutionView, LifecycleReadModelError>;
}
```

如果当前调用点已经持有 `LifecycleRun` 和 repository set，也可以先暴露 pure builder facade：

```rust
pub async fn build_lifecycle_run_view(repos: &LifecycleRepositorySet, run: &LifecycleRun) -> ...
```

但最终应避免 AgentRun 拥有 `LifecycleRepositorySet` 的完整副本来运行 projection。更理想的 boundary 是 composition root 注入 query port。

## Skill Fix Boundary

SkillAsset builtin bootstrap 已作为前置修复处理。本任务只在验证阶段确认：

```text
EnsureAndProject builtin skill -> projected skill key -> VFS read succeeds
```

这个检查存在的原因是 Workflow node lifecycle surface 会继续依赖 lifecycle skill projection；本任务调整 frame construction 时需要确保没有回退前序修复。

## 风险与回滚点

- Workflow AgentCall 是运行期控制面路径，frame 成功但 anchor / delivery binding 未写入会制造不可取消、不可追踪的 runtime session。实现时先加测试，再改 adapter。
- `FrameConstructionCommand` 扩展会影响 API bootstrap、tests、Lifecycle dispatch 和 AgentRun project-agent start tests。提交前需要跑 targeted crate tests。
- Read model contract 移动容易扩大依赖面。优先移动 DTO/pure view 类型，再迁移调用点，最后删除复制实现。
- 如果 read model 收束牵涉过多 crate graph 调整，可以先完成 Workflow AgentCall 修复并保留 read model plan；但任务完成标准仍要求 read model 单 owner。

## 推荐实施顺序

1. 增加 failing regression test：production adapter 下 Workflow AgentCall materialization 不 rejected，并写出 lifecycle node frame surface。
2. 删除 `ComposeLaunchSurface` 重复路径和 command 改写。
3. 新增 workflow-node construction port 与 production adapter。
4. 升级 API bootstrap wiring，为 workflow-node adapter 注入 deps。
5. 跑 Lifecycle / AgentRun targeted tests。
6. 移动 read model DTO contract，建立 Lifecycle query facade/port。
7. 迁移 AgentRun presentation/workspace 调用点。
8. 删除 AgentRun 复制 read-model implementation，增加静态检查。
