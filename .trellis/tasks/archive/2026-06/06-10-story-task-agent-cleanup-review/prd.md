# 评估并清理 Story/Task Agent 残留链路

## Goal

把 Story/Task Agent 残留链路清理成可并行执行的任务树：删除 Story/Task 专用 agent command / composer，同时建立 Story/Task 上下文按 `SubjectRef` 动态分配到 ProjectAgent / AgentFrame 的通用模型，避免继续用硬编码 owner path 维护运行时上下文。

## Background

当前代码里仍存在一组 Story/Task 专用 agent/runtime 入口：

- Task API 暴露 `/tasks/{id}/start`、`/tasks/{id}/continue`、`/tasks/{id}/cancel`，前端 `TaskSubjectExecutionPanel` 仍提供启动/继续/取消按钮。
- Task 后端路径创建 `SubjectExecutionIntent(subject_ref=task)` 并经 `LifecycleDispatchService` 创建 run / agent / frame / runtime session，但 `TaskExecutionCommand.prompt`、`executor_config`、`identity` 没有进入实际 launch；`LaunchCommand::task_service_input()` 目前只有定义没有调用。
- `story_step` / `StoryStepSpec` / `compose_story_step` 本质是 Task subject frame construction 路径，不是 Story step 或 workflow step 的权威模型。
- Story API 暴露 `/stories/{id}/launch`，并通过 `is_default_for_story` 的 `ProjectAgent` 启动 `AgentLaunchIntent(subject_ref=story)`；实际特殊性主要是默认 ProjectAgent 选择、Story subject_ref 和 Story owner context。
- ProjectAgent 仍暴露 `is_default_for_story` / `is_default_for_task`，容易把 Story/Task 误表达为 agent owner。
- Context builder 已有 `Contribution` / `SessionContextBundle` reducer，Story/Task context 也已有 contributor 雏形；问题是这些 contributor 仍挂在 hard-coded owner composer 或 task composer 上，没有成为 `SubjectRef` 驱动的通用 assignment。

项目当前目标模型已经收束为 `ProjectAgent + LifecycleRun + LifecycleAgent + AgentFrame + RuntimeSessionExecutionAnchor`。Story / Task 应主要作为业务 subject、上下文 profile 和只读 projection，而不是拥有独立 agent 启动链路。

## Child Tasks

1. `06-10-subject-context-assignment`：建立 `SubjectContext` 动态分配模型。优先执行设计，给后续清理提供替代路径。
2. `06-10-story-task-agent-command-hard-cut`：删除 backend Story/Task Agent command 和 hard-coded composer 残留。依赖 child 1 的目标模型，但可并行做清单和非重叠删除。
3. `06-10-story-task-agent-frontend-contract-cleanup`：删除前端、contracts、permission/capability 暴露。依赖 child 1/2 的最终 API shape。

## Requirements

1. 审计 Story/Task 专用 runtime/agent 入口，给出保留、删除、改义三类清单。
2. 明确 Story / Task 作为业务 subject 的保留边界：业务字段、上下文声明、只读执行投影、相关 Lifecycle 查询可以保留；command 型 agent 启动入口默认删除。
3. 建立动态上下文 assignment 模型：
   - 输入为 `SubjectRef` 或 `LifecycleSubjectAssociation`，而不是 hard-coded `OwnerScope::Story` / `StoryStepSpec`。
   - 输出为可合并的 `Contribution` / `SessionContextBundle` / `AgentFrame` surface。
   - 支持 Story、Task、Project，且 Task context 应组合 Task binding + parent Story context + workspace sources。
   - Assignment 发生在底层 ProjectAgent session start/message 或 frame construction 中，不引入 Story Agent / Task Agent。
   - 底层允许携带可选 `subject_ref`；当前 ProjectAgent UI 不新增 subject 选择器。
   - 后续 Story 可设计“快速创建会话”入口，该入口必须是 ProjectAgent session start + `subject_ref=story` 的薄 facade，而不是 Story Agent。
4. 评估并规划删除 Task Agent 链路：
   - `/tasks/{id}/start`
   - `/tasks/{id}/continue`
   - `/tasks/{id}/cancel`
   - `StoryActivityActivationService` 的 command 路径
   - `TaskExecutionCommand` / `TaskExecutionResult` / command DTO
   - `TaskLaunchSource` / `TaskLaunchPhase` / `LaunchCommand::task_service_input`
   - `composer_task`
   - `StoryStepSpec` / `StoryStepPhase` / `compose_story_step*`
   - 前端 Task 执行按钮与相关 store/service command 方法
   - `task_management::start_task` 等直接启动 Task Agent 的 capability/grant 路径
5. 评估并规划删除或改义 Story Agent 链路：
   - `/stories/{id}/launch`
   - `StoryLifecycleLaunchService`
   - `composer_story`
   - `OwnerScope::Story` 作为 runtime owner 的使用
   - `is_default_for_story` 作为 ProjectAgent 默认绑定语义
6. 更新 docs/spec/contracts/frontend，使 repo-facing 文案不再暗示 Story Agent / Task Agent 是独立运行时模型。
7. 触及数据库或 contracts 时，使用 forward migration 与现有 contracts/type generation 流程，不保留兼容 fallback。

## Acceptance Criteria

- [ ] 父任务能清楚追踪三个 child task 的交付边界、依赖和最终合流检查。
- [ ] 明确最终目标模型：ProjectAgent 是唯一可运行 agent 配置；Story/Task 是 business subject、context profile 与 projection，不拥有专用 runtime command 入口。
- [ ] `SubjectContext` assignment 设计能替代 `OwnerScope::Story` / `StoryStepSpec` 等硬编码路径。
- [ ] 底层 ProjectAgent session start/message 可承载可选 `subject_ref`，但 ProjectAgent UI 不新增 subject 选择器。
- [ ] Story 快速创建会话被记录为后续薄 facade 方向，不复用旧 Story Agent route/service。
- [ ] 保留路径只包含业务 subject、上下文声明、Lifecycle/SubjectExecution 只读查询和 ProjectAgent 通用 launch/message。
- [ ] 删除或改义后的 API / generated contracts / frontend 入口没有 Story Agent / Task Agent 启动按钮、方法或 grant path 残留。
- [ ] 所有涉及 schema / contracts / frontend generated type 的变更都有对应验证命令。
- [ ] 最终残留扫描覆盖 `compose_story_step`、`StoryStepSpec`、`TaskLaunchSource`、Story/Task launch routes、`is_default_for_story`、`is_default_for_task`、`task_management::start_task`。

## Notes

- 本任务是 parent/orchestration task，保持 planning，等 child artifacts 评审后再逐个 `task.py start`。
- 不做兼容保留；项目处于预研期，允许硬切旧入口。
- 不把 Story / Task 业务实体本身删除；本任务目标是删除/收束它们作为 agent owner/runtime command 入口的残留链路。
