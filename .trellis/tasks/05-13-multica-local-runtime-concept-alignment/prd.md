# AgentDash 与 multica 本地运行时概念对齐学习

## Goal

建立一份面向后续设计与实现的学习任务，系统对齐 AgentDash 现有概念、目录结构与 `references/multica` 的核心概念、目录结构。目标不是只研究某一个模块，而是先做一份“概念与目录一一对应”的导航图，让后续可以按领域完整展开学习。

本地连接、本机运行时、daemon/local backend、桌面端前端与本机能力合并是重点专题之一；同时也需要覆盖云端服务、任务/Issue 协作、数据模型、实时事件、自动化、技能、通知、前端包结构、文档/部署/CLI 等其它主要能力。

这份任务的产出应帮助后续开发者快速回答：

- AgentDash 的 `agentdash-local` / Relay / SessionHub / VFS / BackendRegistry 分别对应 multica 的哪些 daemon、runtime、task queue、agent backend、workspace/repo cache 概念。
- multica 在云端服务、协作产品模型、实时事件、数据访问、自动化、本地 daemon 连接、运行时注册、心跳、离线恢复、任务认领、任务日志、工作目录管理、CLI/desktop 体验上有哪些值得 AgentDash 学习。
- 哪些设计可以吸收，哪些因为 AgentDash 已有 VFS、Hook Runtime、Lifecycle DAG、Pi Agent Loop 等更强抽象而不应照搬。

## Requirements

- 梳理 AgentDash 与 multica 的概念映射表与目录映射表，覆盖至少：
  - 项目/工作区/任务/会话/Agent/运行时/技能/自动化/通知/活动流。
  - AgentDash `local backend`、`cloud backend`、`relay protocol`、`SessionHub`、`VFS`、`Routine`、`Lifecycle` 与 multica `daemon`、`server`、`agent_runtime`、`agent_task_queue`、`workspace repo`、`autopilot` 的关系。
- 重点研究云端/服务端能力：
  - AgentDash：`agentdash-api`、`agentdash-application`、`agentdash-domain`、`agentdash-infrastructure`、`agentdash-mcp`、`agentdash-plugin-api`。
  - multica：`server/cmd/server`、`server/internal/handler`、`server/internal/service`、`server/internal/realtime`、`server/internal/events`、`server/pkg/db`、`server/migrations`。
  - 对比 API 边界、服务层边界、数据库查询/迁移、事件总线、实时推送、权限/成员/工作区隔离、通知与活动流。
- 重点研究本地连接链路：
  - AgentDash：`agentdash-local` 主循环、WebSocket 注册、能力上报、BackendRegistry、relay message、终端/工具/MCP/session event 回传。
  - multica：CLI/daemon 启动、runtime 注册、daemon heartbeat、task claim/start/progress/complete/fail、runtime gone 恢复、orphan task 恢复、workspace GC、repo/worktree cache。
- 重点研究本机 app / desktop 方向：
  - multica 如何通过 `apps/desktop`、`apps/web`、`packages/core`、`packages/views`、`packages/ui` 共享前端能力。
  - multica desktop 与 daemon/CLI/local runtime 的用户操作边界。
  - AgentDash 后续桌面端统一架构、local dashboard、前端与本机后端合并体验可以借鉴什么。
- 形成可执行学习结论：
  - “可直接学习”的工程机制。
  - “需按 AgentDash 架构改写”的机制。
  - “不建议学习/暂不适用”的机制。
  - 建议拆出的后续实现任务候选清单。
- 避免兼容性和回退方案思维；本项目仍处预研期，结论应偏向最正确的长期形态。

## Acceptance Criteria

- [ ] 产出一份研究文档，包含 AgentDash ↔ multica 核心概念映射表。
- [ ] 产出一份目录映射文档，按后端、前端、本机运行时、协议、数据层、自动化、文档/部署等维度列出两边目录和职责对应关系。
- [ ] 云端能力映射覆盖 API/handler、application/service、domain/model、database/migration、realtime/event、auth/workspace/membership、notification/activity。
- [ ] 产出本地连接链路对比图或分阶段描述，明确 local backend / daemon / runtime / relay / task queue 的对应关系。
- [ ] 产出桌面端与本机能力合并的专题分析，明确 multica 可参考点与 AgentDash 适配方式。
- [ ] 至少列出 5 个值得学习的机制，并为每个机制标注参考文件、AgentDash 对应模块、预期收益、改造风险。
- [ ] 至少列出 3 个不应直接照搬的差异点，说明原因。
- [ ] 形成后续任务拆分建议，优先级覆盖云端协作模型、数据/事件架构、runtime 健康恢复、任务日志/可观测性、前端实时状态同步、desktop/local 一体化体验。
- [ ] 不修改业务代码；本任务只沉淀研究、设计与后续计划。

## Notes

- 用户明确建议新建任务，不恢复之前删除的 `05-13-multica-reference-review` 目录。
- 本任务应优先复用 `references/multica` 本地源码，不需要联网。
- 重点不是评价 multica 整体产品，而是建立 AgentDash 后续系统性学习 multica 的“概念索引”和“目录索引”。
