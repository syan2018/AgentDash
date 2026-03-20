# Workflow 模块设计与主干集成说明

## 目标

把 AgentDash 中原本隐式存在于 prompt、task 目录和人工约定里的研发流程，上升为正式的平台能力：

- `WorkflowDefinition`：定义一条 workflow 是什么
- `WorkflowAssignment`：定义它被哪个 Project / Agent Role 采用
- `WorkflowRun`：定义某个真实目标对象当前正在跑哪一条 workflow

第一版聚焦 `Trellis Dev Workflow`，让它成为 AgentDash 内部第一条可持久化、可查询、可推进的正式 workflow。

## 分层结构

### Domain

位置：`crates/agentdash-domain/src/workflow/`

- `WorkflowDefinition`
  - workflow 元定义
  - 包含 key、target_kind、version、enabled、phases、record_policy
- `WorkflowAssignment`
  - project 与 workflow 的绑定关系
  - 包含 role、enabled、is_default
- `WorkflowRun`
  - 真实运行实例
  - 包含 target_kind、target_id、current_phase_key、phase_states、record_artifacts

当前 run 的核心领域行为：

- `activate_phase`
- `attach_session_binding`
- `complete_phase`
- `append_record_artifact`

这些行为负责守住 phase 顺序、session 绑定和记录产物追加的边界。

### Application

位置：`crates/agentdash-application/src/workflow/`

- `definition.rs`
  - 提供内置 workflow builder
  - 当前第一条内置流程是 `Trellis Dev Workflow`
- `catalog.rs`
  - `WorkflowCatalogService`
  - 负责 definition upsert、assignment 写入、default 收束
- `run.rs`
  - `WorkflowRunService`
  - 负责 run 创建、phase 激活、phase 完成、record artifact 写入

这一层不关心 HTTP，也不依赖具体 SQLite 实现，只依赖 repository trait。

### Infrastructure

位置：`crates/agentdash-infrastructure/src/persistence/sqlite/workflow_repository.rs`

- `SqliteWorkflowRepository`
  - 同时实现：
    - `WorkflowDefinitionRepository`
    - `WorkflowAssignmentRepository`
    - `WorkflowRunRepository`
- 当前采用 JSON 序列化字段保存复杂结构：
  - `phases`
  - `record_policy`
  - `phase_states`
  - `record_artifacts`

这是符合当前项目预研阶段节奏的实现：先稳定领域契约，再逐步细化 schema。

### API

位置：`crates/agentdash-api/src/routes/workflows.rs`

已经对外提供：

- `GET /api/workflows`
- `POST /api/workflows/bootstrap/trellis-dev`
- `GET /api/projects/{id}/workflow-assignments`
- `POST /api/projects/{id}/workflow-assignments`
- `POST /api/workflows/runs`
- `GET /api/workflow-runs/{id}`
- `GET /api/workflow-runs/targets/{target_kind}/{target_id}`
- `POST /api/workflow-runs/{id}/phases/{phase_key}/activate`
- `POST /api/workflow-runs/{id}/phases/{phase_key}/complete`

## 当前与项目主干的集成方式

### 1. 与 AppState 的集成

`AppState` 已经把 workflow repository 正式注入主干：

- `workflow_definition_repo`
- `workflow_assignment_repo`
- `workflow_run_repo`

初始化发生在应用启动时，与 project / story / task 等现有仓储并列。

### 2. 与 Project / Story / Task 主模型的关系

当前 workflow 不替代现有主业务模型，而是作为横切编排层：

- `Project`
  - 决定采用哪些 workflow
  - 通过 `WorkflowAssignment` 与 workflow 发生关系
- `Story / Task / Project`
  - 都可以成为 `WorkflowRun` 的 target
  - 由 `WorkflowTargetKind + target_id` 统一表达

这意味着 workflow 是“作用于主实体的流程层”，不是新的业务容器层。

### 3. 与 SessionBinding 的关系

对于需要会话驱动的 phase，例如 `Implement` / `Check`：

- `WorkflowRunService.activate_phase` 会校验该 phase 是否需要 session
- 如果需要，则必须传入 `session_binding_id`
- `WorkflowRun` 内部将这个 binding 记录在对应 `WorkflowPhaseState.session_binding_id`

因此当前集成关系是：

`WorkflowRun.phase -> SessionBinding -> ExecutorHub session`

workflow 自己不直接持有 executor session，而是复用项目已有的 `SessionBinding` 统一关系模型。

### 4. 与 Trellis 工作流语义的关系

`Trellis Dev Workflow` 当前被建模为四阶段：

1. `Start`
2. `Implement`
3. `Check`
4. `Record`

其中：

- `Start`
  - 不强制 session
  - 负责读取 workflow / spec / PRD 并准备上下文
- `Implement`
  - 需要 session
  - 对接 implement context 与开发执行
- `Check`
  - 需要 session
  - 对接 review / checklist / 质量确认
- `Record`
  - 不强制 session
  - 负责沉淀 summary、journal suggestion、archive suggestion

这四阶段正好对应 Trellis 现有研发习惯，因此它不是凭空新造流程，而是把已有研发路径平台化。

## 当前模块边界

### 已完成

- workflow 领域模型
- workflow SQLite 持久化
- workflow application service
- workflow API 路由
- Trellis 内置 workflow bootstrap
- run / phase / record artifact 的最小 runtime
- 前端 workflow service / store / 类型映射
- Project 详情页 workflow 配置面板
- Task 抽屉 workflow run / phase 推进面板
- phase 与 `SessionBinding` 的前端串联
- Playwright 真实 UI 闭环验证

### 还未完成

- story 级 / project 级 workflow run 的专门可视化
- phase 与真实 session 创建动作的自动触发联动
- `Record` 阶段自动 journal / archive 执行动作
- 更通用的 workflow designer / 多 workflow 管理能力

## 当前前端接入位置

### Project 侧

位置：`frontend/src/features/project/project-selector.tsx`

- `ProjectDetailDrawer` 新增 `Workflow` tab
- 使用 `ProjectWorkflowPanel`
- 当前支持：
  - 注册内置 `Trellis Dev Workflow`
  - 查看 workflow definition
  - 把某个 task-target workflow 设为当前 Project 默认 Task 流程

### Task 侧

位置：`frontend/src/features/task/task-drawer.tsx`

- `TaskDrawer` 新增 `Workflow 执行` 面板
- 使用 `TaskWorkflowPanel`
- 当前支持：
  - 查询当前 Task 的 workflow runs
  - 基于 Project 默认 assignment 启动 run
  - 激活 / 完成当前 phase
  - 展示 phase 状态和 `record_artifacts`

### SessionBinding 串联方式

位置：`frontend/src/features/workflow/task-workflow-panel.tsx`

- 当前不会直接猜测 `task.session_id == session_binding_id`
- 前端会先通过：
  - `GET /api/sessions/{id}/bindings`
- 再解析当前 task 对应的 `SessionBindingOwner`
- 当 phase `requires_session=true` 时，activate 会把该 binding id 传回：
  - `POST /api/workflow-runs/{id}/phases/{phase_key}/activate`

因此当前主干集成关系已经变成：

`TaskDrawer -> TaskWorkflowPanel -> SessionBinding -> WorkflowRun phase`

## 为什么这样设计

### 不把 workflow 直接塞回 SessionComposition

`session_composition` 只描述一次会话的上下文和 persona 约束，不负责：

- workflow 生命周期
- phase 状态
- assignment/default 选择
- run 记录

所以 workflow 应该是比 `session_composition` 更高一层的流程编排对象。

### 不把 workflow 直接嵌进 Project / Story / Task 实体

如果直接把 workflow 状态写进这些实体，会导致：

- Project / Story / Task 承担过多流程状态
- 多 workflow 并存时难以扩展
- 无法复用统一的 run / phase / record 契约

因此选择独立聚合 `WorkflowRun`，用 target 引用主实体。

### 为什么先做 builtin workflow，而不是开放任意用户定义

当前项目还在预研期，最值钱的是先把：

- 领域边界
- phase 契约
- runtime 状态
- 主干接线

这些核心稳定下来。

所以第一版先固化 `Trellis Dev Workflow` 这条真实流程，再考虑更通用的 workflow designer。
