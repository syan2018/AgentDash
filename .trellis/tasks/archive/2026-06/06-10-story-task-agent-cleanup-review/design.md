# Story/Task Agent 残留链路清理设计

## Target Model

ProjectAgent 是唯一可运行 agent 配置与会话启动入口。Story / Task 不再表达 agent owner，也不再提供专用 runtime command route。它们只作为业务 subject、上下文声明来源和执行投影查询条件存在。

目标链路：

```text
ProjectAgent session start / message
  -> optional requested SubjectRef / context assignment
  -> LifecycleDispatchService
  -> LifecycleRun / LifecycleAgent / AgentFrame
  -> RuntimeSessionExecutionAnchor
  -> LifecycleSubjectAssociation
  -> SubjectExecutionView / LifecycleRunView projection
```

上下文分配目标：

```text
SubjectRef(kind, id)
  -> SubjectContextResolver
  -> Vec<Contribution>
  -> build_session_context_bundle
  -> AgentFrameBuilder.with_surface_input
```

不再存在：

```text
Task API start/continue
  -> Task-specific LaunchCommand / StoryStepSpec
  -> compose_story_step
```

```text
Story API launch
  -> default Story Agent
  -> Story owner session composer
```

## Parallel Workstreams

### A. SubjectContext Assignment

Owner: `06-10-subject-context-assignment`

建立一个 application 层 resolver。它从 `SubjectRef` 读取业务实体并生成 context contributions。它不启动 runtime session，不创建 agent，不写 lifecycle state。它只是把业务 subject 的上下文变成 frame construction 可消费的 contribution。

建议 shape：

```rust
pub struct SubjectContextAssignmentRequest {
    pub project_id: Uuid,
    pub subject_ref: SubjectRef,
    pub workspace_policy: SubjectWorkspacePolicy,
}

pub struct SubjectContextAssignment {
    pub subject_ref: SubjectRef,
    pub workspace: Option<Workspace>,
    pub contributions: Vec<Contribution>,
    pub capability_scope: CapabilityScopeCtx,
}
```

Task assignment 组合：

```text
Task binding contribution
  + parent Story context contribution
  + workspace declared sources
  + optional session plan fragments
```

Story assignment 组合：

```text
Story core contribution
  + Project core contribution
  + workspace declared sources
```

### B. Backend Hard Cut

Owner: `06-10-story-task-agent-command-hard-cut`

删除 Story/Task 专用 command route 和 frame construction 分叉。保留底层 ProjectAgent session start/message、lifecycle node composer、companion composer 和 read-only SubjectExecution projection。

Backend hard-cut 必须以 SubjectContext Assignment 作为替代模型：如果删除 `composer_story` 会影响 ProjectAgent subject context，则先把 ProjectAgent frame construction 接到 resolver。

### C. Frontend / Contracts / Capability Cleanup

Owner: `06-10-story-task-agent-frontend-contract-cleanup`

前端不再提供 Task start/continue/cancel 或旧 Story launch。Task 页面保留 SubjectExecution projection 和 run/session trace links。ProjectAgent 管理不再暴露 default Story/Task toggle，也不在当前 ProjectAgent 入口新增 subject 选择器。后续 Story 快速创建会话可以作为单独 Story surface 入口设计，但只能是 ProjectAgent session start + `subject_ref=story` 的薄 facade。

## Boundary Decisions

### Story / Task Business Model

保留 Story / Task 作为业务聚合和工作项模型。Story 继续持有 Task child entity；Task 继续保留 title、description、workspace_id、dispatch_preference 等 authoring-time 字段。Task status / artifacts 继续作为 projection 字段存在，但运行事实不写回 Task 作为 truth。

### Runtime Ownership

Runtime ownership 只从 Lifecycle control-plane 反查：`RuntimeSessionExecutionAnchor -> AgentFrame -> LifecycleAgent -> LifecycleRun -> LifecycleSubjectAssociation`。Story / Task 不再拥有独立 session owner path。

### Context Injection

Story / Task context 可以继续作为底层 ProjectAgent session start/message 的 subject context 输入，但不应需要 `OwnerScope::Story` / `StoryStepSpec` 这类专用 owner composer。目标是把 subject context 作为通用 frame construction contribution，而不是单独分叉出 Story/Task agent。

### Permission / Capability

`story_management` 可以保留为 agent 修改 Story/Task 业务数据的工具能力。`task_management::start_task` 这类直接启动 Task Agent 的 grant path 应删除或改义。若仍需要“让 agent 请求对某个 Task 继续工作”，应通过 ProjectAgent 通用消息或 Lifecycle interaction，而不是 Task command route。

## Migration Notes

This project is pre-release, so the cleanup should hard-cut old routes, DTOs, fields, and UI affordances. Database changes should still use forward migrations. If `is_default_for_story` / `is_default_for_task` columns are removed, add a forward migration and update repository row mapping in the same implementation slice.

## Product Decision

已确认：底层允许 ProjectAgent session start/message 携带可选 `subject_ref`，但当前 ProjectAgent UI 入口不新增 subject 选择器。

后续 Story 可以设计“快速创建会话”入口。该入口必须是薄 facade：选择合适的 ProjectAgent，调用 ProjectAgent session start，并传入 `subject_ref=story` 触发 SubjectContext assignment。它不能重新引入 Story Agent、Story owner session 或 `/stories/{id}/launch` 的旧语义。Task 暂不设计快速创建会话入口；Task 通过 Story 会话、ProjectAgent 会话或只读 projection 被处理。
