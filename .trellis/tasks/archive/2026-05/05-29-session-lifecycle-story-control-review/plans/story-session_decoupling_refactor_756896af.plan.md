---
name: Story-Session Decoupling Refactor
overview: 将 Story 从 Session/LifecycleRun 的隐式绑定中彻底解耦：引入显式 LifecycleRunLink 替代 session 反查路径，Session 降级为 runtime substrate，前后端一体化切换到 Story/LifecycleRun/Activity 主语。Agent Permission System 作为独立后续任务。
todos:
  - id: phase-1-domain
    content: "Phase 1: Domain 层 - 新增 LifecycleRunLink entity/repo, 修改 LifecycleRun.session_id 语义, 写 migration"
    status: in_progress
  - id: phase-2-application
    content: "Phase 2: Application 层 - RunLinkService, 重构 StoryStepActivationService 和 LifecycleRun 创建路径, Routine executor 调整"
    status: completed
  - id: phase-3-capability
    content: "Phase 3: CapabilityResolver 过渡改造 - 引入 CapabilityContext, 保留 owner_ctx 兼容但新增 run context 路径"
    status: completed
  - id: phase-4-api
    content: "Phase 4: API 层 - 新增 /stories/{id}/runs 等 run-oriented API, 降级 session API, 更新 TS contracts"
    status: completed
  - id: phase-5-frontend
    content: "Phase 5: 前端迁移 - storyStore 从 session 切到 runs, 页面展示 Activity timeline, 导航切换"
    status: completed
  - id: phase-6-cleanup
    content: "Phase 6: Spec 更新 + 残留清理 + 验证"
    status: completed
isProject: false
---

# Story-Session 解耦与控制面重构

## 现状核心耦合（经代码审计确认）

当前 `LifecycleRun.session_id: StorySessionId` 是所有耦合的根因：

```
Story -> SessionBinding(Story, "companion") -> session_id
LifecycleRun.session_id = 上述 session_id (1:1)
StoryStepActivationService: story -> binding -> session -> run -> task
CapabilityResolver: SessionOwnerCtx -> allowed_owner_types -> tool visibility
Frontend: storyStore.sessionsByStoryId -> /stories/{id}/sessions
```

Activity 模型已是唯一 lifecycle 模型（`step_states` 已移除），这是好消息。

## 重构策略

**不做兼容层**（预研期直接收敛到正确模型）。Agent Permission System 独立拆出，本次聚焦：

1. 引入 `LifecycleRunLink` 显式关联层
2. `LifecycleRun` 去除 `session_id` 的业务语义，添加 runtime session association
3. `StoryStepActivationService` 改为通过 run link 查 Story 的 active run
4. `CapabilityResolver` 解除对 `SessionOwnerType` 的独占依赖（但完整 AgentPermission 路径后续）
5. API 和前端切换到 Story/Run 主语

---

## Phase 1: Domain 层 — LifecycleRunLink + Entity 调整

### 1.1 新增 `LifecycleRunLink` 实体

位置: `crates/agentdash-domain/src/workflow/`

```rust
pub struct LifecycleRunLink {
    pub id: Uuid,
    pub run_id: Uuid,
    pub subject_kind: RunLinkSubjectKind,  // Story | Project | RoutineExecution | Task | External
    pub subject_id: Uuid,
    pub role: RunLinkRole,  // Source | Subject | ProjectionTarget | ControlScope | SpawnedBy
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

pub enum RunLinkSubjectKind { Story, Project, RoutineExecution, Task, External }
pub enum RunLinkRole { Source, Subject, ProjectionTarget, ControlScope, SpawnedBy }
```

### 1.2 修改 `LifecycleRun` 实体

- 将 `session_id: StorySessionId` 改为 `session_id: Option<String>`，语义从"Story root session"降级为"当前 run 的 runtime session（如果有）"
- 移除 `StorySessionId` 类型别名在 run 上的使用
- `new_activity()` 构造不再要求传入 session_id（session 在 attempt claim 时再绑定）

### 1.3 新增 Repository trait

```rust
pub trait LifecycleRunLinkRepository {
    async fn create(&self, link: &LifecycleRunLink) -> Result<()>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleRunLink>>;
    async fn list_by_subject(&self, kind: RunLinkSubjectKind, id: Uuid) -> Result<Vec<LifecycleRunLink>>;
    async fn list_by_subject_and_role(...) -> Result<Vec<LifecycleRunLink>>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}
```

### 1.4 调整 `LifecycleRunRepository`

- 移除/降级 `list_by_session()` 方法（保留为内部 debug 用途，不做 Story 业务查询入口）
- 新增 `list_by_ids(run_ids: &[Uuid])` 供 link 查询后批量加载

### 1.5 数据库 Migration

- 新表 `lifecycle_run_links`
- `lifecycle_runs.session_id` 改为 nullable
- 数据迁移脚本：为每条 existing run，通过 `session_id -> SessionBinding(Story) -> story_id` 创建 `LifecycleRunLink(subject=Story, role=Subject)`

涉及文件:
- [crates/agentdash-domain/src/workflow/entity.rs](crates/agentdash-domain/src/workflow/entity.rs)
- [crates/agentdash-domain/src/workflow/mod.rs](crates/agentdash-domain/src/workflow/mod.rs) (新模块 `run_link`)
- [crates/agentdash-infrastructure/src/persistence/postgres/](crates/agentdash-infrastructure/src/persistence/postgres/) (新 repo + migration)

---

## Phase 2: Application 层 — 服务改造

### 2.1 新增 `LifecycleRunLinkService`

位置: `crates/agentdash-application/src/workflow/`

```rust
impl LifecycleRunLinkService {
    pub async fn attach_subject(run_id, subject_kind, subject_id, role) -> Result<LifecycleRunLink>;
    pub async fn list_runs_for_story(story_id) -> Result<Vec<LifecycleRun>>;
    pub async fn active_run_for_story(story_id) -> Result<Option<LifecycleRun>>;
    pub async fn list_subjects_for_run(run_id) -> Result<Vec<LifecycleRunLink>>;
}
```

### 2.2 重构 `StoryStepActivationService`

当前链路（36处 session 引用）:
```
story_id -> SessionBinding(Story, "companion") -> session_id -> lifecycle_run_repo.list_by_session
```

目标链路:
```
story_id -> run_link_service.active_run_for_story(story_id) -> LifecycleRun
```

核心改动:
- `activate_story_step()` 不再通过 `find_story_session_id` 获取 companion session
- Task execution session 仍通过 `SessionBinding(Task, "execution")` 创建（这是 runtime association，合理保留）
- `LifecycleRunService::bind_session_and_activate_step` 中 session binding 保留为 runtime trace，但不再作为 run 查询入口

### 2.3 重构 LifecycleRun 创建路径

当前: `ensure_freeform_lifecycle_run(session_id)` — 从 session 创建 run
目标: `ensure_lifecycle_run_for_story(story_id, lifecycle_id)` — 从 Story 创建 run，同时创建 LifecycleRunLink

当前 session 创建仍可保留为 runtime 副作用（companion attempt 需要 session），但 run 的业务归属不再由 session 决定。

### 2.4 Routine Executor 调整

当前 `RoutineExecution.session_id` 直接记录。

调整:
- Routine 触发 LifecycleRun 时，创建 `LifecycleRunLink(subject=RoutineExecution, role=Source)`
- `RoutineExecution.session_id` 可降级为 `Option` 或只记录 runtime attempt session

涉及文件:
- [crates/agentdash-application/src/task/service.rs](crates/agentdash-application/src/task/service.rs) (StoryStepActivationService)
- [crates/agentdash-application/src/workflow/freeform.rs](crates/agentdash-application/src/workflow/freeform.rs)
- [crates/agentdash-application/src/routine/executor.rs](crates/agentdash-application/src/routine/executor.rs)
- 新文件 `crates/agentdash-application/src/workflow/run_link_service.rs`

---

## Phase 3: CapabilityResolver 过渡改造

Agent Permission System 是独立任务，但本次需要打通 Session demotion 的路径。

### 3.1 引入 `CapabilityContext` 替代单一 `SessionOwnerCtx`

```rust
pub struct CapabilityContext {
    pub owner_ctx: SessionOwnerCtx,       // 保留兼容，但不再是唯一来源
    pub lifecycle_run: Option<LifecycleRunRef>,
    pub run_links: Vec<LifecycleRunLink>,  // run 的业务关联
    // 后续: pub active_grants: Vec<AgentPermissionGrant>,
}
```

### 3.2 调整 `is_capability_visible`

- `allowed_owner_types` 保留为 fallback
- 新增 `allowed_by_run_context` 路径：如果 run link 指向 Story scope，Story-scoped capabilities 可见
- 这为后续 AgentPermission 插入留好 hook point

涉及文件:
- [crates/agentdash-spi/src/platform/tool_capability.rs](crates/agentdash-spi/src/platform/tool_capability.rs)
- [crates/agentdash-application/src/capability/resolver.rs](crates/agentdash-application/src/capability/resolver.rs)

---

## Phase 4: API 层重构

### 4.1 新增 Run-oriented API

```
GET  /stories/{story_id}/runs              # 替代 /stories/{id}/sessions 的业务查询
GET  /lifecycle-runs/{run_id}              # 已有，保留
GET  /lifecycle-runs/{run_id}/links        # 新增
POST /lifecycle-runs/{run_id}/links        # 新增 (attach subject)
GET  /lifecycle-runs/{run_id}/timeline     # 新增 (Activity attempt timeline)
```

### 4.2 降级 Session API

- `/stories/{id}/sessions` — 保留为 runtime/debug，不再是进入 Story 的主路径
- `/lifecycle-runs/by-session/{session_id}` — 标记 deprecated，内部重定向到 link 查询
- `/tasks/{id}/session` — 保留（Task execution session 是 runtime 关联，合理）

### 4.3 TS Contracts 更新

`crates/agentdash-contracts/src/workflow.rs` 中新增：
- `LifecycleRunLinkDto`
- `StoryRunsResponse`
- `RunTimelineResponse`

运行 `cargo run -p agentdash-contracts --bin generate_contracts_ts` 重新生成前端类型。

涉及文件:
- [crates/agentdash-api/src/routes.rs](crates/agentdash-api/src/routes.rs)
- [crates/agentdash-api/src/routes/stories.rs](crates/agentdash-api/src/routes/stories.rs)
- [crates/agentdash-api/src/routes/story_sessions.rs](crates/agentdash-api/src/routes/story_sessions.rs) (降级)
- [crates/agentdash-contracts/src/workflow.rs](crates/agentdash-contracts/src/workflow.rs)
- 新文件 `crates/agentdash-api/src/routes/story_runs.rs`

---

## Phase 5: 前端迁移

### 5.1 storyStore 调整

- `sessionsByStoryId` -> `runsByStoryId: Record<string, LifecycleRun[]>`
- `fetchStorySessions(storyId)` -> `fetchStoryRuns(storyId)` 调用新 API
- Story 详情页的"会话"概念替换为"运行记录"

### 5.2 services/story.ts 调整

- 移除 `createStorySession` / `fetchStorySessions` / `unbindStorySession` 的产品路径用途
- 新增 `fetchStoryRuns(storyId)` / `fetchRunTimeline(runId)`
- `fetchTaskSession` 保留（runtime 用途）

### 5.3 导航与页面调整

- Story 详情页：从展示 "Sessions" 列表改为展示 "Runs" + Activity timeline
- Task 操作：`startTaskExecution` 不变（API facade 不变），但内部不再依赖 companion session 存在
- 工作区面板 (workspace-panel): session 相关的标签类型保留为 runtime detail

### 5.4 Agent feature 调整

- `session-grouping.ts` / `session-filter.ts` / `session-relations.ts` 需要审计
- 这些如果是为了 Agent runtime session 列表展示，保留
- 如果是为了 Story 导航入口，迁移到 run-based 查询

涉及文件:
- [packages/app-web/src/stores/storyStore.ts](packages/app-web/src/stores/storyStore.ts)
- [packages/app-web/src/services/story.ts](packages/app-web/src/services/story.ts)
- [packages/app-web/src/features/agent/session-grouping.ts](packages/app-web/src/features/agent/session-grouping.ts)
- [packages/app-web/src/generated/workflow-contracts.ts](packages/app-web/src/generated/workflow-contracts.ts) (自动生成)

---

## Phase 6: 清理与验证

### 6.1 Spec 更新

- `.trellis/spec/backend/story-task-runtime.md` — 从 "Story-as-durable-session" 改为 "Story-as-thin-business-scope + LifecycleRunLink"
- 移除 "Story ↔ Story session 1:1" 不变量
- 明确 SessionBinding 仅作为 runtime/debug association

### 6.2 残留清理

- `LifecycleRun.session_id` 最终目标是移除或改名为 `runtime_session_id: Option<String>`
- `StorySessionId` 类型别名可以删除（改为普通 `String`）
- `SessionBinding` label `"companion"` 不再具有特殊地位
- `WorkflowBindingKind` 暂保留为 catalog filter，但 PR 注释标注后续 Agent Permission 完成后应审计替换

### 6.3 验证策略

```
cargo test -p agentdash-domain
cargo test -p agentdash-application
cargo test -p agentdash-api
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web test
```

E2E 场景:
- Story 创建 -> 不创建 session -> 通过 UI 启动 Task -> 自动创建 LifecycleRun + RunLink + runtime session
- Story 查询 related runs 通过 `/stories/{id}/runs`
- Task start/continue/cancel 不受影响

---

## 不在本次范围内

- Agent Permission System (Request/Grant/Policy/Compiler) — 独立任务
- WorkflowBindingKind 完整替换为 launch scope/subject requirements/capability contract — 需要 Agent Permission 就位后
- OwnedAgent 概念移除 — 依赖 Agent Permission 的 grant 查询
- StoryRun / WorkRun 产品层索引 — 需求不明确时不提前引入

---

## 风险与注意事项

- `StoryStepActivationService` 是改动最大的服务（36处 session 引用），需要逐个方法审查
- `CapabilityResolver` 的 `SessionOwnerCtx` 被多处消费（assembler, construction_planner, context_builder, step_activation），过渡期保留 owner_ctx 但新增 CapabilityContext 包装
- 前端 `session-shortcut-rows` 和 workspace-panel 中的 session 概念是否等价于 "runtime session detail" 需要逐文件判断
- Migration 需要处理历史数据：existing `lifecycle_runs` 通过 `session_id -> binding -> story_id` 回填 run link
