# 硬切 Story/Task Agent command 与 composer 残留

## Goal

删除 Story/Task 专用 agent command 入口和 hard-coded frame construction 分叉，让 backend 只保留 ProjectAgent、lifecycle node、companion 等通用 runtime launch 路径。

## Requirements

1. 删除 Task command API 和 application command 方法：start、continue、cancel。
2. 删除 `TaskLaunchSource` / `TaskLaunchPhase` / `LaunchCommand::task_service_input`。
3. 删除 `StoryStepSpec` / `StoryStepPhase` / `compose_story_step*` 和 `composer_task`。
4. 删除 Story launch API 和 `StoryLifecycleLaunchService`；未来 Story 快速创建会话另行设计为 ProjectAgent session start + `subject_ref=story` 的薄 facade，不复用旧 `/stories/{id}/launch` 语义。
5. 删除 `composer_story` 和 classifier 中只为 Story owner session 服务的 branch。
6. 保留 read-only SubjectExecution/Lifecycle projection 查询。
7. 不破坏 ProjectAgent launch/message、lifecycle node launch、companion launch。

## Acceptance Criteria

- [ ] `rg` 找不到 `compose_story_step`、`StoryStepSpec`、`TaskLaunchSource`、`task_service_input`。
- [ ] `rg` 找不到 `/tasks/{id}/start`、`/tasks/{id}/continue`、`/stories/{id}/launch`。
- [ ] `FrameConstructionService` 不再通过 Story/Task owner branch 组装 frame surface。
- [ ] ProjectAgent launch/message 仍有可用 frame surface。
- [ ] SubjectExecution read APIs 仍可查询 Story/Task projection。

## Dependencies

- 依赖 `06-10-subject-context-assignment` 的目标模型。若实际删除 `composer_story` 前发现 ProjectAgent 需要 Story/Task context，应先接入 SubjectContext assignment。
- Story 快速创建会话是后续产品入口，不阻塞本任务删除旧 Story Agent route/service。
