# Permission 系统收束维护提醒

## Goal

重新评估并收束当前 permission / approval 系统，明确权限、审批、companion capability grant、workflow approval、Task fanout policy 的事实源边界。

这个任务是后续维护提醒，不阻塞当前 Story / Task subject 模型清理。当前默认策略是 Task fanout 审批门保留但默认开放；本任务用于回头确认 permission 系统是否需要重构、删减或重新接入运行主链。

## Requirements

- 梳理现有 PermissionGrant、policy engine、companion capability grant、workflow approval、fanout policy 的代码路径和 spec 约束。
- 判断当前权限审批系统是否仍符合 AgentRun / LifecycleRun / SubjectRef 主线。
- 明确授权事实源：后续应以 PermissionGrant / policy projection 为准，companion 交互只作为申请、通知或 broker，不成为授权结果事实源。
- 明确 approval gate 的适用范围：哪些操作默认开放，哪些操作应由 project policy / workflow rule / permission grant 切换为需要审批。
- 检查前后端 contract 是否存在手写 DTO、JsonValue 绕过 generated contract、前端镜像权限规则等漂移。
- 输出一份后续清理建议，区分必须修复、可删减、可延期的权限能力。

## Acceptance Criteria

- [ ] 形成 permission / approval 现状梳理，列出主要事实源和调用入口。
- [ ] 明确 PermissionGrant、companion grant、workflow approval、Task fanout policy 的边界关系。
- [ ] 给出是否保留、重构或删除旧 permission / approval 能力的建议。
- [ ] 给出后续实现拆分建议，避免一次性重构影响 AgentRun / Lifecycle 主链。
- [ ] 更新必要 spec 或提出 spec 更新清单。

## Notes

- 轻量提醒任务，当前只需要 PRD。
- 相关背景来自 `06-16-story-task-subject-model-cleanup`：Task fanout 默认开放，但保留可配置审批门。
