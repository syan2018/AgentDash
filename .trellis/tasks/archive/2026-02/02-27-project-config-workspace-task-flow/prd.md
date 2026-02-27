# Project 配置增强与 Story 下 Task 创建流程

## Goal

在现有 `Project -> Workspace -> Story -> Task` 领域模型基础上，补齐配置能力与创建闭环：
- Project 侧支持更完整的配置维护（默认 Agent / 默认 Workspace / Agent 预设）
- Workspace 侧支持更可靠的本地目录选择与 Git 信息识别
- Story 侧支持创建 Task，并在创建时绑定 Agent 与 Workspace

目标是让用户在前端完成从 Project 配置到 Task 创建的关键路径，且契约清晰、可验证、可回滚。

## What I already know

- 已有 Project/Workspace 基础能力：
  - 已支持 Project 创建/选择，Workspace 按 Project 列表与创建
  - 已支持 Story 按 `project_id` 查询与创建，Task 按 Story 查询
- 当前缺口：
  - Task 创建链路缺失（后端仅有 `GET /stories/{id}/tasks`，无 `POST`）
  - `storyStore` 无 `createTask` action，`StoryDrawer` 无 Task 创建入口
  - Workspace 本地目录选择仅通过 `input[webkitdirectory]` 推断顶层目录名，未识别 Git 信息
  - Project 配置仅在创建时可传，缺少配置编辑与校验闭环
- 可参考模式：
  - `vibe-kanban` 的看板项创建流程先提交主实体，再补充关联关系并回到详情，适合借鉴到 Task 创建流程。

## Requirements

- R1. Project 配置增强
  - 支持编辑并保存 `Project.config`：
    - `default_agent_type`
    - `default_workspace_id`
    - `agent_presets[]`
  - 前端在创建 Task 时优先读取 Project 默认配置作为初始值。

- R2. Workspace 多实例与本地目录选择增强
  - 支持同一 Project 下新增多个 Workspace（现有能力保留并完善交互）。
  - 支持从本地选择目录后填写 `container_ref`。
  - 选择目录后可触发 Git 信息识别，自动回填（仓库、分支、提交）。
  - Git 识别失败时允许用户手动修正，不阻塞创建。

- R3. Story 下创建 Task
  - 在 Story 详情中提供“创建 Task”入口。
  - 创建表单支持填写：
    - `title`（必填）
    - `description`（可选）
    - `workspace_id`（可选，但推荐）
    - `agent_binding.agent_type` / `preset_name`（至少二选一策略，见验证矩阵）
  - 成功后任务出现在当前 Story 任务列表，并可直接打开 Task Drawer。

- R4. 参考 vibe-kanban 创建逻辑
  - 借鉴“创建模式与编辑模式分离、提交态防抖、成功后跳转/打开详情”的交互结构。
  - 保持当前 AgentDash 简化实现，不引入非必要复杂状态机。

- R5. 跨层契约一致性
  - 前后端字段统一使用 snake_case 契约，前端 Store 做明确映射。
  - 创建/更新失败有可见错误提示，不静默吞错。

## Acceptance Criteria

- [ ] 在 Project 设置中可编辑并保存 `default_agent_type`、`default_workspace_id`、`agent_presets`
- [ ] 同一 Project 下可新增并展示多个 Workspace，列表隔离按 `project_id` 生效
- [ ] 选择本地目录后可触发 Git 识别并回填 `git_config`（失败时可手动编辑后继续）
- [ ] Story Drawer 内可创建 Task，支持选择 Workspace 与 Agent/预设
- [ ] 新建 Task 后立即出现在任务列表，可打开详情，显示绑定信息
- [ ] 后端提供 Task 创建 API，包含参数校验与错误返回
- [ ] 前后端 lint/typecheck 通过（`cargo check` + 前端 typecheck/lint）
- [ ] 现有 Story 列表、Task 查询、Workspace 查询行为无回归

## Definition of Done

- [ ] 新增/修改接口有最小可运行测试覆盖（至少 happy path + 关键 bad case）
- [ ] Store 映射、表单提交、错误处理路径可手工验证
- [ ] 关键设计决策记录在本 PRD（ADR-lite）
- [ ] 若新增跨层契约，相关 spec/notes 得到更新

## Technical Approach

### 1) API 与数据契约

- A. Task 创建接口（新增）
  - `POST /api/stories/{id}/tasks`
  - Request:
    - `title: string` (required)
    - `description?: string`
    - `workspace_id?: string | null`
    - `agent_binding?: { agent_type?: string | null, preset_name?: string | null, agent_pid?: string | null }`
  - Response:
    - `Task`

- B. Git 信息识别接口（新增）
  - `POST /api/workspaces/detect-git`
  - Request:
    - `container_ref: string`
  - Response:
    - `{ is_git_repo: boolean, source_repo?: string, branch?: string, commit_hash?: string }`
  - 说明：
    - 若路径不是 Git 仓库，返回 `is_git_repo=false`，前端继续允许创建。

- C. Project 配置更新（复用现有）
  - `PUT /api/projects/{id}` 扩展前端调用与表单映射。

### 2) 前端实现分层

- Store 层：
  - `projectStore`：新增 `updateProjectConfig(...)`
  - `workspaceStore`：新增 `detectGitInfo(containerRef)`
  - `storyStore`：新增 `createTask(storyId, payload)`

- UI 层：
  - Project 区域新增配置入口（可内联 panel 或 dialog）
  - Workspace 创建面板新增“识别 Git 信息”动作与结果展示
  - Story Drawer 的 tasks tab 新增 Task 创建表单
  - 创建成功后默认打开新 Task（借鉴 vibe-kanban 的“创建后聚焦新实体”）

### 3) 后端实现分层

- `crates/agentdash-api/src/routes/stories.rs`
  - 新增 `create_task` handler
- `crates/agentdash-api/src/routes/workspaces.rs`
  - 新增 `detect_git` handler
- `crates/agentdash-api/src/routes.rs`
  - 注册新增路由
- Repository 层复用 `TaskRepository::create`；必要时补充查询用于校验 story/workspace 关联

### 4) Validation & Error Matrix

- Task 创建：
  - `story_id` 非法 -> `400 BadRequest`
  - story 不存在 -> `404 NotFound`
  - `title` 为空 -> `400 BadRequest`
  - `workspace_id` 非法 -> `400 BadRequest`
  - workspace 不存在 -> `404 NotFound`
  - workspace 与 story 不同 project -> `409 Conflict`
  - `agent_binding` 缺失且无项目默认 agent -> `422 UnprocessableEntity`

- Git 识别：
  - `container_ref` 为空 -> `400 BadRequest`
  - 路径不可访问 -> `400 BadRequest`（含错误提示）
  - 非 Git 目录 -> `200` + `is_git_repo=false`
  - 识别异常 -> `500`（前端展示可回退手填）

### 5) Good / Base / Bad Cases

- Good
  - 用户在 Story 下创建 Task，选择 Workspace + AgentPreset，创建后列表立即可见并可打开详情。
- Base
  - 用户仅填标题创建 Task（使用 Project 默认 agent），不绑定 workspace 也可创建。
- Bad
  - 用户传入其他 Project 的 workspace_id，后端拒绝并返回冲突错误，前端提示后不污染本地状态。

## Decision (ADR-lite)

- Context
  - 需求同时涉及 Project 配置、Workspace 路径/Git、Story->Task 创建，属于跨层改动。
  - 需要在“尽快可用”与“避免重复重构”之间平衡。

- Decision
  - 采用“最小闭环优先”：
    - 先打通 Task 创建核心链路（API + Store + Story Drawer 表单）
    - 同步补齐 Workspace Git 识别接口与前端入口
    - Project 配置先支持可编辑字段与默认值透传，不在本期引入复杂 preset 管理器

- Consequences
  - 优点：上线路径短，用户能立刻完成核心操作。
  - 代价：高级 preset 生命周期管理（重命名、迁移、批量管理）延后。

## Out of Scope

- 不引入新的全局状态库或表单库
- 不实现 Agent 在线探测/实时能力校验
- 不实现 Workspace 自动创建 Git Worktree 分支策略编排
- 不改造 Session/Execution 流程

## Implementation Plan (small PRs)

- PR1: 后端契约与接口
  - 新增 Task 创建与 Git 识别路由
  - 完成参数校验、错误矩阵、最小测试

- PR2: 前端 Store 与 API 对接
  - 新增 `createTask` / `detectGitInfo` / `updateProjectConfig`
  - 完成映射、错误处理、状态更新策略

- PR3: 前端交互闭环
  - Project 配置面板
  - Workspace 创建面板 Git 识别
  - Story Drawer Task 创建表单与创建后聚焦

- PR4: 回归与文档
  - 手工回归脚本、边界场景验证
  - 更新相关 spec/notes

## Technical Notes

- 主要参考文件：
  - `frontend/src/stores/projectStore.ts`
  - `frontend/src/stores/workspaceStore.ts`
  - `frontend/src/stores/storyStore.ts`
  - `frontend/src/features/workspace/workspace-list.tsx`
  - `frontend/src/features/story/story-drawer.tsx`
  - `crates/agentdash-api/src/routes/stories.rs`
  - `crates/agentdash-api/src/routes/workspaces.rs`
  - `third_party/vibe-kanban/packages/web-core/src/pages/kanban/KanbanIssuePanelContainer.tsx`

- 现状差距结论（用于实施阶段核对）：
  - 已有：Project/Workspace/Story 创建与查询
  - 缺失：Task 创建、Git 识别、Project 配置编辑闭环
