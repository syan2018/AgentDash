# AgentRun lifecycle 与 companion 协议收束设计

## Goal

把 companion 交互协议、内嵌 SkillAsset bundle 投影、AgentRun lifecycle VFS、前端 runtime/capability 可见性收束到同一套 AgentRun-first 模型。

本任务只负责设计与实施计划。实现阶段应在设计被确认后再启动。

## Background

当前讨论暴露了几条相互关联的偏差：

- `companion_request` 的业务正文在不同路径里混用 `payload.prompt` 与 `payload.message`，而工具 schema 只声明开放 object，导致 Agent 容易生成错误字段。
- `companion-system` / `workspace-module-system` / `routine-memory` 等内嵌 bundle 已注册为项目级 builtin SkillAsset，但它们是否进入 runtime 可发现 skill surface 取决于 launch 路径是否恰好写入 lifecycle mount metadata。
- 当前代码同时存在 `node_runtime` 与 `agent_run_session` 两类 lifecycle mount 表达，部分 query 路径会用 AgentRun session-scope lifecycle mount 替换 frame typed VFS 中既有 lifecycle mount，从而丢失 `skill_asset_project_id` / `skill_asset_keys` metadata。
- 产品方向已经收敛为 AgentRun-first：用户工作台、命令、mailbox、frame、runtime trace 都以 AgentRun 控制面为事实源，RuntimeSession 只作为 delivery trace/ref。

## Requirements

- Companion payload 正文字段收束为 `payload.message`。
- `payload.prompt` 不再作为 companion request 的标准字段。
- `task`、`review`、`approval`、`notification` 的 request payload 都使用 `message` 表达正文。
- `capability_grant_request` 保持结构化字段：`requested_paths`、`reason`、`scope`。
- `companion_request` tool schema 必须显式描述已注册 payload type 的字段形态，减少模型猜测。
- `companion-system` skill 文档必须包含完整 payload matrix，覆盖 target、payload type、required fields、response type 与典型用途。
- AgentRun runtime surface 必须稳定包含 AgentRun lifecycle VFS surface。
- 内嵌 SkillAsset bundle 必须经 AgentRun lifecycle surface 投影，而不是依赖 active workflow 或旧 session path 的偶然分支。
- ProjectAgent graphless AgentRun、workflow node AgentRun、plain companion child、companion+workflow child、workspace query/resource surface 都必须遵循同一套 lifecycle skill projection 语义。
- 前端展示的 capability/resource surface 必须与执行器实际可见的 VFS/skill surface 来自同一 frame/runtime projection。
- 本项目尚未上线，实现阶段无需保留旧字段兼容或双读 fallback；数据库结构若需要调整，必须补 migration。

## Acceptance Criteria

- [ ] 设计文档明确 `message` 作为 companion request 正文字段的唯一 contract，并列出所有内置 request/response payload type。
- [ ] 设计文档明确 AgentRun lifecycle VFS 的 mount scope、metadata、skill projection、node artifact/record surface 的关系。
- [ ] 设计文档明确 builtin SkillAsset bundle 如何从 embedded bundle 同步到 Project SkillAsset，再投影到 AgentRun lifecycle runtime surface。
- [ ] 设计文档覆盖 ProjectAgent graphless、workflow node、plain companion child、companion+workflow child、workspace query 五条关键路径。
- [ ] 实施计划包含后端 contract/schema、frame construction、VFS lifecycle mount、skill docs、前端展示、测试与 spec 更新步骤。
- [ ] 实施计划列出后续实现前必须检查的迁移、测试和回归风险点。

## Scope

本任务覆盖设计与计划：

- Companion payload contract 收束。
- AgentRun lifecycle VFS 和 SkillAsset projection contract 收束。
- Frame construction / workspace query / companion launch 路径的设计收束。
- 前端 runtime capability/resource surface 可见性的设计收束。
- 后续实现步骤拆解。

## Out Of Scope

- 直接修改生产代码。
- 启动或完成实现阶段。
- 处理与 companion/lifecycle/SkillAsset projection 无关的 UI 重构。
- 重新定义 AgentRun mailbox command 模型。

## Planning Status

- 当前状态：planning。
- 设计产物：`design.md`。
- 执行计划：`implement.md`。
- 下一步：用户确认设计方向后，进入实现任务或拆分子任务。
