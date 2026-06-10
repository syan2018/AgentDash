# 收束 Story/Task Agent 前端与契约暴露

## Goal

删除前端和 contracts 中把 Story/Task 表达为 agent owner 或 command launch 入口的残留，让 UI 只通过 ProjectAgent 启动 agent，通过 SubjectExecution/Lifecycle projection 查看 Story/Task 运行事实。

## Requirements

1. 删除 Task 执行按钮、prompt box、start/continue/cancel store/service 方法。
2. 删除 Story launch route 的 generated contract / frontend service 使用。
3. 删除 ProjectAgent 默认 Story/Task toggles，除非被明确改义为 context assignment preset。
4. ProjectAgent UI 不新增 subject 选择器；未来 Story 快速创建会话入口另行设计，并作为 ProjectAgent session start + `subject_ref=story` 的薄 facade。
5. 删除 `task_management::start_task` 等 permission/capability 文案和测试残留。
6. 保留 Task/Story 页面中的只读 SubjectExecution projection、run/session trace 导航。
7. 更新 generated contracts 和 frontend type aliases。

## Acceptance Criteria

- [ ] 前端没有 Story Agent / Task Agent 启动按钮或 service 方法。
- [ ] Generated contracts 不暴露 Story/Task launch command DTO。
- [ ] ProjectAgent UI 不再配置 `is_default_for_story` / `is_default_for_task`，除非后端保留为新语义。
- [ ] ProjectAgent UI 不新增 subject selector；Story 快速创建会话只作为后续入口方向记录。
- [ ] Story/Task 页面仍能展示业务数据和 execution projection。

## Dependencies

- 依赖 `06-10-story-task-agent-command-hard-cut` 的 API 删除结果。
- 若 `06-10-subject-context-assignment` 添加 ProjectAgent subject launch 参数，当前 ProjectAgent 入口不消费该参数；未来 Story 快速创建会话可以消费该参数，但不得恢复 Story/Task 专用启动按钮。
