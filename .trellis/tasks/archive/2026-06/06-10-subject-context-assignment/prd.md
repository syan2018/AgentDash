# 建立 SubjectContext 动态分配到 AgentFrame 的通用模型

## Goal

把 Story/Task/Project 上下文从 hard-coded owner composer 中抽离，改为由 `SubjectRef` / `LifecycleSubjectAssociation` 动态解析成 context contribution，并在 ProjectAgent launch/message 或 frame construction 时写入 `AgentFrame` surface。

## Requirements

1. 新增或收束 application 层 `SubjectContextAssignment` 模型，输入为 `SubjectRef`，输出为 `Vec<Contribution>`、workspace 解析结果和 capability scope。
2. Story context 由 Story + Project + workspace declared sources 组成，不需要 Story owner session。
3. Task context 由 Task binding + parent Story context + Task workspace/default workspace declared sources 组成，不需要 Task Agent 或 `story_step`。
4. ProjectAgent session start 应能接收可选 `subject_ref`，以便把 Story/Task context 动态 assign 到 AgentFrame；当前 ProjectAgent UI 不新增 subject 选择器。
5. 未来 Story “快速创建会话”入口应作为薄 facade 调用 ProjectAgent session start + `subject_ref=story`，不得引入 Story Agent 或 Story owner session。
6. Assignment 只构建 context/capability/VFS surface，不创建 runtime session、不创建 lifecycle run、不修改业务状态。
7. 设计应复用现有 `Contribution` / `SessionContextBundle` / `AgentFrameBuilder`，避免另起一套 context format。

## Acceptance Criteria

- [ ] 有明确的 `SubjectRef -> SubjectContextAssignment -> AgentFrame surface` 数据流。
- [ ] Story/Task context contribution 不再需要 `OwnerScope::Story` 或 `StoryStepSpec`。
- [ ] ProjectAgent launch/message 能在不引入 Story Agent / Task Agent 的情况下获得 Story/Task 上下文。
- [ ] ProjectAgent UI 不新增 subject 选择器；Story 快速创建会话被定义为未来薄 facade 方向。
- [ ] Task assignment 覆盖 Task 自身字段、parent Story、Project、Workspace、declared sources。
- [ ] 相关 spec 更新说明 Story/Task 是 context profile / subject，不是 agent owner。

## Dependencies

- 本任务是 backend hard-cut 的设计前置；`06-10-story-task-agent-command-hard-cut` 删除 `composer_story` / `compose_story_step` 前应使用本任务模型替代需要保留的上下文注入能力。
