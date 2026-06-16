# Story/Task subject 模型清理执行计划

## Phase 1: 规划确认

- [x] 创建 Trellis task。
- [x] 写入 PRD，记录目标业务语义和验收标准。
- [x] 写入 design，记录 Story / Story-subject AgentRun / TaskTodo / Lifecycle 的目标边界。
- [x] 写入 implement，记录后续执行顺序。
- [x] 与用户确认锚定 `Task` 命名，不迁移为 Todo / Work item。
- [x] 与用户确认 Task 不归属 Story domain，而是任意 AgentRun 可创建和管理的通用计划项。
- [x] 与用户确认 Task 支持 AgentRun 自执行、快速 assign 给 Companion subagent、dynamic orchestration 批量生成并扇出分配。
- [x] 与用户确认 fanout 审批门保留但默认开放，不阻塞默认一键扇出。
- [ ] 与用户确认 Task 状态集合。
- [ ] 与用户确认 Story projection 关联规则的最小集合。
- [ ] 记录 permission / approval 系统后续研究收束任务候选。

## Phase 2: Spec 收口

- [ ] 更新 `.trellis/spec/backend/story-task-runtime.md`，将 Task 定位改为通用 AgentRun-created work item，并将 Story 侧定义为 projection。
- [ ] 更新相关 frontend / cross-layer contract spec，明确 Story subject run、Task linked runs、Story Task projection 的 UI/API 语言。
- [ ] 将 `StoryAgent` 表述收敛为 Story subject AgentRun，不作为独立模型写入长期 spec。
- [ ] 补充 Task assignment / companion subagent / dynamic orchestration fanout 的 subject association 和 command 边界。
- [ ] 补充 fanout policy 口径：默认开放，允许 workflow / project policy / permission grant 后续切换为审批模式。

## Phase 3: 后端模型清理

- [ ] 调整 Task status enum，从执行状态迁移为 Todo 计划状态。
- [ ] 将 Task 从 `stories.tasks JSONB` 迁出为通用 Task repository / table。
- [ ] 增加 Task 与 Story / AgentRun / Lifecycle subject 的 link / association 查询路径。
- [ ] 设计 Task assignment intent / execution link，用于自执行、Companion assign 和 orchestration fanout。
- [ ] 增加 Task batch plan / create / assign / fanout 的应用层命令边界。
- [ ] 为 fanout command 预留 approval policy 参数或 policy resolver 接口，但默认实现直接放行。
- [ ] 移除 Task artifacts 的事实源职责，改为 linked execution projection。
- [ ] 评估 TaskDispatchPreference 拆分为 context sources / launch hint / dispatch command 参数。
- [ ] 统一 Task / Story execution read model 到 SubjectExecutionView。
- [ ] 收口或删除 `/tasks/{id}/execution` 专属 DTO / route。
- [ ] 审查 StoryMcpServer / TaskMcpServer，将实体专属状态推进工具迁向 subject-scoped capability。

## Phase 4: 前端体验清理

- [ ] Story 页面保留 brief、人工状态、context、Story Task projection、linked runs。
- [ ] TaskDrawer 保留 Task 命名，改为计划项编辑与 linked runs 查看。
- [ ] Task UI 支持自执行、Assign to Companion、加入 orchestration fanout plan 三种入口的最小表达。
- [ ] TaskSubjectExecutionPanel 使用 SubjectExecutionView 或 linked runs projection。
- [ ] StoryBoard / bulk / quick jump 等 PM 产品化表面按当前目标裁剪或隐藏。
- [ ] 更新前端文案，避免把 Story subject run 表达为独立 StoryAgent 实体。
- [ ] Fanout UI 默认显示可直接执行的一键动作，同时预留 policy 要求审批时的 review / approve 状态入口。

## Phase 5: 验证

- [ ] `cargo check --workspace`
- [ ] `pnpm typecheck`
- [ ] `pnpm test -- --run`
- [ ] 针对 migration 增加或更新检查脚本。
- [ ] 用 Story 页面手动验证：创建 Story、编辑人工状态、创建 Task、启动 Story subject run、查看 Story Task projection 和 linked run。

## 风险文件

- `crates/agentdash-domain/src/task/value_objects.rs`
- `crates/agentdash-domain/src/task/entity.rs`
- `crates/agentdash-domain/src/story/repository.rs`
- `crates/agentdash-domain/src/story/entity.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/story_repository.rs`
- `crates/agentdash-api/src/routes/stories.rs`
- `crates/agentdash-api/src/routes/task_execution.rs`
- `crates/agentdash-application/src/workflow/subject_context_assignment.rs`
- `crates/agentdash-application/src/task/service.rs`
- `crates/agentdash-application/src/workflow/dispatch_service.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`
- `packages/app-web/src/pages/StoryPage.tsx`
- `packages/app-web/src/features/task/task-drawer.tsx`
- `packages/app-web/src/features/task/task-subject-execution-panel.tsx`

## Rollback 点

- 后端 enum / DTO / Task repository migration 改动应单独提交，便于回滚。
- API route 删除或替换应先确认前端引用和 generated contract 使用点。
- UI 瘦身应避免与视觉重构混在同一提交。

## Follow-up 候选

- Permission / approval 系统收束研究：重新评估 PermissionGrant、companion capability grant、workflow approval、fanout policy 的事实源边界。
