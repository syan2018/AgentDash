# Workspace 声明式来源 Local 解析下沉

## Goal

把 Story / Task 中依赖 workspace 物理文件的声明式上下文来源，正式改造为通过目标 local backend 解析与回传，彻底移除 cloud 对 `Workspace.container_ref` 的直接解引用。

## Background

在本轮边界重构里，为了先阻断 cloud 直接读本地目录的行为，已经做了临时收口：

- `task_agent_context` 和 `acp_sessions` 不再直接用 `container_ref` 作为 `working_dir`
- `file` / `project_snapshot` 类声明式来源在 cloud 侧被显式跳过，并注入提醒说明

这能保证边界正确，但还不是完整功能态。后续需要把这些来源真正 relay 到目标 local backend 执行。

## Requirements

- 为 workspace file / project snapshot 类来源建立 local backend 解析链路
- cloud 只负责路由、聚合和错误包装，不直接读取 workspace 目录
- 解析结果仍复用现有 `ContextFragment` / `resolve_declared_sources` 契约，尽量减少上层改动
- Story 会话与 Task 执行两条路径都能使用同一套下沉后的能力
- 失败场景要区分 backend 不在线、路径不存在、文件过大、非文本等错误

## Acceptance Criteria

- [ ] `ContextSourceKind::File` 在绑定 workspace 时可通过目标 backend 成功注入内容
- [ ] `ContextSourceKind::ProjectSnapshot` 可通过目标 backend 生成目录快照
- [ ] cloud 正式路径中不再使用 `std::fs` 直接读取 workspace 文件来源
- [ ] Story 会话和 Task 执行都能消费同一能力
- [ ] 原先“已跳过”的临时说明可以移除或仅保留为异常分支说明

## Technical Notes

- 尽量优先扩展现有 relay 协议，而不是在 API 层单独拼装一套只给 injection 用的旁路
- 可以考虑在 `agentdash-relay` 中新增更贴近 source resolver 的命令，或复用 `workspace_files` 能力并补足 project snapshot 所需信息
- 该任务完成后，应回头同步收敛 `crates/agentdash-injection` 中对本地文件系统的默认假设
