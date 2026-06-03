# Session Runtime 控制面事实源设计

## Architecture Boundary

Session 是用户消息流壳；runtime session id 只定位这条消息流。业务控制面事实由 `RuntimeSessionExecutionAnchor` 连接到 `LifecycleRun`、`LifecycleAgent`、`AgentFrame` 与 activity attempt。前端只消费后端 read model，不自行重建控制面关系。

目标数据流：

```text
GET /sessions/{runtime_session_id}/runtime-control
  -> load SessionMeta
  -> load RuntimeSessionExecutionAnchor
  -> load LifecycleRun
  -> load LifecycleAgent
  -> load current AgentFrame
  -> load run/agent subject associations
  -> build SessionRuntimeControlView
```

Session 列表数据流：

```text
GET /projects/{project_id}/sessions
  -> list project SessionMeta shell rows
  -> join anchors by runtime_session_id
  -> load LifecycleRun / LifecycleAgent / current AgentFrame refs
  -> project subject label
  -> build ProjectSessionListView
```

## Backend Contracts

### Session Shell

`SessionMeta` / `sessions` 保存用户壳字段：

- `id`
- `project_id`
- `title`
- `title_source`
- `created_at`
- `updated_at`
- `last_event_seq`
- `last_turn_id`
- `last_delivery_status`
- `tab_layout_json`

`last_delivery_status` 表达消息流或投递状态。run status、agent status、assignment、attempt 与 activity status 从 lifecycle 控制面投影。

### RuntimeSessionExecutionAnchor

Anchor 是 runtime session 到控制面的唯一索引：

```rust
pub struct RuntimeSessionExecutionAnchor {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub launch_frame_id: Uuid,
    pub assignment_id: Option<Uuid>,
    pub graph_instance_id: Option<Uuid>,
    pub activity_key: Option<String>,
    pub attempt: Option<i32>,
    pub created_by_kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Repository 需要支持：

- `find_by_session(runtime_session_id)`
- `list_by_run(run_id)`
- `list_by_agent(agent_id)`
- `list_by_project_session_ids(session_ids)`
- `latest_for_agent(agent_id)`

### SessionRuntimeControlView

标准 Session 页面入口：

```rust
pub struct SessionRuntimeControlView {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub session_meta: SessionShellDto,
    pub anchor: RuntimeSessionExecutionAnchorDto,
    pub run: LifecycleRunView,
    pub agent: LifecycleAgentView,
    pub frame_runtime: AgentFrameRuntimeView,
    pub subject_associations: Vec<LifecycleSubjectAssociationDto>,
    pub can_send: bool,
    pub send_unavailable_reason: Option<String>,
}
```

`can_send` 由后端根据 anchor、agent status、current frame 与 frame runtime surface 解析，不由前端猜测。

### ProjectSessionListView

项目会话列表入口：

```rust
pub struct ProjectSessionListView {
    pub project_id: String,
    pub sessions: Vec<ProjectSessionListEntry>,
}

pub struct ProjectSessionListEntry {
    pub runtime_session_id: String,
    pub title: String,
    pub delivery_status: String,
    pub run_status: Option<LifecycleRunStatus>,
    pub run_ref: Option<LifecycleRunRefDto>,
    pub agent_ref: Option<LifecycleAgentRefDto>,
    pub frame_ref: Option<AgentFrameRefDto>,
    pub subject_ref: Option<SubjectRefDto>,
    pub subject_label: Option<String>,
    pub updated_at: String,
}
```

列表可以包含没有 anchor 的 trace shell，但这类 entry 没有 run/agent/frame refs，并且不能被视作可发送 Agent 会话。

## Frontend Contracts

`SessionPage` 主查询：

```ts
fetchSessionRuntimeControl(runtimeSessionId): Promise<SessionRuntimeControlView>
```

`WorkspaceRuntimeData` 目标结构：

```ts
interface WorkspaceRuntimeData {
  projectId: string | null;
  runtimeSessionId: string | null;
  sessionMeta: SessionShell | null;
  controlAnchor: RuntimeSessionExecutionAnchorView | null;
  lifecycleRun: LifecycleRunView | null;
  lifecycleAgent: LifecycleAgentView | null;
  frameRuntime: AgentFrameRuntimeView | null;
  subjectAssociations: LifecycleSubjectAssociationDto[];
  extensionRuntime: ProjectExtensionRuntimeState;
  activeCanvasId: string | null;
}
```

`ContextOverviewTab` 消费单一 `lifecycleRun`。active workflow metadata 只帮助定位 activity / attempt，不决定 run 选择。

会话列表消费 `ProjectSessionListView`，不从 `lifecycleStore` 组合 Session entry。

## Validation Notes

后端 route 先校验 session shell 的 project 权限，再校验 anchor run project 与 session project 一致。缺失 anchor 表示 runtime trace 不能作为业务控制面会话继续发送。

前端只显示用户可理解的 Session 文案。Lifecycle 术语保留在详情页、debug 和内部类型中。
